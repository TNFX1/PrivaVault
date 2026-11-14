//! Fuzz-style robustness checks on stable Rust (the coverage-guided targets live in `fuzz/`).
//!
//! A vault is untrusted input: anyone can hand you a `.pvault` file. Whatever its bytes, opening it
//! must return an error (never panic, hang or allocate wildly), a damaged vault must never open,
//! and no entry name may lead outside the extraction folder. Deterministic PRNG, so failures repeat.

use privavault_core::paths::{join_safe, sanitize_entry};
use privavault_core::v2::MAGIC;
use privavault_core::{create_vault, Job, ScryptParams, Vault};
use std::fs;
use std::path::{Component, Path};

const FAST: ScryptParams = ScryptParams { log_n: 14, r: 8, p: 1 };
const PW: &str = "correct horse battery staple";

/// xorshift64*: tiny, deterministic, good enough to pick bytes and positions.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Open `bytes` as a vault; it must fail. Returns the error code.
fn must_fail(dir: &Path, bytes: &[u8], password: &str, what: &str) -> &'static str {
    let p = dir.join("case.pvault");
    fs::write(&p, bytes).unwrap();
    match Vault::open(&p, password, &Job::none()) {
        Ok(mut v) => match v.verify(&Job::none()) {
            Ok(_) => panic!("{what}: a damaged vault opened and verified"),
            Err(e) => e.code(),
        },
        Err(e) => e.code(),
    }
}

fn v2_sample(dir: &Path) -> Vec<u8> {
    let src = dir.join("src");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("a.txt"), b"hello").unwrap();
    fs::write(src.join("sub/b.bin"), (0..150_000u32).map(|i| (i % 251) as u8).collect::<Vec<_>>()).unwrap();
    let out = dir.join("sample.pvault");
    create_vault(&[src], &out, PW, FAST, &Job::none()).unwrap();
    fs::read(out).unwrap()
}

#[test]
fn v2_any_single_byte_change_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let good = v2_sample(d.path());
    let mut rng = Rng(0x5eed_0001);
    // Every header byte, then random positions in the encrypted body.
    let positions: Vec<usize> = (0..80).chain((0..100).map(|_| 80 + rng.below(good.len() - 80))).collect();
    for i in positions {
        let mut b = good.clone();
        b[i] ^= 1 << rng.below(8);
        must_fail(d.path(), &b, PW, &format!("v2 byte {i}"));
    }
}

#[test]
fn v2_truncated_or_extended_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let good = v2_sample(d.path());
    let mut rng = Rng(0x5eed_0002);
    let mut lens: Vec<usize> = vec![0, 1, 7, 8, 79, 80, 81, good.len() - 16, good.len() - 1];
    lens.extend((0..30).map(|_| rng.below(good.len())));
    for n in lens {
        must_fail(d.path(), &good[..n], PW, &format!("v2 truncated to {n}"));
    }
    let mut longer = good.clone();
    longer.extend_from_slice(&[0; 17]);
    must_fail(d.path(), &longer, PW, "v2 extended");
}

#[test]
fn v2_header_fields_with_wild_values_are_refused() {
    // Keep the header valid-looking and randomise the lengths and chunk size: exercises the
    // size arithmetic (overflow, allocation) before anything is decrypted.
    let d = tempfile::tempdir().unwrap();
    let good = v2_sample(d.path());
    let mut rng = Rng(0x5eed_0003);
    for case in 0..400 {
        let mut b = good.clone();
        match case % 4 {
            0 => b[40..48].copy_from_slice(&rng.next().to_be_bytes()),
            1 => b[48..56].copy_from_slice(&rng.next().to_be_bytes()),
            2 => b[14] = rng.next() as u8,
            _ => {
                b[40..48].copy_from_slice(&(rng.next() >> rng.below(64)).to_be_bytes());
                b[48..56].copy_from_slice(&(rng.next() >> rng.below(64)).to_be_bytes());
            }
        }
        if b == good {
            continue;
        }
        must_fail(d.path(), &b, PW, &format!("v2 header case {case}"));
    }
}

#[test]
fn v1_damage_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let good = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1_evil.pvault")).unwrap();
    let mut rng = Rng(0x5eed_0004);
    for i in 0..good.len() {
        // Bytes 28..32 are the PBKDF2 iteration count: a flip there can ask for millions of rounds
        // (capped at 10M, see kdf::V1_MAX_ITERATIONS), which is slow but not a bug.
        if (28..32).contains(&i) {
            continue;
        }
        let mut b = good.clone();
        b[i] ^= 1 << rng.below(8);
        must_fail(d.path(), &b, "evil", &format!("v1 byte {i}"));
    }
    for n in [0, 1, 51, 52, 60, good.len() - 1] {
        must_fail(d.path(), &good[..n], "evil", &format!("v1 truncated to {n}"));
    }
}

#[test]
fn random_files_never_panic() {
    let d = tempfile::tempdir().unwrap();
    let mut rng = Rng(0x5eed_0005);
    for case in 0..300 {
        let len = [rng.below(16), rng.below(100), rng.below(5000)][case % 3];
        let mut b: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        if case % 2 == 0 && b.len() >= 8 {
            b[..8].copy_from_slice(MAGIC); // so the v2 parser sees it
        }
        // v1 has no magic: cap the iteration count so the case stays fast.
        if b.len() >= 32 && &b[..8] != MAGIC {
            b[28..32].copy_from_slice(&(1 + rng.below(2000) as u32).to_be_bytes());
        }
        must_fail(d.path(), &b, PW, &format!("random case {case}"));
    }
}

#[test]
fn entry_names_never_escape_the_folder() {
    let root = Path::new("/tmp/pv-root");
    let parts =
        ["a", "..", ".", "", "/", "\\", "b.txt", "C:", "con", " ", "x\u{0}", "\u{202e}", "ş", "\n", "//", "..\\.."];
    let mut rng = Rng(0x5eed_0006);
    for _ in 0..20_000 {
        let name: String = (0..1 + rng.below(6)).map(|_| parts[rng.below(parts.len())]).collect();
        if sanitize_entry(&name).is_err() {
            continue;
        }
        let p = join_safe(root, &name).unwrap_or_else(|e| panic!("{name:?}: sanitized but not joinable: {e}"));
        let rel = p.strip_prefix(root).unwrap_or_else(|_| panic!("{name:?} escaped to {}", p.display()));
        assert!(
            rel.components().all(|c| matches!(c, Component::Normal(_))) && rel.components().count() > 0,
            "{name:?} → {}",
            p.display()
        );
    }
}
