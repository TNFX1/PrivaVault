//! Notes: text typed in the app, stored as a top-level `.txt` file entry. The text only ever
//! lives in memory (zeroised on drop); in the vault it is an ordinary file any version can extract.

use crate::paths::sanitize_entry;
use crate::v2::{EntryKind, ManifestEntry};
use std::collections::HashSet;
use zeroize::Zeroizing;

/// A note to store in a vault. `name` is the title the user typed; it becomes `<name>.txt`.
pub struct Note {
    pub name: String,
    pub text: Zeroizing<String>,
}

/// Manifest entries for `notes`: one-component `.txt` names, clear of the lower-cased
/// top-level names in `taken` (which is updated), ` (2)`, ` (3)`… on a clash.
pub(crate) fn note_entries(notes: &[Note], taken: &mut HashSet<String>) -> Vec<ManifestEntry> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64);
    notes
        .iter()
        .map(|n| {
            let cleaned: String =
                n.name.chars().map(|c| if c == '/' || c == '\\' || c.is_control() { '-' } else { c }).collect();
            let mut base = cleaned.trim().trim_start_matches('.').trim().to_string();
            if !base.to_lowercase().ends_with(".txt") {
                base.push_str(".txt");
            }
            if base == ".txt" || sanitize_entry(&base).is_err() {
                base = "note.txt".into();
            }
            let stem = base[..base.len() - 4].to_string();
            let mut name = base;
            let mut i = 2;
            while !taken.insert(name.to_lowercase()) {
                name = format!("{stem} ({i}).txt");
                i += 1;
            }
            ManifestEntry { path: name, kind: EntryKind::File, size: n.text.len() as u64, mtime: now, exec: false }
        })
        .collect()
}
