//! Notes: text typed in the app and stored in the vault as a plain `.txt` file entry. The text
//! only exists in memory; any PrivaVault version extracts it as an ordinary text file.

use privavault_core::{create_vault_with_notes, Job, Note, ScryptParams, Vault};
use std::fs;

const FAST: ScryptParams = ScryptParams { log_n: 14, r: 8, p: 1 };

fn note(name: &str, text: &str) -> Note {
    Note { name: name.into(), text: text.to_string().into() }
}

fn read(v: &mut Vault, path: &str) -> String {
    let i = v.entries().iter().position(|e| e.path == path).unwrap_or_else(|| panic!("{path} missing"));
    String::from_utf8(v.read_entry(i, u64::MAX, &Job::none()).unwrap().to_vec()).unwrap()
}

fn paths(v: &Vault) -> Vec<String> {
    let mut p: Vec<String> = v.entries().iter().map(|e| e.path.clone()).collect();
    p.sort();
    p
}

#[test]
fn notes_are_stored_as_text_files_next_to_files() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("photo.jpg");
    fs::write(&f, b"jpeg").unwrap();
    let out = d.path().join("v.pvault");
    let notes = [note("Alışveriş", "süt\nekmek\n"), note("plan.txt", "")];
    create_vault_with_notes(&[f], &notes, &out, "pw", FAST, &Job::none()).unwrap();

    let mut v = Vault::open(&out, "pw", &Job::none()).unwrap();
    assert_eq!(paths(&v), ["Alışveriş.txt", "photo.jpg", "plan.txt"]);
    assert_eq!(read(&mut v, "Alışveriş.txt"), "süt\nekmek\n");
    assert_eq!(read(&mut v, "plan.txt"), "");
}

#[test]
fn a_vault_can_hold_only_a_note() {
    let d = tempfile::tempdir().unwrap();
    let out = d.path().join("v.pvault");
    create_vault_with_notes(&[], &[note("kodlar", "1234-5678")], &out, "pw", FAST, &Job::none()).unwrap();
    let mut v = Vault::open(&out, "pw", &Job::none()).unwrap();
    assert_eq!(read(&mut v, "kodlar.txt"), "1234-5678");
}

#[test]
fn editing_a_note_replaces_it_and_clashing_names_get_a_suffix() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("notes.txt");
    fs::write(&f, b"file on disk").unwrap();
    let out = d.path().join("v.pvault");
    create_vault_with_notes(&[f], &[note("notes", "typed")], &out, "pw", FAST, &Job::none()).unwrap();
    let mut v = Vault::open(&out, "pw", &Job::none()).unwrap();
    assert_eq!(paths(&v), ["notes (2).txt", "notes.txt"]);

    // Save an edited note: remove the old entry, add the new text under the same name
    v.rewrite_with_notes(
        &out,
        "pw",
        FAST,
        &["notes (2).txt".into()],
        &[],
        &[note("notes (2).txt", "edited")],
        &Job::none(),
    )
    .unwrap();
    let mut v = Vault::open(&out, "pw", &Job::none()).unwrap();
    assert_eq!(paths(&v), ["notes (2).txt", "notes.txt"]);
    assert_eq!(read(&mut v, "notes (2).txt"), "edited");
    assert_eq!(read(&mut v, "notes.txt"), "file on disk");
}

#[test]
fn note_names_cannot_leave_the_vault_root() {
    let d = tempfile::tempdir().unwrap();
    let out = d.path().join("v.pvault");
    let notes = [note("../../etc/passwd", "x"), note("a\\b", "y"), note("   ", "z")];
    create_vault_with_notes(&[], &notes, &out, "pw", FAST, &Job::none()).unwrap();
    let v = Vault::open(&out, "pw", &Job::none()).unwrap();
    for e in v.entries() {
        assert!(!e.path.contains('/') && !e.path.contains('\\') && !e.unsafe_path, "{}", e.path);
        assert!(e.path.ends_with(".txt"), "{}", e.path);
    }
    assert_eq!(v.entries().len(), 3);
}
