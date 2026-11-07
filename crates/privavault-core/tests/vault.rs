//! End-to-end tests for both vault formats.
//!
//! `tests/fixtures/v1_*.pvault` were produced by the *original* PrivaVault 1.4.0 encryption
//! code (archiver 5.3.2 + Node crypto), see `tools/v1-fixtures/`.

use privavault_core::{create_vault, detect_format, Error, Format, Job, ScryptParams, Vault};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const FAST: ScryptParams = ScryptParams { log_n: 14, r: 8, p: 1 };

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn big_pattern() -> Vec<u8> {
    (0..300_000u32).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

fn code<T>(r: Result<T, Error>) -> &'static str {
    match r {
        Ok(_) => "ok",
        Err(e) => e.code(),
    }
}

fn names(v: &Vault) -> Vec<String> {
    let mut n: Vec<_> = v.entries().iter().map(|e| e.path.clone()).collect();
    n.sort();
    n
}

fn copy_to_tmp(src: &Path, dir: &Path) -> PathBuf {
    let p = dir.join(src.file_name().unwrap());
    fs::copy(src, &p).unwrap();
    p
}

fn flip(path: &Path, offset: u64) {
    let mut b = fs::read(path).unwrap();
    let i = if offset as usize >= b.len() { b.len() - 1 } else { offset as usize };
    b[i] ^= 0x01;
    fs::write(path, b).unwrap();
}

// ------------------------------------------------------------------------------------------
// v1 (PrivaVault 1.x) compatibility
// ------------------------------------------------------------------------------------------

#[test]
fn v1_real_fixture_lists_and_extracts() {
    let path = fixture("v1_multi.pvault");
    assert_eq!(detect_format(&path).unwrap(), Format::V1);
    let mut v = Vault::open(&path, "correct horse battery staple", &Job::none()).unwrap();
    assert_eq!(v.format(), Format::V1);
    let n = names(&v);
    for expected in ["hello.txt", "big.bin", "docs/nested/notes.md", "docs/it's (1).txt", "docs/şifre-ğüı.txt"] {
        assert!(n.iter().any(|x| x == expected), "missing {expected} in {n:?}");
    }
    assert!(v.entries().iter().all(|e| !e.unsafe_path));

    let out = tempfile::tempdir().unwrap();
    let rep = v.extract_all(out.path(), "restored", &Job::none()).unwrap();
    let root = out.path().join("restored");
    assert_eq!(rep.root, root);
    assert!(rep.skipped_unsafe.is_empty());
    assert_eq!(fs::read(root.join("hello.txt")).unwrap(), b"Hello from PrivaVault 1.4.0\n");
    assert_eq!(fs::read(root.join("big.bin")).unwrap(), big_pattern());
    assert_eq!(fs::read(root.join("docs/nested/notes.md")).unwrap(), b"# notes\n");
    assert_eq!(fs::read(root.join("docs/it's (1).txt")).unwrap(), b"quote\n");
    assert_eq!(fs::read(root.join("docs/şifre-ğüı.txt")).unwrap(), b"unicode\n");
    assert!(root.join("docs/empty").is_dir());
}

#[test]
fn v1_single_entry_and_zip_export() {
    let mut v = Vault::open(&fixture("v1_multi.pvault"), "correct horse battery staple", &Job::none()).unwrap();
    let idx = v.entries().iter().find(|e| e.path == "big.bin").unwrap().index;
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("big-copy.bin");
    v.extract_entry(idx, &dest, &Job::none()).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), big_pattern());

    let zip_path = out.path().join("export.zip");
    let rep = v.export_zip(&zip_path, &Job::none()).unwrap();
    assert!(rep.files >= 5);
    let mut z = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name("hello.txt").unwrap(), &mut s).unwrap();
    assert_eq!(s, "Hello from PrivaVault 1.4.0\n");
}

#[test]
fn v1_100k_iterations_and_non_ascii_password() {
    let v = Vault::open(&fixture("v1_single_100k.pvault"), "Tr-şifre 123!", &Job::none()).unwrap();
    assert_eq!(names(&v), ["hello.txt"]);
    assert!(v.kdf_description().contains("100000"));
}

