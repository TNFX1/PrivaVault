//! PrivaVault desktop app: a thin command layer between the web UI and `privavault-core`.
//!
//! Security-relevant design points:
//! * **Output paths never come from the web UI.** Every destination (vault file, extracted file,
//!   extraction folder, ZIP) is chosen by the user in a native dialog opened *here*. Even a
//!   compromised renderer cannot make the app write to an arbitrary path silently.
//! * Heavy work runs on blocking worker threads; the UI thread never derives keys or does I/O.
//! * One operation at a time; cancellable; partial outputs are always cleaned up.
//! * The opened vault (and its derived key) is held only until the user locks it, the idle
//!   auto-lock fires, or the app exits.

mod platform;
mod settings;
mod update;

use privavault_core::{create_vault, detect_format, EntryInfo, Format, Job, Progress, Vault};
use serde::Serialize;
use settings::{Settings, Theme};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

const RELEASES_URL: &str = "https://github.com/TNFX1/PrivaVault/releases";

struct Session {
    path: PathBuf,
    vault: Vault,
}

struct AppState {
    config_dir: Option<PathBuf>,
    settings: Mutex<Settings>,
    session: Mutex<Option<Session>>,
    busy: AtomicBool,
    cancel: Arc<AtomicBool>,
    quit_requested: AtomicBool,
    launch_paths: Mutex<Vec<PathBuf>>,
    last_output: Mutex<Option<PathBuf>>,
    /// Desktop light/dark preference (Linux: XDG portal; `None` = unknown / no preference).
    system_dark: Mutex<Option<bool>>,
    /// Update found by the last check, ready for `install_update`.
    pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
}

/// Error shape sent to the UI: a stable code for localisation plus a technical detail line.
#[derive(Debug, Serialize)]
struct CmdError {
    code: String,
    detail: String,
}

impl From<privavault_core::Error> for CmdError {
    fn from(e: privavault_core::Error) -> Self {
        CmdError { code: e.code().into(), detail: e.to_string() }
    }
}

fn err(code: &str, detail: impl Into<String>) -> CmdError {
    CmdError { code: code.into(), detail: detail.into() }
}

type CmdResult<T> = Result<T, CmdError>;

/// Marks the app busy for the lifetime of one operation.
struct BusyGuard(AppHandle);

impl BusyGuard {
    fn acquire(app: &AppHandle) -> CmdResult<BusyGuard> {
        let st = app.state::<AppState>();
        if st.busy.swap(true, Ordering::SeqCst) {
            return Err(err("busy", "another operation is running"));
        }
        st.cancel.store(false, Ordering::SeqCst);
        Ok(BusyGuard(app.clone()))
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        let st = self.0.state::<AppState>();
        st.busy.store(false, Ordering::SeqCst);
        // The user asked to quit during the operation: it has now been cancelled and its
        // partial output removed, so exiting is safe.
        if st.quit_requested.load(Ordering::SeqCst) {
            self.0.exit(0);
        }
    }
}

/// Run `f` on a blocking thread with a progress emitter (≤ 10 events/s) and the cancel flag.
async fn run_job<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&AppHandle, &Job) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = BusyGuard::acquire(&app2)?;
        let cancel = app2.state::<AppState>().cancel.clone();
        let last = Cell::new(Instant::now() - Duration::from_secs(1));
        let emitter = app2.clone();
        let progress = move |p: Progress| {
            let now = Instant::now();
            if now.duration_since(last.get()) >= Duration::from_millis(100) || p.done == p.total {
                last.set(now);
                let _ = emitter.emit("progress", p);
            }
        };
        let job = Job::new(&cancel, &progress);
        f(&app2, &job)
    })
    .await
    .map_err(|e| err("internal", e.to_string()))?
}

fn window(app: &AppHandle) -> CmdResult<WebviewWindow> {
    app.get_webview_window("main").ok_or_else(|| err("internal", "main window missing"))
}

fn to_path(fp: tauri_plugin_dialog::FilePath) -> CmdResult<PathBuf> {
    fp.into_path().map_err(|e| err("internal", e.to_string()))
}

