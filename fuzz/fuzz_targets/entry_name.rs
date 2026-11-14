//! A name accepted from a vault must always resolve to a path inside the extraction folder.
#![no_main]
use libfuzzer_sys::fuzz_target;
use privavault_core::paths::{join_safe, sanitize_entry};
use std::path::{Component, Path};

fuzz_target!(|data: &[u8]| {
    let Ok(name) = std::str::from_utf8(data) else { return };
    if sanitize_entry(name).is_err() {
        return;
    }
    let root = Path::new("/fuzz-root");
    let p = join_safe(root, name).expect("sanitized names join");
    let rel = p.strip_prefix(root).expect("stays inside the root");
    assert!(rel.components().count() > 0 && rel.components().all(|c| matches!(c, Component::Normal(_))));
});
