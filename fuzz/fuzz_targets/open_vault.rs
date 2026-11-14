//! Any bytes opened as a vault must give an error or a vault, never a panic or a runaway allocation.
#![no_main]
use libfuzzer_sys::fuzz_target;
use privavault_core::{v2::MAGIC, Job, Vault};

fuzz_target!(|data: &[u8]| {
    // shortcut: skip inputs that ask for an expensive KDF (scrypt above 2^14, PBKDF2 above 2000
    // rounds). Those are capped by the core and only slow, but they would stall the fuzzer.
    if data.len() >= 13 && &data[..8] == MAGIC && (data[10] > 14 || data[11] > 8 || data[12] > 1) {
        return;
    }
    if data.len() >= 32 && data[..8] != MAGIC[..] && u32::from_be_bytes(data[28..32].try_into().unwrap()) > 2000 {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.pvault");
    std::fs::write(&path, data).unwrap();
    if let Ok(mut v) = Vault::open(&path, "fuzz password", &Job::none()) {
        let _ = v.verify(&Job::none());
    }
});