/// Ask the user where to save a file. Returns `None` if the dialog was dismissed.
fn ask_save_path(
    app: &AppHandle,
    w: &WebviewWindow,
    name: &str,
    dir: Option<PathBuf>,
    filter: Option<(&str, &str)>,
) -> CmdResult<Option<PathBuf>> {
    if let Some(d) = e2e_output_dir() {
        return Ok(Some(d.join(name)));
    }
    let mut d = app.dialog().file().set_parent(w).set_file_name(name);
    if let Some((label, ext)) = filter {
        d = d.add_filter(label, &[ext]);
    }
    if let Some(dir) = dir {
        d = d.set_directory(dir);
    }
    d.blocking_save_file().map(to_path).transpose()
}

/// Ask the user for a destination folder. Returns `None` if the dialog was dismissed.
fn ask_folder(app: &AppHandle, w: &WebviewWindow, dir: Option<PathBuf>) -> CmdResult<Option<PathBuf>> {
    if let Some(d) = e2e_output_dir() {
        return Ok(Some(d));
    }
    let mut d = app.dialog().file().set_parent(w);
    if let Some(dir) = dir {
        d = d.set_directory(dir);
    }
    d.blocking_pick_folder().map(to_path).transpose()
}

/// GUI end-to-end tests (tests/gui) cannot click native dialogs. In **debug builds only**, this
/// env var makes every save/folder dialog answer with a path inside the given directory.
/// The code does not exist in release builds.
#[cfg(debug_assertions)]
fn e2e_output_dir() -> Option<PathBuf> {
    std::env::var_os("PRIVAVAULT_E2E_OUTPUT_DIR").map(PathBuf::from)
}

#[cfg(not(debug_assertions))]
fn e2e_output_dir() -> Option<PathBuf> {
    None
}

/// Keep only a plain file name (no directories) for dialog suggestions coming from the UI.
fn safe_file_name(s: &str, fallback: &str) -> String {
    let base = s.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let cleaned: String = base.chars().filter(|c| !c.is_control() && !":*?\"<>|".contains(*c)).collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        fallback.to_string()
    } else {
        cleaned
    }
}

// ---------------------------------------------------------------------------------------------
// Info & settings
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    environment: platform::Environment,
    config_dir: Option<PathBuf>,
    releases_url: &'static str,
    /// Desktop prefers dark (Linux portal). `None`: let the WebView's media query decide.
    system_dark: Option<bool>,
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        environment: platform::environment(|k| std::env::var(k).ok()),
        config_dir: state.config_dir.clone(),
        releases_url: RELEASES_URL,
        system_dark: *state.system_dark.lock().unwrap(),
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let s = settings.sanitized();
    if let Some(dir) = &state.config_dir {
        s.save(dir).map_err(|e| err("io", e))?;
    }
    apply_theme(&app, s.theme);
    *state.settings.lock().unwrap() = s.clone();
    Ok(s)
}

/// Apply the theme to the native window (title bar, GTK widgets, WebKit's color scheme).
///
/// "System" is resolved explicitly when the desktop preference is known: on Linux, passing
/// `None` would make tao clear GTK's prefer-dark flag and force a light scheme on dark desktops.
/// On Windows/macOS `None` correctly follows the OS, including live changes.
fn apply_theme(app: &AppHandle, theme: Theme) {
    let Some(w) = app.get_webview_window("main") else { return };
    let system = *app.state::<AppState>().system_dark.lock().unwrap();
    let dark = match theme {
        Theme::Light => Some(false),
        Theme::Dark => Some(true),
        Theme::System if cfg!(target_os = "linux") => system,
        Theme::System => None,
    };
    let _ = w.set_theme(dark.map(|d| if d { tauri::Theme::Dark } else { tauri::Theme::Light }));
}

#[tauri::command]
fn take_launch_paths(state: State<'_, AppState>) -> Vec<PathInfo> {
    let paths = std::mem::take(&mut *state.launch_paths.lock().unwrap());
    paths.iter().map(|p| path_info(p)).collect()
}

// ---------------------------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct PathInfo {
    path: PathBuf,
    name: String,
    is_dir: bool,
    /// File size, or total size of regular files inside a folder.
    size: u64,
    /// Number of files (1 for a file).
    files: usize,
    /// Looks like a vault (v2 magic, or a known vault extension for v1).
    is_vault: bool,
}