#[test]
fn v1_wrong_password() {
    let r = Vault::open(&fixture("v1_multi.pvault"), "wrong", &Job::none());
    assert_eq!(code(r), "wrong_password");
}

#[test]
fn v1_tampering_is_detected_before_any_output() {
    let dir = tempfile::tempdir().unwrap();
    // Middle of ciphertext, last ciphertext byte, the tag itself.
    let size = fs::metadata(fixture("v1_multi.pvault")).unwrap().len();
    for off in [5_000, size - 17, size - 1] {
        let p = copy_to_tmp(&fixture("v1_multi.pvault"), dir.path());
        flip(&p, off);
        assert_eq!(code(Vault::open(&p, "correct horse battery staple", &Job::none())), "corrupted", "offset {off}");
    }
    // Truncation.
    let p = copy_to_tmp(&fixture("v1_multi.pvault"), dir.path());
    let b = fs::read(&p).unwrap();
    fs::write(&p, &b[..b.len() - 100]).unwrap();
    assert_eq!(code(Vault::open(&p, "correct horse battery staple", &Job::none())), "corrupted");
    // Nothing was written next to the vault.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn v1_tampered_iteration_count_cannot_freeze_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy_to_tmp(&fixture("v1_multi.pvault"), dir.path());
    let mut b = fs::read(&p).unwrap();
    b[28..32].copy_from_slice(&u32::MAX.to_be_bytes());
    fs::write(&p, b).unwrap();
    let t = std::time::Instant::now();
    assert_eq!(code(Vault::open(&p, "correct horse battery staple", &Job::none())), "not_a_vault");
    assert!(t.elapsed().as_secs() < 2);
}

#[test]
fn v1_malicious_entry_names_are_never_extracted() {
    let mut v = Vault::open(&fixture("v1_evil.pvault"), "evil", &Job::none()).unwrap();
    let unsafe_names: Vec<String> = v.entries().iter().filter(|e| e.unsafe_path).map(|e| e.path.clone()).collect();
    assert!(unsafe_names.iter().any(|n| n == "../evil.txt"));
    assert!(unsafe_names.iter().any(|n| n == "/abs.txt"));

    let sandbox = tempfile::tempdir().unwrap();
    let parent = sandbox.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let rep = v.extract_all(&parent, "out", &Job::none()).unwrap();
    assert_eq!(rep.skipped_unsafe.len(), unsafe_names.len());
    assert!(!sandbox.path().join("evil.txt").exists());
    assert!(!parent.join("evil.txt").exists());
    assert!(!Path::new("/abs.txt").exists());
    assert_eq!(fs::read(parent.join("out/ok.txt")).unwrap(), b"fine");

    let idx = v.entries().iter().find(|e| e.path == "../evil.txt").unwrap().index;
    assert_eq!(code(v.extract_entry(idx, &sandbox.path().join("x"), &Job::none())), "unsafe_path");
}

// ------------------------------------------------------------------------------------------
// v2
// ------------------------------------------------------------------------------------------

/// A name with a colon is legal on Linux (and must round-trip) but impossible on Windows.
const ODD_NAME: &str = if cfg!(windows) { "notes 2024 şğü.md" } else { "notes:2024 şğü.md" };

fn sample_tree(dir: &Path) -> PathBuf {
    let root = dir.join("Project");
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    fs::create_dir_all(root.join("empty-dir")).unwrap();
    fs::write(root.join("readme.txt"), "hello v2").unwrap();
    fs::write(root.join("empty.txt"), "").unwrap();
    fs::write(root.join("sub/deeper/data.bin"), big_pattern()).unwrap();
    fs::write(root.join("sub").join(ODD_NAME), "colon + unicode").unwrap();
    root
}

fn make_v2(dir: &Path, password: &str) -> (PathBuf, PathBuf) {
    let tree = sample_tree(dir);
    let out = dir.join("project.pvault");
    let rep = create_vault(std::slice::from_ref(&tree), &out, password, FAST, &Job::none()).unwrap();
    assert_eq!(rep.files, 4);
    assert_eq!(rep.folders, 4);
    (tree, out)
}

