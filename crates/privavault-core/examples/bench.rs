//! Throughput benchmark: `cargo run --release -p privavault-core --example bench -- [MiB]`
//! Uses real (non-sparse) random data on disk next to the system temp dir.
use privavault_core::{create_vault, Job, ScryptParams, Strength, Vault};
use std::io::Write;
use std::time::Instant;

fn main() {
    let mib: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(512);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("data.bin");
    let mut f = std::fs::File::create(&input).unwrap();
    let mut block = vec![0u8; 1 << 20];
    for i in 0..mib {
        for (j, b) in block.iter_mut().enumerate() {
            *b = (i as usize ^ j.wrapping_mul(2654435761)) as u8;
        }
        f.write_all(&block).unwrap();
    }
    f.sync_all().unwrap();
    drop(f);

    let t = Instant::now();
    let p = Strength::Standard.params();
    p.derive("password", b"0123456789abcdef").unwrap();
    println!("scrypt standard (N=2^{}, 128 MiB): {:.0} ms", p.log_n, t.elapsed().as_secs_f64() * 1e3);

    let vault = dir.path().join("v.pvault");
    let fast = ScryptParams { log_n: 14, r: 8, p: 1 };
    let t = Instant::now();
    create_vault(std::slice::from_ref(&input), &vault, "password", fast, &Job::none()).unwrap();
    let s = t.elapsed().as_secs_f64();
    println!("encrypt {mib} MiB: {s:.2} s = {:.0} MiB/s", mib as f64 / s);

    let mut v = Vault::open(&vault, "password", &Job::none()).unwrap();
    let t = Instant::now();
    v.extract_all(dir.path(), "out", &Job::none()).unwrap();
    let s = t.elapsed().as_secs_f64();
    println!("decrypt {mib} MiB: {s:.2} s = {:.0} MiB/s", mib as f64 / s);
}