fn path_info(p: &Path) -> PathInfo {
    let name = privavault_core::paths::display_name(p);
    let is_dir = p.is_dir();
    let (size, files) = if is_dir {
        privavault_core::v2::collect_inputs(&[p.to_path_buf()])
            .map(|c| (c.total_bytes, c.sources.iter().filter(|s| s.is_some()).count()))
            .unwrap_or((0, 0))
    } else {
        (std::fs::metadata(p).map(|m| m.len()).unwrap_or(0), 1)
    };
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let is_vault = !is_dir
        && (matches!(detect_format(p), Ok(Format::V2))
            || (settings::EXTENSIONS.contains(&ext.as_str()) && detect_format(p).is_ok()));
    PathInfo { path: p.to_path_buf(), name, is_dir, size, files, is_vault }
}

/// Describe paths dropped onto the window or passed on the command line.
#[tauri::command]
async fn inspect_paths(paths: Vec<PathBuf>) -> CmdResult<Vec<PathInfo>> {
    tauri::async_runtime::spawn_blocking(move || paths.iter().filter(|p| p.exists()).map(|p| path_info(p)).collect())
        .await
        .map_err(|e| err("internal", e.to_string()))
}

#[tauri::command]
async fn pick_inputs(app: AppHandle, folders: bool) -> CmdResult<Vec<PathInfo>> {
    let w = window(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let d = app.dialog().file().set_parent(&w);
        let picked = if folders { d.blocking_pick_folders() } else { d.blocking_pick_files() };
        let paths: Vec<PathBuf> = picked.unwrap_or_default().into_iter().filter_map(|p| to_path(p).ok()).collect();
        Ok(paths.iter().map(|p| path_info(p)).collect())
    })
    .await
    .map_err(|e| err("internal", e.to_string()))?
}

#[tauri::command]
async fn pick_vault(app: AppHandle) -> CmdResult<Option<PathInfo>> {
    let w = window(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_parent(&w)
            .add_filter("PrivaVault", settings::EXTENSIONS)
            .add_filter("*", &["*"])
            .blocking_pick_file();
        Ok(picked.and_then(|p| to_path(p).ok()).map(|p| path_info(&p)))
    })
    .await
    .map_err(|e| err("internal", e.to_string()))?
}

// ---------------------------------------------------------------------------------------------
// Encrypt
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct EncryptResult {
    output: PathBuf,
    files: usize,
    folders: usize,
    bytes: u64,
    skipped: Vec<PathBuf>,
    originals_deleted: bool,
    left_behind: Vec<PathBuf>,
}

#[tauri::command]
async fn encrypt(
    app: AppHandle,
    paths: Vec<PathBuf>,
    password: String,
    suggested_name: String,
    delete_originals: Option<bool>,
) -> CmdResult<Option<EncryptResult>> {
    let password = Zeroizing::new(password);
    if paths.is_empty() {
        return Err(err("empty_input", "nothing selected"));
    }
    let (ext, kdf) = {
        let st = app.state::<AppState>();
        let s = st.settings.lock().unwrap();
        (s.default_extension.clone(), s.strength.params())
    };
    let name = safe_file_name(&suggested_name, "vault");
    let name = if name.to_ascii_lowercase().ends_with(&format!(".{ext}")) { name } else { format!("{name}.{ext}") };
    let w = window(&app)?;
    let dir = paths[0].parent().map(Path::to_path_buf);
    run_job(&app, move |app, job| {
        let Some(out) = ask_save_path(app, &w, &name, dir, Some(("PrivaVault", ext.as_str())))? else {
            return Ok(None);
        };
        let rep = create_vault(&paths, &out, &password, kdf, job)?;
        *app.state::<AppState>().last_output.lock().unwrap() = Some(rep.output.clone());
        // Originals go only after the new vault reopens with this password and every byte checks out.
        let left = if delete_originals.unwrap_or(false) {
            Vault::open(&rep.output, &password, job)?.verify(job)?;
            Some(privavault_core::remove_originals(&paths, &rep.encrypted))
        } else {
            None
        };
        Ok(Some(EncryptResult {
            output: rep.output,
            files: rep.files,
            folders: rep.folders,
            bytes: rep.bytes,
            skipped: rep.skipped,
            originals_deleted: left.is_some(),
            left_behind: left.unwrap_or_default(),
        }))
    })
    .await
}

// ---------------------------------------------------------------------------------------------
// Open / extract
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct OpenResult {
    path: PathBuf,
    name: String,
    size: u64,
    format: Format,
    kdf: String,
    entries: Vec<EntryInfo>,
}