#[test]
fn v2_roundtrip() {
    let d = tempfile::tempdir().unwrap();
    let (_, vault) = make_v2(d.path(), "pässwörd");
    assert_eq!(detect_format(&vault).unwrap(), Format::V2);
    let mut v = Vault::open(&vault, "pässwörd", &Job::none()).unwrap();
    assert!(v.kdf_description().starts_with("scrypt"));
    assert_eq!(
        names(&v),
        [
            "Project", "Project/empty-dir", "Project/empty.txt", "Project/readme.txt", "Project/sub",
            "Project/sub/deeper", "Project/sub/deeper/data.bin", &format!("Project/sub/{ODD_NAME}"),
        ]
    );
    let out = d.path().join("out");
    fs::create_dir(&out).unwrap();
    v.extract_all(&out, "restored", &Job::none()).unwrap();
    let r = out.join("restored/Project");
    assert_eq!(fs::read(r.join("readme.txt")).unwrap(), b"hello v2");
    assert_eq!(fs::read(r.join("empty.txt")).unwrap(), b"");
    assert_eq!(fs::read(r.join("sub/deeper/data.bin")).unwrap(), big_pattern());
    assert_eq!(fs::read(r.join("sub").join(ODD_NAME)).unwrap(), b"colon + unicode");
    assert!(r.join("empty-dir").is_dir());

    // Second extraction never overwrites: it goes to "restored (2)".
    let rep = v.extract_all(&out, "restored", &Job::none()).unwrap();
    assert_eq!(rep.root, out.join("restored (2)"));
}

#[test]
fn v2_single_file_vault_and_entry_extraction() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("one.bin");
    fs::write(&f, big_pattern()).unwrap();
    let vault = d.path().join("one.pvault");
    create_vault(std::slice::from_ref(&f), &vault, "pw", FAST, &Job::none()).unwrap();
    let mut v = Vault::open(&vault, "pw", &Job::none()).unwrap();
    assert_eq!(names(&v), ["one.bin"]);
    let dest = d.path().join("copy.bin");
    v.extract_entry(0, &dest, &Job::none()).unwrap();
    assert_eq!(fs::read(dest).unwrap(), big_pattern());

    let zip_path = d.path().join("all.zip");
    v.export_zip(&zip_path, &Job::none()).unwrap();
    let mut z = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
    let mut back = Vec::new();
    std::io::Read::read_to_end(&mut z.by_name("one.bin").unwrap(), &mut back).unwrap();
    assert_eq!(back, big_pattern());
}

#[test]
fn v2_chunk_boundaries() {
    // Plaintext totals around exact multiples of the 64 KiB chunk size.
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("f");
    let vault = d.path().join("v.pvault");
    for target_total in [65_536u64, 131_072, 131_073] {
        // First pass to learn the manifest length for a same-width size, then hit the target.
        let probe = target_total - 200;
        fs::write(&f, vec![1u8; probe as usize]).unwrap();
        create_vault(std::slice::from_ref(&f), &vault, "pw", FAST, &Job::none()).unwrap();
        let hdr = fs::read(&vault).unwrap();
        let manifest_len = u64::from_be_bytes(hdr[40..48].try_into().unwrap());
        let size = target_total - manifest_len;
        assert_eq!(size.to_string().len(), probe.to_string().len(), "size width must not change manifest length");
        fs::write(&f, vec![2u8; size as usize]).unwrap();
        create_vault(std::slice::from_ref(&f), &vault, "pw", FAST, &Job::none()).unwrap();
        let hdr = fs::read(&vault).unwrap();
        assert_eq!(u64::from_be_bytes(hdr[48..56].try_into().unwrap()), target_total);
        let mut v = Vault::open(&vault, "pw", &Job::none()).unwrap();
        let dest = d.path().join(format!("out-{target_total}"));
        v.extract_entry(0, &dest, &Job::none()).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), vec![2u8; size as usize]);
    }
}

