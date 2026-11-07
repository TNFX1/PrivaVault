//! One-click updates.
//!
//! The release manifest (`latest.json`, one per release) lists a download URL and a **minisign
//! signature** for every installable package. Packages are always verified against the public key
//! compiled into the app (`plugins.updater.pubkey`) before anything is installed, and signatures are
//! bound to the version (`requireSignedVersion`) so an old release can't be replayed as "new".
//!
//! | Installed as            | Manifest key              | Installed by                              |
//! | ----------------------- | ------------------------- | ----------------------------------------- |
//! | Windows installer       | `windows-x86_64-nsis`     | updater plugin (passive installer)        |
//! | Linux AppImage          | `linux-x86_64-appimage`   | updater plugin (replaces the AppImage)    |
//! | Debian/Ubuntu `.deb`    | `linux-x86_64-deb`        | updater plugin (`pkexec dpkg -i`)         |
//! | Fedora/openSUSE `.rpm`  | `linux-x86_64-rpm`        | updater plugin (`pkexec rpm -U`)          |
//! | Arch/CachyOS package    | `linux-x86_64-pacman`     | here: `pkexec pacman -U` (polkit prompt)  |
//! | Windows portable, other | —                         | not installable: open the release page    |
//!
//! There is deliberately no generic `linux-x86_64` key: an installation whose type is unknown
//! finds no update instead of receiving the wrong kind of package.
//!
//! Privacy: nothing here runs unless the user enabled "Check for updates" or pressed
//! "Check now" — both explicit consent.

use serde::Serialize;
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Nsis,
    AppImage,
    Deb,
    Rpm,
    Pacman,
    /// Can't update in place (portable exe, unknown install): show the release page.
    Manual,
}

/// Set by packaging/arch/PKGBUILD at build time (`PRIVAVAULT_PACKAGE=pacman`).
const PACKAGE: Option<&str> = option_env!("PRIVAVAULT_PACKAGE");

pub fn method() -> Method {
    if PACKAGE == Some("pacman") {
        return Method::Pacman;
    }
    use tauri::utils::config::BundleType;
    match tauri::utils::platform::bundle_type() {
        Some(BundleType::Nsis) if windows_installed() => Method::Nsis,
        Some(BundleType::AppImage) if std::env::var_os("APPIMAGE").is_some() => Method::AppImage,
        Some(BundleType::Deb) => Method::Deb,
        Some(BundleType::Rpm) => Method::Rpm,
        _ => Method::Manual,
    }
}

/// The Windows installer installs per-user into `%LOCALAPPDATA%\PrivaVault`. The portable exe runs
/// from anywhere else; running the installer for it would create a second copy, not update it.
fn windows_installed() -> bool {
    let (Ok(exe), Some(local)) = (std::env::current_exe(), std::env::var_os("LOCALAPPDATA")) else {
        return false;
    };
    let expected = std::path::Path::new(&local).join("PrivaVault");
    exe.parent().is_some_and(|dir| dir.to_string_lossy().eq_ignore_ascii_case(&expected.to_string_lossy()))
}

#[derive(Debug, Serialize)]
pub struct Available {
    pub version: String,
    pub notes: Option<String>,
    pub method: Method,
    /// True when the button can install it; otherwise the UI opens the release page.
    pub can_install: bool,
}

pub async fn check(app: &tauri::AppHandle) -> Result<Option<(Available, Option<Update>)>, String> {
    let method = method();
    let mut builder = app.updater_builder();
    if method == Method::Pacman {
        builder = builder.target(format!("linux-{}-pacman", std::env::consts::ARCH));
    }
    // GUI tests only (debug builds): point the updater at a local manifest. Absent from release builds.
    #[cfg(debug_assertions)]
    if let Some(url) = std::env::var("PRIVAVAULT_E2E_UPDATE_ENDPOINT").ok().and_then(|u| u.parse().ok()) {
        builder = builder.endpoints(vec![url]).map_err(|e| e.to_string())?;
        if let Ok(key) = std::env::var("PRIVAVAULT_E2E_UPDATE_PUBKEY") {
            builder = builder.pubkey(key); // throwaway test key, so tests never need the real secret
        }
    }
    let updater = builder.build().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let can_install = method != Method::Manual;
    let info = Available { version: update.version.clone(), notes: update.body.clone(), method, can_install };
    Ok(Some((info, can_install.then_some(update))))
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Downloading,
    Installing,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
}

/// Download (signature-verified by the plugin) and install. On success the caller restarts the app.
pub async fn install(update: Update, method: Method, progress: impl Fn(Progress) + Send + 'static) -> Result<(), String> {
    let mut done = 0u64;
    let on_chunk = {
        let progress = &progress;
        move |n: usize, total: Option<u64>| {
            done += n as u64;
            progress(Progress { phase: Phase::Downloading, done, total: total.unwrap_or(0) });
        }
    };
    match method {
        Method::Pacman => {
            // `download` verifies the minisign signature before returning the bytes.
            let bytes = update.download(on_chunk, || {}).await.map_err(|e| e.to_string())?;
            progress(Progress { phase: Phase::Installing, done: 0, total: 0 });
            tauri::async_runtime::spawn_blocking(move || install_with_pacman(&bytes))
                .await
                .map_err(|e| e.to_string())?
        }
        Method::Manual => Err("manual".into()),
        _ => {
            let mut installing = false;
            let progress_ref = &progress;
            update
                .download_and_install(on_chunk, || {
                    if !installing {
                        installing = true;
                        progress_ref(Progress { phase: Phase::Installing, done: 0, total: 0 });
                    }
                })
                .await
                .map_err(|e| e.to_string())
        }
    }
}

/// Install an Arch package through polkit. The package bytes were signature-checked already;
/// they are written to a fresh private directory just for pacman to read.
fn install_with_pacman(bytes: &[u8]) -> Result<(), String> {
    let mut rnd = [0u8; 8];
    getrandom::fill(&mut rnd).map_err(|e| e.to_string())?;
    let name: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("privavault-update-{name}"));
    privavault_core::fsutil::create_private_dir_all(&dir).map_err(|e| e.to_string())?;
    let pkg = dir.join("privavault-update.pkg.tar.zst");
    let result = (|| {
        std::fs::write(&pkg, bytes).map_err(|e| e.to_string())?;
        let status = std::process::Command::new("pkexec")
            .args(["pacman", "-U", "--noconfirm", "--needed"])
            .arg(&pkg)
            .status()
            .map_err(|e| format!("pkexec: {e}"))?;
        match status.code() {
            Some(0) => Ok(()),
            // 126: the user dismissed the password dialog; 127: not authorized.
            Some(126) | Some(127) => Err("cancelled".into()),
            _ => Err(format!("pacman exited with {status}")),
        }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_installs_are_manual() {
        // Test binaries are not patched with a bundle type and are not built by the PKGBUILD.
        if PACKAGE.is_none() {
            assert_eq!(method(), Method::Manual);
        }
    }
}