#[tauri::command]
async fn open_vault(app: AppHandle, path: PathBuf, password: String) -> CmdResult<OpenResult> {
    let password = Zeroizing::new(password);
    run_job(&app, move |app, job| {
        let st = app.state::<AppState>();
        st.session.lock().unwrap().take(); // lock any previously open vault first
        let vault = Vault::open(&path, &password, job)?;
        let res = OpenResult {
            name: privavault_core::paths::display_name(&path),
            size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            format: vault.format(),
            kdf: vault.kdf_description(),
            entries: vault.entries().to_vec(),
            path: path.clone(),
        };
        *st.session.lock().unwrap() = Some(Session { path, vault });
        Ok(res)
    })
    .await
}

/// Forget the open vault and its key.
/// Async so it never blocks the main (UI) thread while an operation holds the session.
#[tauri::command]
async fn lock_vault(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().session.lock().unwrap().take();
    })
    .await;
}

/// Add files to / remove entries from the open vault, or give it a new password. The vault is
/// rewritten (fresh salt and nonce) next to itself and replaces the old file only when complete.
/// `password` is the current one, checked first so a typo can't silently re-key the vault.
#[tauri::command]
async fn edit_vault(
    app: AppHandle,
    password: String,
    new_password: Option<String>,
    remove: Vec<String>,
    add: Vec<PathBuf>,
) -> CmdResult<OpenResult> {
    let password = Zeroizing::new(password);
    let new_password = new_password.map(Zeroizing::new);
    let kdf = app.state::<AppState>().settings.lock().unwrap().strength.params();
    run_job(&app, move |app, job| {
        let st = app.state::<AppState>();
        let path = st
            .session
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.path.clone())
            .ok_or_else(|| err("locked", "no vault is open"))?;
        let mut source = Vault::open(&path, &password, job)?; // wrong password → WrongPassword, nothing written
        let final_pw: &str = new_password.as_deref().map(|p| p.as_str()).unwrap_or(&password);
        source.rewrite(&path, final_pw, kdf, &remove, &add, job)?;
        drop(source);
        st.session.lock().unwrap().take();
        let vault = Vault::open(&path, final_pw, job)?;
        let res = OpenResult {
            name: privavault_core::paths::display_name(&path),
            size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            format: vault.format(),
            kdf: vault.kdf_description(),
            entries: vault.entries().to_vec(),
            path: path.clone(),
        };
        *st.session.lock().unwrap() = Some(Session { path, vault });
        Ok(res)
    })
    .await
}

/// Decrypt and authenticate the whole open vault without writing anything.
#[tauri::command]
async fn verify_vault(app: AppHandle) -> CmdResult<u64> {
    run_job(&app, move |app, job| with_session(app, |s| Ok(s.vault.verify(job)?))).await
}

const PREVIEW_MAX: u64 = 25 * 1024 * 1024;

/// One file's bytes for an in-memory preview (images, text). Nothing touches the disk.
#[tauri::command]
async fn preview_entry(app: AppHandle, index: usize) -> CmdResult<tauri::ipc::Response> {
    run_job(&app, move |app, job| {
        with_session(app, |s| {
            let bytes = s.vault.read_entry(index, PREVIEW_MAX, job)?;
            Ok(tauri::ipc::Response::new(bytes.to_vec()))
        })
    })
    .await
}

fn with_session<T>(app: &AppHandle, f: impl FnOnce(&mut Session) -> CmdResult<T>) -> CmdResult<T> {
    let st = app.state::<AppState>();
    let mut guard = st.session.lock().unwrap();
    let s = guard.as_mut().ok_or_else(|| err("locked", "no vault is open"))?;
    f(s)
}

#[derive(Serialize)]
struct ExtractResult {
    output: PathBuf,
    files: usize,
    folders: usize,
    bytes: u64,
    skipped_unsafe: Vec<String>,
}