#[test]
fn v2_wrong_password() {
    let d = tempfile::tempdir().unwrap();
    let (_, vault) = make_v2(d.path(), "right");
    assert_eq!(code(Vault::open(&vault, "wrong", &Job::none())), "wrong_password");
    assert_eq!(code(Vault::open(&vault, "", &Job::none())), "invalid_params");
}

#[test]
fn v2_tampering_is_detected() {
    let d = tempfile::tempdir().unwrap();
    let (_, vault) = make_v2(d.path(), "pw");
    let len = fs::metadata(&vault).unwrap().len();
    let cases: &[(u64, &str)] = &[
        (20, "wrong_password"),  // salt → different key → key check fails
        (45, "wrong_password"),  // manifest length is covered by the key check
        (70, "wrong_password"),  // key check itself
        (100, "corrupted"),      // first chunk (manifest)
    ];
    for &(off, expected) in cases {
        let p = d.path().join("t.pvault");
        fs::copy(&vault, &p).unwrap();
        flip(&p, off);
        assert_eq!(code(Vault::open(&p, "pw", &Job::none())), expected, "offset {off}");
    }
    // Opening only authenticates the manifest chunk (instant listing). A flipped byte in file
    // data — including the very last tag — is caught when that chunk is read, before release.
    let out = d.path().join("x");
    fs::create_dir(&out).unwrap();
    for off in [80 + 65_536 + 16 + 10, len - 1] {
        let p = d.path().join("t.pvault");
        fs::copy(&vault, &p).unwrap();
        flip(&p, off);
        let mut v = Vault::open(&p, "pw", &Job::none()).unwrap();
        assert_eq!(code(v.extract_all(&out, "r", &Job::none())), "corrupted", "offset {off}");
        assert!(!out.join("r").exists(), "partial extraction is cleaned up");
        assert_eq!(code(v.export_zip(&out.join("e.zip"), &Job::none())), "corrupted");
        assert!(!out.join("e.zip").exists());
    }
    let p = d.path().join("t.pvault");

    // Truncation and appended data are rejected up front.
    let b = fs::read(&vault).unwrap();
    fs::write(&p, &b[..b.len() - 1]).unwrap();
    assert_eq!(code(Vault::open(&p, "pw", &Job::none())), "corrupted");
    let mut ext = b.clone();
    ext.push(0);
    fs::write(&p, ext).unwrap();
    assert_eq!(code(Vault::open(&p, "pw", &Job::none())), "corrupted");

    // Reordering chunks is detected (counter is part of the nonce).
    let mut swapped = b.clone();
    let c = 65_536 + 16;
    let (a, rest) = swapped[80..].split_at_mut(c);
    a.swap_with_slice(&mut rest[..c]);
    fs::write(&p, swapped).unwrap();
    assert_eq!(code(Vault::open(&p, "pw", &Job::none())), "corrupted");
}

#[test]
fn v2_header_params_are_bounded() {
    let d = tempfile::tempdir().unwrap();
    let (_, vault) = make_v2(d.path(), "pw");
    let mut b = fs::read(&vault).unwrap();
    b[10] = 40; // scrypt N = 2^40
    fs::write(&vault, &b).unwrap();
    assert_eq!(code(Vault::open(&vault, "pw", &Job::none())), "unsupported");
    b[10] = 14;
    b[8] = 3; // future version
    fs::write(&vault, &b).unwrap();
    assert_eq!(code(Vault::open(&vault, "pw", &Job::none())), "unsupported");
}

#[test]
fn cancellation_leaves_no_output() {
    let d = tempfile::tempdir().unwrap();
    let tree = sample_tree(d.path());
    let out = d.path().join("c.pvault");
    let cancel = AtomicBool::new(true);
    let job = Job::new(&cancel, &|_| {});
    assert_eq!(code(create_vault(&[tree], &out, "pw", FAST, &job)), "cancelled");
    assert!(!out.exists());
    let leftovers: Vec<_> = fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers, ["Project"], "no .pvpart files left");
}

