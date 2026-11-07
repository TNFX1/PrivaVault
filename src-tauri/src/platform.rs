//! Platform-specific behaviour, kept out of the command layer.
//!
//! Everything here is a pure function of explicit inputs where possible so it can be unit-tested
//! on any OS (e.g. Windows path rules are tested on Linux CI and vice versa).

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const APP_DIR_UNIX: &str = "privavault";
pub const APP_DIR_WINDOWS: &str = "PrivaVault";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Linux,
    Windows,
    Macos,
    Other,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else {
            Os::Other
        }
    }
}

/// Where settings live.
///
/// * Linux/BSD: `$XDG_CONFIG_HOME/privavault`, falling back to `~/.config/privavault`
///   (XDG Base Directory spec: relative `XDG_CONFIG_HOME` values must be ignored).
/// * Windows: `%APPDATA%\PrivaVault` (roaming profile, per-user ACL).
/// * macOS: `~/Library/Application Support/PrivaVault`.
pub fn config_dir(os: Os, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    // Judge absoluteness by the *target* OS rules so both are testable on any host.
    let abs = |v: Option<String>| {
        v.filter(|s| !s.is_empty()).map(PathBuf::from).filter(|p| match os {
            Os::Windows => looks_absolute_windows(p),
            _ => p.to_string_lossy().starts_with('/'),
        })
    };
    match os {
        Os::Windows => abs(env("APPDATA")).map(|p| p.join(APP_DIR_WINDOWS)),
        Os::Macos => abs(env("HOME")).map(|h| h.join("Library/Application Support").join(APP_DIR_WINDOWS)),
        Os::Linux | Os::Other => abs(env("XDG_CONFIG_HOME"))
            .or_else(|| abs(env("HOME")).map(|h| h.join(".config")))
            .map(|p| p.join(APP_DIR_UNIX)),
    }
}

fn looks_absolute_windows(p: &Path) -> bool {
    // Allows testing Windows env values on a Unix host ("C:\Users\x\AppData\Roaming").
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || s.starts_with("\\\\")
}

#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    pub os: Os,
    /// "wayland", "x11", "windows", …
    pub session: String,
    /// e.g. "KDE", "GNOME" (Linux only).
    pub desktop: Option<String>,
}

pub fn environment(env: impl Fn(&str) -> Option<String>) -> Environment {
    let os = Os::current();
    let session = match os {
        Os::Linux | Os::Other => env("XDG_SESSION_TYPE")
            .filter(|s| !s.is_empty())
            .or_else(|| env("WAYLAND_DISPLAY").map(|_| "wayland".into()))
            .or_else(|| env("DISPLAY").map(|_| "x11".into()))
            .unwrap_or_else(|| "unknown".into()),
        Os::Windows => "windows".into(),
        Os::Macos => "macos".into(),
    };
    Environment { os, session, desktop: env("XDG_CURRENT_DESKTOP").filter(|s| !s.is_empty()) }
}

/// Paths passed on the command line ("Open with PrivaVault", file-manager double-click,
/// or forwarded from a second instance). Flags and non-existent paths are ignored.
pub fn launch_paths<I: IntoIterator<Item = String>>(args: I, cwd: Option<&Path>) -> Vec<PathBuf> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|a| {
            // Desktop files may pass file:// URIs (%U).
            let a = a.strip_prefix("file://").map(percent_decode).unwrap_or(a);
            let p = PathBuf::from(a);
            match (p.is_absolute(), cwd) {
                (false, Some(c)) => c.join(p),
                _ => p,
            }
        })
        .filter(|p| p.exists())
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Apply WebKitGTK workarounds before the webview starts (Linux only).
///
/// WebKitGTK's DMA-BUF renderer is known to fail on the proprietary NVIDIA driver under Wayland
/// (blank window / "Error 71 (Protocol error)"). Only when that driver is loaded and the user has
/// not chosen otherwise do we fall back to the slower but reliable path.
pub fn apply_linux_webview_workarounds() {
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let nvidia = Path::new("/proc/driver/nvidia/version").exists();
        if wayland && nvidia && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            // Called at the very start of main(), before any other thread exists.
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn linux_config_dir_follows_xdg() {
        let e = env(&[("XDG_CONFIG_HOME", "/home/u/.cfg"), ("HOME", "/home/u")]);
        assert_eq!(config_dir(Os::Linux, e), Some(PathBuf::from("/home/u/.cfg/privavault")));
        let e = env(&[("HOME", "/home/u")]);
        assert_eq!(config_dir(Os::Linux, e), Some(PathBuf::from("/home/u/.config/privavault")));
        // Relative XDG values are invalid per spec and must be ignored.
        let e = env(&[("XDG_CONFIG_HOME", "relative/dir"), ("HOME", "/home/u")]);
        assert_eq!(config_dir(Os::Linux, e), Some(PathBuf::from("/home/u/.config/privavault")));
        let e = env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/u")]);
        assert_eq!(config_dir(Os::Linux, e), Some(PathBuf::from("/home/u/.config/privavault")));
        assert_eq!(config_dir(Os::Linux, env(&[])), None);
    }

    #[test]
    fn windows_config_dir_uses_appdata() {
        let e = env(&[("APPDATA", r"C:\Users\u\AppData\Roaming")]);
        let p = config_dir(Os::Windows, e).unwrap();
        assert!(p.to_string_lossy().starts_with(r"C:\Users\u\AppData\Roaming"));
        assert!(p.to_string_lossy().ends_with("PrivaVault"));
        assert_eq!(config_dir(Os::Windows, env(&[("APPDATA", "rel")])), None);
    }

    #[test]
    fn session_detection() {
        let e = environment(env(&[("XDG_SESSION_TYPE", "wayland"), ("XDG_CURRENT_DESKTOP", "KDE")]));
        if cfg!(target_os = "linux") {
            assert_eq!(e.session, "wayland");
            assert_eq!(e.desktop.as_deref(), Some("KDE"));
            let e = environment(env(&[("DISPLAY", ":0")]));
            assert_eq!(e.session, "x11");
        }
    }

    #[test]
    fn launch_paths_filters_and_decodes() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("my vault.pvault");
        std::fs::write(&f, b"x").unwrap();
        let uri = format!("file://{}", f.to_string_lossy().replace(' ', "%20"));
        let args = vec![
            "privavault".to_string(),
            "--flag".to_string(),
            f.to_string_lossy().into_owned(),
            uri,
            "/does/not/exist".to_string(),
            "my vault.pvault".to_string(),
        ];
        let got = launch_paths(args, Some(d.path()));
        if cfg!(windows) {
            assert!(got.contains(&f));
        } else {
            assert_eq!(got, vec![f.clone(), f.clone(), f]);
        }
    }
}