#[tauri::command]
async fn extract_entry(app: AppHandle, index: usize) -> CmdResult<Option<ExtractResult>> {
    let w = window(&app)?;
    run_job(&app, move |app, job| {
        let (name, dir) = with_session(app, |s| {
            let e = s.vault.entries().get(index).ok_or_else(|| err("entry_not_found", "bad index"))?;
            let n = e.path.rsplit(['/', '\\']).next().unwrap_or("file").to_string();
            Ok((safe_file_name(&n, "file"), s.path.parent().map(Path::to_path_buf)))
        })?;
        let Some(dest) = ask_save_path(app, &w, &name, dir, None)? else { return Ok(None) };
        let out = with_session(app, |s| Ok(s.vault.extract_entry(index, &dest, job)?))?;
        let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        *app.state::<AppState>().last_output.lock().unwrap() = Some(out.clone());
        Ok(Some(ExtractResult { output: out, files: 1, folders: 0, bytes, skipped_unsafe: vec![] }))
    })
    .await
}

#[tauri::command]
async fn extract_all(app: AppHandle) -> CmdResult<Option<ExtractResult>> {
    let w = window(&app)?;
    run_job(&app, move |app, job| {
        let (stem, dir) = with_session(app, |s| {
            let stem = s.path.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "vault".into());
            Ok((safe_file_name(&stem, "vault"), s.path.parent().map(Path::to_path_buf)))
        })?;
        let Some(parent) = ask_folder(app, &w, dir)? else { return Ok(None) };
        let rep = with_session(app, |s| Ok(s.vault.extract_all(&parent, &stem, job)?))?;
        *app.state::<AppState>().last_output.lock().unwrap() = Some(rep.root.clone());
        Ok(Some(ExtractResult {
            output: rep.root,
            files: rep.files,
            folders: rep.folders,
            bytes: rep.bytes,
            skipped_unsafe: rep.skipped_unsafe,
        }))
    })
    .await
}

#[tauri::command]
async fn export_zip(app: AppHandle) -> CmdResult<Option<ExtractResult>> {
    let w = window(&app)?;
    run_job(&app, move |app, job| {
        let (stem, dir) = with_session(app, |s| {
            let stem = s.path.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| "vault".into());
            Ok((safe_file_name(&stem, "vault"), s.path.parent().map(Path::to_path_buf)))
        })?;
        let Some(dest) = ask_save_path(app, &w, &format!("{stem}.zip"), dir, Some(("ZIP", "zip")))? else {
            return Ok(None);
        };
        let rep = with_session(app, |s| Ok(s.vault.export_zip(&dest, job)?))?;
        *app.state::<AppState>().last_output.lock().unwrap() = Some(rep.root.clone());
        Ok(Some(ExtractResult {
            output: rep.root,
            files: rep.files,
            folders: rep.folders,
            bytes: rep.bytes,
            skipped_unsafe: rep.skipped_unsafe,
        }))
    })
    .await
}