#[test]
fn progress_is_reported_and_monotonic() {
    let d = tempfile::tempdir().unwrap();
    let tree = sample_tree(d.path());
    let out = d.path().join("p.pvault");
    let seen = std::sync::Mutex::new(Vec::new());
    let cancel = AtomicBool::new(false);
    let cb = |p: privavault_core::Progress| seen.lock().unwrap().push(p);
    create_vault(&[tree], &out, "pw", FAST, &Job::new(&cancel, &cb)).unwrap();
    let seen = seen.into_inner().unwrap();
    let enc: Vec<_> = seen.iter().filter(|p| p.phase == privavault_core::Phase::Encrypting).collect();
    assert!(enc.windows(2).all(|w| w[0].done <= w[1].done));
    assert_eq!(enc.last().unwrap().done, enc.last().unwrap().total);
}

#[test]
fn output_cannot_be_an_input() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("a.txt");
    fs::write(&f, "x").unwrap();
    assert_eq!(code(create_vault(std::slice::from_ref(&f), &f, "pw", FAST, &Job::none())), "output_is_input");
    assert_eq!(fs::read(&f).unwrap(), b"x");
}

#[test]
fn duplicate_top_level_names_are_disambiguated() {
    let d = tempfile::tempdir().unwrap();
    for sub in ["a", "b"] {
        fs::create_dir(d.path().join(sub)).unwrap();
        fs::write(d.path().join(sub).join("same.txt"), sub).unwrap();
    }
    let vault = d.path().join("v.pvault");
    create_vault(&[d.path().join("a/same.txt"), d.path().join("b/same.txt")], &vault, "pw", FAST, &Job::none())
        .unwrap();
    let v = Vault::open(&vault, "pw", &Job::none()).unwrap();
    assert_eq!(names(&v), ["same (2).txt", "same.txt"]);
}

#[cfg(unix)]
#[test]
fn symlinks_inside_folders_are_not_followed() {
    let d = tempfile::tempdir().unwrap();
    let tree = d.path().join("t");
    fs::create_dir(&tree).unwrap();
    fs::write(tree.join("real.txt"), "r").unwrap();
    std::os::unix::fs::symlink("/etc/passwd", tree.join("link")).unwrap();
    std::os::unix::fs::symlink(&tree, tree.join("loop")).unwrap();
    let vault = d.path().join("v.pvault");
    let rep = create_vault(&[tree], &vault, "pw", FAST, &Job::none()).unwrap();
    assert_eq!(rep.skipped.len(), 2);
    let v = Vault::open(&vault, "pw", &Job::none()).unwrap();
    assert_eq!(names(&v), ["t", "t/real.txt"]);
}

#[cfg(unix)]
#[test]
fn extracted_files_are_private_and_keep_exec_bit() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let tree = d.path().join("t");
    fs::create_dir(&tree).unwrap();
    fs::write(tree.join("run.sh"), "#!/bin/sh").unwrap();
    fs::set_permissions(tree.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(tree.join("doc.txt"), "d").unwrap();
    let vault = d.path().join("v.pvault");
    create_vault(&[tree], &vault, "pw", FAST, &Job::none()).unwrap();
    let mut v = Vault::open(&vault, "pw", &Job::none()).unwrap();
    v.extract_all(d.path(), "out", &Job::none()).unwrap();
    let mode = |p: &str| fs::metadata(d.path().join("out/t").join(p)).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode("doc.txt"), 0o600);
    assert_eq!(mode("run.sh"), 0o700);
    assert_eq!(fs::metadata(&vault).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn garbage_files_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    let tiny = d.path().join("tiny");
    fs::write(&tiny, b"hi").unwrap();
    assert_eq!(code(detect_format(&tiny)), "not_a_vault");
    let junk = d.path().join("junk.pvault");
    fs::write(&junk, (0..500u32).map(|i| (i * 7) as u8).collect::<Vec<_>>()).unwrap();
    let c = code(Vault::open(&junk, "pw", &Job::none()));
    assert!(["not_a_vault", "wrong_password"].contains(&c), "got {c}");
}
