//! Password-based key derivation.
//!
//! * v1 vaults: PBKDF2-HMAC-SHA256 (as written by PrivaVault 1.x).
//! * v2 vaults: scrypt (memory-hard), parameters stored in the authenticated header.
//!
//! All parameters read from a file are bounds-checked *before* any work is done so a
//! tampered header cannot freeze the app or exhaust memory.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub type Key = Zeroizing<[u8; 32]>;

/// Upper bound for PBKDF2 iterations accepted from a v1 header (1.x offered at most 600k).
pub const V1_MAX_ITERATIONS: u32 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScryptParams {
    pub log_n: u8,
    pub r: u32,
    pub p: u32,
}

/// User-facing strength presets for new vaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Strength {
    /// N=2^17, r=8, p=1 — 128 MiB, the scrypt crate's recommended default.
    #[default]
    Standard,
    /// N=2^18 — 256 MiB.
    High,
    /// N=2^19 — 512 MiB.
    Maximum,
}

impl Strength {
    pub fn params(self) -> ScryptParams {
        let log_n = match self {
            Strength::Standard => 17,
            Strength::High => 18,
            Strength::Maximum => 19,
        };
        ScryptParams { log_n, r: 8, p: 1 }
    }
}

impl ScryptParams {
    /// Memory needed by scrypt in bytes (128 · r · N).
    pub fn memory_bytes(&self) -> u64 {
        128u64 * self.r as u64 * (1u64 << self.log_n)
    }

    /// Reject parameters that are too weak to be safe or large enough to be a DoS.
    pub fn validate(&self) -> Result<()> {
        if !(14..=22).contains(&self.log_n) {
            return Err(Error::Unsupported("scrypt log2(N) out of range"));
        }
        if !(1..=32).contains(&self.r) || !(1..=16).contains(&self.p) {
            return Err(Error::Unsupported("scrypt r/p out of range"));
        }
        if self.memory_bytes() > 1 << 30 {
            return Err(Error::Unsupported("scrypt memory above 1 GiB"));
        }
        Ok(())
    }

    pub fn derive(&self, password: &str, salt: &[u8]) -> Result<Key> {
        self.validate()?;
        let params = scrypt::Params::new(self.log_n, self.r, self.p, 32)
            .map_err(|_| Error::Unsupported("invalid scrypt parameters"))?;
        let mut out = Zeroizing::new([0u8; 32]);
        scrypt::scrypt(password.as_bytes(), salt, &params, out.as_mut())
            .map_err(|_| Error::Unsupported("invalid scrypt output length"))?;
        Ok(out)
    }
}

/// PBKDF2-HMAC-SHA256 as used by v1 vaults (Node's `crypto.pbkdf2Sync(pw, salt, n, 32, 'sha256')`).
pub fn pbkdf2_sha256(password: &str, salt: &[u8], iterations: u32) -> Result<Key> {
    if iterations == 0 || iterations > V1_MAX_ITERATIONS {
        return Err(Error::Corrupted("implausible PBKDF2 iteration count"));
    }
    let mut out = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), salt, iterations, out.as_mut());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_matches_rfc7914_vector() {
        // RFC 7914 §11: PBKDF2-HMAC-SHA256("passwd", "salt", c=1, dkLen=64), first 32 bytes.
        let k = pbkdf2_sha256("passwd", b"salt", 1).unwrap();
        assert_eq!(hex(&k[..]), "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc");
    }

    #[test]
    fn scrypt_matches_rfc7914_vector() {
        // RFC 7914 §12: scrypt("password", "NaCl", N=1024, r=8, p=16), first 32 bytes.
        // N=1024 is below our accepted floor, so call the crate directly for the vector.
        let params = scrypt::Params::new(10, 8, 16, 32).unwrap();
        let mut out = [0u8; 32];
        scrypt::scrypt(b"password", b"NaCl", &params, &mut out).unwrap();
        assert_eq!(hex(&out), "fdbabe1c9d3472007856e7190d01e9fe7c6ad7cbc8237830e77376634b373162");
    }

    #[test]
    fn rejects_dos_parameters() {
        assert!(pbkdf2_sha256("x", b"s", u32::MAX).is_err());
        assert!(pbkdf2_sha256("x", b"s", 0).is_err());
        assert!(ScryptParams { log_n: 30, r: 8, p: 1 }.validate().is_err());
        assert!(ScryptParams { log_n: 22, r: 32, p: 1 }.validate().is_err()); // 16 GiB
        assert!(ScryptParams { log_n: 10, r: 8, p: 1 }.validate().is_err()); // too weak
        for s in [Strength::Standard, Strength::High, Strength::Maximum] {
            s.params().validate().unwrap();
        }
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}