#[tauri::command]
fn cancel(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

/// Quit, cancelling a running operation first so its partial output is removed.
#[tauri::command]
fn quit_app(app: AppHandle, state: State<'_, AppState>) {
    state.session.lock().unwrap().take();
    if state.busy.load(Ordering::SeqCst) {
        state.quit_requested.store(true, Ordering::SeqCst);
        state.cancel.store(true, Ordering::SeqCst);
    } else {
        app.exit(0);
    }
}

// ---------------------------------------------------------------------------------------------
// Desktop integration
// ---------------------------------------------------------------------------------------------

/// Show the most recent output in the system file manager (Dolphin, Explorer, …).
/// Only paths this app produced can be revealed.
#[tauri::command]
fn reveal_output(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let p = state.last_output.lock().unwrap().clone().ok_or_else(|| err("io_not_found", "nothing to show"))?;
    app.opener().reveal_item_in_dir(p).map_err(|e| err("io", e.to_string()))
}

#[tauri::command]
fn open_releases(app: AppHandle, url: Option<String>) -> CmdResult<()> {
    let url = url.filter(|u| u.starts_with(&format!("{RELEASES_URL}/"))).unwrap_or_else(|| RELEASES_URL.into());
    app.opener().open_url(url, None::<&str>).map_err(|e| err("io", e.to_string()))
}

// ---------------------------------------------------------------------------------------------
// Updates (see update.rs). Network access only with consent: the setting, or "Check now".
// ---------------------------------------------------------------------------------------------

#[tauri::command]
async fn check_update(app: AppHandle, manual: bool) -> CmdResult<Option<update::Available>> {
    let allowed = manual || app.state::<AppState>().settings.lock().unwrap().check_updates;
    if !allowed || platform::environment(|k| std::env::var(k).ok()).store_managed {
        return Ok(None);
    }
    match update::check(&app).await {
        Ok(Some((info, pending))) => {
            *app.state::<AppState>().pending_update.lock().unwrap() = pending;
            Ok(Some(info))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(err("update_check_failed", e)),
    }
}

#[tauri::command]
async fn install_update(app: AppHandle) -> CmdResult<()> {
    let _guard = BusyGuard::acquire(&app)?;
    let st = app.state::<AppState>();
    let pending = st.pending_update.lock().unwrap().take().ok_or_else(|| err("update_unavailable", "check first"))?;
    // Never leave a key in memory while the program is being replaced.
    st.session.lock().unwrap().take();
    let emitter = app.clone();
    let last = Mutex::new(Instant::now() - Duration::from_secs(1));
    let progress = move |p: update::Progress| {
        let mut l = last.lock().unwrap();
        if l.elapsed() >= Duration::from_millis(100) || p.done == p.total {
            *l = Instant::now();
            let _ = emitter.emit("update-progress", p);
        }
    };
    update::install(pending, update::method(), progress).await.map_err(|e| {
        if e == "cancelled" {
            err("update_cancelled", e)
        } else {
            err("update_failed", e)
        }
    })?;
    app.restart()
}

#[tauri::command]
fn notify(app: AppHandle, state: State<'_, AppState>, title: String, body: String) {
    if state.settings.lock().unwrap().notifications {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

pub fn run() {
    platform::apply_linux_webview_workarounds();

    let config_dir = platform::config_dir(platform::Os::current(), |k| std::env::var(k).ok());
    let settings = config_dir.as_deref().map(Settings::load).unwrap_or_default();
    let cwd = std::env::current_dir().ok();
    let launch = platform::launch_paths(std::env::args(), cwd.as_deref());
    #[cfg(target_os = "linux")]
    let system_dark = platform::appearance::prefers_dark();
    #[cfg(not(target_os = "linux"))]
    let system_dark = None;

    tauri::Builder::default()
        // Must be first: a second launch (e.g. double-clicking another vault) forwards its
        // arguments to the running window instead of starting a new process.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let paths = platform::launch_paths(args, Some(Path::new(&cwd)));
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            if !paths.is_empty() {
                let _ = app.emit("open-paths", paths);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            config_dir,
            settings: Mutex::new(settings.clone()),
            session: Mutex::new(None),
            busy: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            quit_requested: AtomicBool::new(false),
            launch_paths: Mutex::new(launch),
            last_output: Mutex::new(None),
            system_dark: Mutex::new(system_dark),
            pending_update: Mutex::new(None),
        })
        .setup(move |app| {
            apply_theme(app.handle(), settings.theme);
            #[cfg(target_os = "linux")]
            {
                let handle = app.handle().clone();
                platform::appearance::watch(move |dark| {
                    let st = handle.state::<AppState>();
                    *st.system_dark.lock().unwrap() = dark;
                    let theme = st.settings.lock().unwrap().theme;
                    apply_theme(&handle, theme);
                    let _ = handle.emit("system-theme", dark);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let st = window.state::<AppState>();
                if st.busy.load(Ordering::SeqCst) {
                    // Let the UI ask "cancel and quit?" instead of killing a write midway.
                    api.prevent_close();
                    let _ = window.emit("close-requested", ());
                } else {
                    st.session.lock().unwrap().take();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            save_settings,
            take_launch_paths,
            inspect_paths,
            pick_inputs,
            pick_vault,
            encrypt,
            open_vault,
            lock_vault,
            edit_vault,
            verify_vault,
            preview_entry,
            extract_entry,
            extract_all,
            export_zip,
            cancel,
            quit_app,
            reveal_output,
            open_releases,
            notify,
            check_update,
            install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PrivaVault");
}

#[cfg(test)]
mod tests {
    use super::safe_file_name;

    #[test]
    fn suggested_names_cannot_carry_paths() {
        assert_eq!(safe_file_name("../../etc/passwd", "x"), "passwd");
        assert_eq!(safe_file_name("C:\\Windows\\evil.exe", "x"), "evil.exe");
        assert_eq!(safe_file_name("", "vault"), "vault");
        assert_eq!(safe_file_name("..", "vault"), "vault");
        assert_eq!(safe_file_name("a:b*c?.txt", "x"), "abc.txt");
        assert_eq!(safe_file_name("Tatil fotoğrafları", "x"), "Tatil fotoğrafları");
    }
}
