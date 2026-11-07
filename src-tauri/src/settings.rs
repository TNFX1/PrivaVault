//! Persistent user settings.
//!
//! Stored as JSON in the platform config dir (see `platform::config_dir`) with owner-only
//! permissions. Settings never contain secrets, file paths or vault history.
//! A corrupt file is moved aside (`settings.json.corrupt-<unix time>`) and defaults are used,
//! so a bad write can never prevent the app from starting.

use privavault_core::fsutil::{create_private_dir_all, write_private_atomic};
use privavault_core::Strength;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "settings.json";
pub const EXTENSIONS: &[&str] = &["pvault", "vault", "enc", "bin"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    /// "auto", "en" or "tr".
    pub language: String,
    pub strength: Strength,
    pub default_extension: String,
    /// Opt-in: contact api.github.com on launch to look for a newer release.
    pub check_updates: bool,
    pub notifications: bool,
    /// Lock an open vault after this many idle minutes (0 = never).
    pub autolock_minutes: u32,
    pub onboarding_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Theme::System,
            language: "auto".into(),
            strength: Strength::Standard,
            default_extension: "pvault".into(),
            check_updates: false,
            notifications: true,
            autolock_minutes: 10,
            onboarding_done: false,
        }
    }
}

impl Settings {
    /// Clamp anything out of range (hand-edited or from an older/newer version).
    pub fn sanitized(mut self) -> Self {
        if !["auto", "en", "tr"].contains(&self.language.as_str()) {
            self.language = "auto".into();
        }
        if !EXTENSIONS.contains(&self.default_extension.as_str()) {
            self.default_extension = "pvault".into();
        }
        self.autolock_minutes = self.autolock_minutes.min(24 * 60);
        self
    }

    pub fn load(dir: &Path) -> Settings {
        let path = dir.join(FILE_NAME);
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(s) => s.sanitized(),
                Err(_) => {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let _ = std::fs::rename(&path, dir.join(format!("{FILE_NAME}.corrupt-{ts}")));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<PathBuf, String> {
        create_private_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = dir.join(FILE_NAME);
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        write_private_atomic(&path, &json).map_err(|e| e.to_string())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_private_by_default() {
        let s = Settings::default();
        assert!(!s.check_updates, "network access must be opt-in");
        assert_eq!(s.strength, Strength::Standard);
    }

    #[test]
    fn roundtrip_and_permissions() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("cfg");
        let s = Settings { theme: Theme::Dark, language: "tr".into(), check_updates: true, ..Default::default() };
        let path = s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        let _ = path;
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(FILE_NAME), b"{not json").unwrap();
        assert_eq!(Settings::load(d.path()), Settings::default());
        assert!(!d.path().join(FILE_NAME).exists());
        let names: Vec<_> = std::fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert!(names[0].to_string_lossy().starts_with("settings.json.corrupt-"));
    }

    #[test]
    fn unknown_and_missing_fields_and_bad_values() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join(FILE_NAME),
            br#"{"theme":"light","future_option":1,"default_extension":"exe","language":"xx","autolock_minutes":999999}"#,
        )
        .unwrap();
        let s = Settings::load(d.path());
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.default_extension, "pvault");
        assert_eq!(s.language, "auto");
        assert_eq!(s.autolock_minutes, 1440);
        assert!(s.notifications);
    }
}
