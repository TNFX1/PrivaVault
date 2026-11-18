//! Split vaults: a normal v2 vault cut into `name.pvault.001`, `.002`, … (no format change,
//! `cat name.pvault.0* > name.pvault` gives the original file back).

use privavault_core::{create_vault, part_paths, split_vault, Error, Job, ScryptParams, Vault};
use std::fs;

const FAST: ScryptParams = ScryptParams { log_n: 14, r: 8, p: 1 };

fn pattern(n: u32) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

fn make_vault(dir: &std::path::Path) -> (std::path::PathBuf, Vec<u8>) {
    let data = pattern(300_000);
    let input = dir.join("data.bin");
    fs::write(&input, &data).unwrap();
    let out = dir.join("v.pvault");
    create_vault(&[input], &out, "correct horse", FAST, &Job::none()).unwrap();
    (out, data)
}

#[test]
fn split_vault_opens_from_any_part_and_matches_the_original() {
    let d = tempfile::tempdir().unwrap();
    let (vault, data) = make_vault(d.path());
    let whole = fs::read(&vault).unwrap();

    let parts = split_vault(&vault, 100_000, &Job::none()).unwrap();
    assert_eq!(parts.len(), whole.len().div_ceil(100_000));
    assert!(!vault.exists(), "the single file is replaced by the parts");
    assert_eq!(parts[0], d.path().join("v.pvault.001"));
    let joined: Vec<u8> = parts.iter().flat_map(|p| fs::read(p).unwrap()).collect();
    assert_eq!(joined, whole, "parts are a plain byte split");

    for start in [&parts[0], &parts[1], parts.last().unwrap()] {
        assert_eq!(part_paths(start).as_deref(), Some(&parts[..]));
        let mut v = Vault::open(start, "correct horse", &Job::none()).unwrap();
        let i = v.entries().iter().position(|e| e.path == "data.bin").unwrap();
        assert_eq!(&*v.read_entry(i, u64::MAX, &Job::none()).unwrap(), &data[..]);
        assert!(v.verify(&Job::none()).is_ok());
    }
}

#[test]
fn a_missing_part_is_reported_not_misread() {
    let d = tempfile::tempdir().unwrap();
    let (vault, _) = make_vault(d.path());
    let parts = split_vault(&vault, 100_000, &Job::none()).unwrap();
    fs::remove_file(parts.last().unwrap()).unwrap();
    let err = Vault::open(&parts[0], "correct horse", &Job::none()).err().unwrap();
    assert!(matches!(err, Error::Corrupted(_)), "{err:?}");
}

#[test]
fn split_refuses_to_mix_with_existing_parts() {
    let d = tempfile::tempdir().unwrap();
    let (vault, _) = make_vault(d.path());
    fs::write(d.path().join("v.pvault.001"), b"old").unwrap();
    assert!(split_vault(&vault, 100_000, &Job::none()).is_err());
    assert!(vault.exists(), "nothing is removed when the split is refused");
}

#[test]
fn single_files_are_not_parts() {
    let d = tempfile::tempdir().unwrap();
    let (vault, _) = make_vault(d.path());
    assert_eq!(part_paths(&vault), None);
    assert_eq!(part_paths(&d.path().join("photo.jpg.1")), None);
    // A part size below one byte would loop forever
    assert!(split_vault(&vault, 0, &Job::none()).is_err());
}

#[test]
fn split_vaults_are_read_only() {
    let d = tempfile::tempdir().unwrap();
    let (vault, _) = make_vault(d.path());
    let parts = split_vault(&vault, 100_000, &Job::none()).unwrap();
    let before: Vec<Vec<u8>> = parts.iter().map(|p| fs::read(p).unwrap()).collect();
    let mut v = Vault::open(&parts[0], "correct horse", &Job::none()).unwrap();
    assert_eq!(v.part_count(), parts.len());
    let err = v.rewrite(&parts[0], "correct horse", FAST, &["data.bin".into()], &[], &Job::none()).err().unwrap();
    assert!(matches!(err, Error::InvalidParams(_)), "{err:?}");
    let after: Vec<Vec<u8>> = parts.iter().map(|p| fs::read(p).unwrap()).collect();
    assert_eq!(before, after);
}
