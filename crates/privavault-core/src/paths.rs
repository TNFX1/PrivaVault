//! Path safety for entry names stored inside vaults.
//!
//! Entry names come from an untrusted file (anyone can craft a vault and share its password),
//! so they are validated component by component before anything touches the filesystem.
//! This is the defence against "zip-slip" (`../../.bashrc`) and absolute-path entries.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// Validate an entry name and return its normalised components.
///
/// Accepts `/` and `\` as separators (ZIP files written on Windows sometimes use `\`).
/// Rejects absolute paths, drive prefixes, `.`/`..`, empty components, control characters,
/// and — on Windows — names the OS would reinterpret (reserved devices, `:` streams, trailing dots).
pub fn sanitize_entry(name: &str) -> Result<Vec<String>> {
    let unsafe_path = || Error::UnsafePath(name.to_string());
    let trimmed = name.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.starts_with(['/', '\\']) {
        return Err(unsafe_path());
    }
    let mut out = Vec::new();
    for comp in trimmed.split(['/', '\\']) {
        if comp.is_empty() || comp == "." || comp == ".." || comp.len() > 255 {
            return Err(unsafe_path());
        }
        if comp.chars().any(|c| c.is_control()) {
            return Err(unsafe_path());
        }
        if cfg!(windows) && !windows_component_ok(comp) {
            return Err(unsafe_path());
        }
        out.push(comp.to_string());
    }
    Ok(out)
}

fn windows_component_ok(comp: &str) -> bool {
    // ':' would be a drive prefix (`C:x`) or an NTFS alternate data stream (`a.txt:s`).
    if comp.ends_with(['.', ' ']) || comp.contains([':', '<', '>', '"', '|', '?', '*']) {
        return false;
    }
    let stem = comp.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    !reserved
}

/// Join validated components onto `root`. The result is always inside `root`.
pub fn join_safe(root: &Path, name: &str) -> Result<PathBuf> {
    let mut p = root.to_path_buf();
    for c in sanitize_entry(name)? {
        p.push(c);
    }
    debug_assert!(p.starts_with(root));
    Ok(p)
}

/// Return `dir/name`, or `dir/name (2)`, `dir/name (3)`, … — the first path that does not exist.
/// The extension is kept: `photo.jpg` → `photo (2).jpg`.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !exists_no_follow(&candidate) {
        return candidate;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2u32..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !exists_no_follow(p))
        .expect("u32 range exhausted")
}

fn exists_no_follow(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

/// Final component of a path as a display string (portable; handles `\` and `/`).
pub fn display_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_names() {
        assert_eq!(sanitize_entry("a.txt").unwrap(), ["a.txt"]);
        assert_eq!(sanitize_entry("docs/nested/n.md").unwrap(), ["docs", "nested", "n.md"]);
        assert_eq!(sanitize_entry("docs/").unwrap(), ["docs"]);
        assert_eq!(sanitize_entry("win\\style.txt").unwrap(), ["win", "style.txt"]);
        assert_eq!(sanitize_entry("şifre-ğüı.txt").unwrap(), ["şifre-ğüı.txt"]);
        assert_eq!(sanitize_entry("it's (1).txt").unwrap(), ["it's (1).txt"]);
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        for bad in [
            "../evil", "a/../../evil", "/etc/passwd", "\\\\server\\share", "a//b", "./a", "", "/",
            "a/./b", "a\0b", "a\nb", "..\\evil",
        ] {
            assert!(sanitize_entry(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_reserved() {
        for bad in ["C:x", "C:\\x", "file.txt:stream", "CON", "nul.txt", "com1", "LPT9.log", "trailing.", "trailing ", "a?b", "a*b"] {
            assert!(sanitize_entry(bad).is_err(), "should reject {bad:?}");
        }
        assert!(sanitize_entry("CONSOLE.txt").is_ok());
    }

    #[test]
    fn join_stays_inside_root() {
        let root = Path::new("/tmp/root");
        assert_eq!(join_safe(root, "a/b.txt").unwrap(), root.join("a").join("b.txt"));
        assert!(join_safe(root, "../x").is_err());
    }

    #[test]
    fn unique_path_appends_counter() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(d.path(), "a.txt"), d.path().join("a.txt"));
        std::fs::write(d.path().join("a.txt"), b"").unwrap();
        assert_eq!(unique_path(d.path(), "a.txt"), d.path().join("a (2).txt"));
        std::fs::write(d.path().join("a (2).txt"), b"").unwrap();
        assert_eq!(unique_path(d.path(), "a.txt"), d.path().join("a (3).txt"));
        assert_eq!(unique_path(d.path(), ".hidden"), d.path().join(".hidden"));
    }
}
