//! Editing vaults: add/remove files, change password, verify, preview, delete originals.
use privavault_core::{create_vault, remove_originals, Error, Job, ScryptParams, Vault};
use std::fs;
use std::path::PathBuf;

const FAST: ScryptParams = ScryptParams { log_n: 14, r: 8, p: 1 };

fn names(v: &Vault) -> Vec<String> {
    v.entries().iter().map(|e| e.path.clone()).collect()
}

fn setup() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("docs");
    fs::create_dir_all(docs.join("old")).unwrap();
    fs::write(docs.join("a.txt"), b"alpha").unwrap();
    fs::write(docs.join("old/b.txt"), vec![7u8; 200_000]).unwrap();
    let vault = dir.path().join("v.pvault");
    create_vault(&[docs], &vault, "pw", FAST, &Job::none()).unwrap();
    (dir, vault)
}

#[test]
fn add_and_remove_rewrites_in_place() {
    let (dir, path) = setup();
    let extra = dir.path().join("docs.txt"); // same stem as the folder: no clash
    fs::write(&extra, b"new file").unwrap();
    let again = dir.path().join("again");
    fs::create_dir(&again).unwrap();
    fs::write(again.join("docs"), b"name clash with folder").unwrap();

    let mut v = Vault::open(&path, "pw", &Job::none()).unwrap();
    v.rewrite(&path, "pw", FAST, &["docs/old".into()], &[extra, again.join("docs")], &Job::none()).unwrap();
    drop(v);
    let mut v = Vault::open(&path, "pw", &Job::none()).unwrap();
    assert_eq!(names(&v), ["docs", "docs/a.txt", "docs.txt", "docs (2)"]);
    let i = v.entries().iter().find(|e| e.path == "docs (2)").unwrap().index;
    assert_eq!(&*v.read_entry(i, 1 << 20, &Job::none()).unwrap(), b"name clash with folder");
    assert_eq!(v.verify(&Job::none()).unwrap(), 5 + 8 + 22);
}

#[test]
fn change_password_uses_fresh_salt_and_nonce() {
    let (_dir, path) = setup();
    let before = fs::read(&path).unwrap();
    let mut v = Vault::open(&path, "pw", &Job::none()).unwrap();
    v.rewrite(&path, "new password", FAST, &[], &[], &Job::none()).unwrap();
    drop(v);
    let after = fs::read(&path).unwrap();
    assert_ne!(before[16..39], after[16..39], "salt and nonce must change");
    assert!(matches!(Vault::open(&path, "pw", &Job::none()), Err(Error::WrongPassword)));
    let mut v = Vault::open(&path, "new password", &Job::none()).unwrap();
    assert_eq!(names(&v), ["docs", "docs/a.txt", "docs/old", "docs/old/b.txt"]);
    assert_eq!(v.verify(&Job::none()).unwrap(), 200_005);
}

#[test]
fn removing_everything_is_refused_and_keeps_the_vault() {
    let (_dir, path) = setup();
    let before = fs::read(&path).unwrap();
    let mut v = Vault::open(&path, "pw", &Job::none()).unwrap();
    assert!(matches!(v.rewrite(&path, "pw", FAST, &["docs".into()], &[], &Job::none()), Err(Error::EmptyInput)));
    drop(v);
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn verify_finds_a_damaged_chunk() {
    let (_dir, path) = setup();
    let mut bytes = fs::read(&path).unwrap();
    let n = bytes.len();
    bytes[n - 40] ^= 1; // inside the last file's data, not the manifest
    fs::write(&path, &bytes).unwrap();
    let mut v = Vault::open(&path, "pw", &Job::none()).unwrap();
    assert!(v.verify(&Job::none()).is_err());
}

#[test]
fn preview_has_a_size_limit() {
    let (_dir, path) = setup();
    let mut v = Vault::open(&path, "pw", &Job::none()).unwrap();
    let big = v.entries().iter().find(|e| e.path == "docs/old/b.txt").unwrap().index;
    assert!(v.read_entry(big, 1000, &Job::none()).is_err());
    assert_eq!(v.read_entry(big, 1 << 20, &Job::none()).unwrap().len(), 200_000);
}

#[test]
fn remove_originals_deletes_only_encrypted_files() {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("docs");
    fs::create_dir_all(docs.join("sub")).unwrap();
    fs::write(docs.join("a.txt"), b"a").unwrap();
    fs::write(docs.join("sub/b.txt"), b"b").unwrap();
    let vault = dir.path().join("v.pvault");
    let report = create_vault(std::slice::from_ref(&docs), &vault, "pw", FAST, &Job::none()).unwrap();
    fs::write(docs.join("sub/late.txt"), b"added after locking").unwrap(); // not in the vault
    let left = remove_originals(std::slice::from_ref(&docs), &report.encrypted);
    assert!(!docs.join("a.txt").exists() && !docs.join("sub/b.txt").exists());
    assert!(docs.join("sub/late.txt").exists(), "files that aren't in the vault are kept");
    assert!(left.contains(&docs.join("sub")) && left.contains(&docs));
    assert!(vault.exists());
}
