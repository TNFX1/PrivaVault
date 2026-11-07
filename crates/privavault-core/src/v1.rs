//! Read-only support for vaults written by PrivaVault 1.x.
//!
//! ```text
//! salt(16) ‖ iv(12) ‖ iterations(u32 BE) ‖ AES-256-GCM(PBKDF2-SHA256(pw, salt, iterations), iv, ZIP) ‖ tag(16)
//! ```
//!
//! 1.x decrypted the whole archive to a plaintext temp file *before* the GCM tag was checked.
//! This reader instead:
//!
//! 1. bounds-checks the iteration count (a tampered header cannot freeze the app),
//! 2. does a cheap password check on the first 2 plaintext bytes (ZIP magic `PK`, as 1.x did),
//! 3. authenticates the **entire** ciphertext by computing GHASH over it — no plaintext is
//!    produced in this pass — and compares the tag in constant time,
//! 4. only then decrypts, on demand, through a seekable AES-CTR reader that feeds the ZIP parser
//!    directly. Nothing is written to a temp directory.
//!
//! GCM = CTR + GHASH is composed from the same RustCrypto primitives that `aes-gcm` uses
//! (J0 = IV ‖ 0³¹ ‖ 1, tag = GHASH_H(C) ⊕ E_K(J0), data keystream from inc32(J0));
//! it is cross-checked against `aes-gcm` in the tests below and against real 1.4.0 files.

use crate::error::{Error, IoContext, Result};
use crate::kdf::{pbkdf2_sha256, Key};
use crate::paths::sanitize_entry;
use crate::{EntryInfo, Job, Phase};
use aes::cipher::{BlockEncrypt, KeyInit, KeyIvInit, StreamCipher, StreamCipherSeek};
use aes::Aes256;
use ghash::universal_hash::UniversalHash;
use ghash::GHash;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

type Ctr = ctr::Ctr32BE<Aes256>;

const HEADER: u64 = 32;
const TAG: u64 = 16;

/// Initial counter block for the payload: inc32(J0) = IV ‖ 00 00 00 02.
fn data_counter_block(iv: &[u8; 12]) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[..12].copy_from_slice(iv);
    b[15] = 2;
    b
}

fn compute_tag(key: &Key, iv: &[u8; 12], file: &mut File, ct_len: u64, job: &Job) -> Result<[u8; 16]> {
    let cipher = Aes256::new_from_slice(key.as_ref()).expect("32-byte key");
    let mut h = ghash::Block::default();
    cipher.encrypt_block(&mut h);
    let mut ghash = GHash::new(&h);

    file.seek(SeekFrom::Start(HEADER)).ctx(|| "cannot read vault".into())?;
    let mut reader = file.take(ct_len);
    let mut buf = vec![0u8; 1 << 20];
    let mut carry = 0usize; // bytes at the start of buf not yet hashed (always < 16)
    let mut done = 0u64;
    loop {
        job.check_cancel()?;
        let n = reader.read(&mut buf[carry..]).ctx(|| "cannot read vault".into())?;
        if n == 0 {
            break;
        }
        done += n as u64;
        let avail = carry + n;
        let full = avail - avail % 16;
        ghash.update_padded(&buf[..full]); // multiple of 16: no padding applied
        buf.copy_within(full..avail, 0);
        carry = avail - full;
        job.report(Phase::Verifying, done, ct_len);
    }
    if done != ct_len {
        return Err(Error::Corrupted("unexpected end of file"));
    }
    ghash.update_padded(&buf[..carry]);
    let mut len_block = ghash::Block::default();
    len_block[8..].copy_from_slice(&(ct_len * 8).to_be_bytes()); // no AAD in v1
    ghash.update(&[len_block]);
    let mut tag = ghash.finalize();

    let mut j0 = ghash::Block::default();
    j0[..12].copy_from_slice(iv);
    j0[15] = 1;
    cipher.encrypt_block(&mut j0);
    for (t, m) in tag.iter_mut().zip(j0.iter()) {
        *t ^= m;
    }
    Ok(tag.into())
}

/// Seekable decrypting view of the (already authenticated) v1 ciphertext.
struct CtrReader {
    file: File,
    len: u64,
    pos: u64,
    ctr: Ctr,
}

impl Read for CtrReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len {
            return Ok(0);
        }
        let want = (out.len() as u64).min(self.len - self.pos) as usize;
        self.file.seek(SeekFrom::Start(HEADER + self.pos))?;
        let n = self.file.read(&mut out[..want])?;
        self.ctr.seek(self.pos);
        self.ctr.apply_keystream(&mut out[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for CtrReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let new = match to {
            SeekFrom::Start(o) => o as i128,
            SeekFrom::End(o) => self.len as i128 + o as i128,
            SeekFrom::Current(o) => self.pos as i128 + o as i128,
        };
        if new < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

pub struct V1Vault {
    zip: zip::ZipArchive<BufReader<CtrReader>>,
    entries: Vec<EntryInfo>,
    iterations: u32,
}

impl V1Vault {
    pub fn open(mut file: File, password: &str, job: &Job) -> Result<V1Vault> {
        let size = file.metadata().ctx(|| "cannot read vault".into())?.len();
        if size < HEADER + TAG + 4 {
            return Err(Error::NotAVault);
        }
        let mut head = [0u8; HEADER as usize];
        file.seek(SeekFrom::Start(0)).ctx(|| "cannot read vault".into())?;
        file.read_exact(&mut head).ctx(|| "cannot read vault".into())?;
        let salt = &head[0..16];
        let iv: [u8; 12] = head[16..28].try_into().unwrap();
        let iterations = u32::from_be_bytes(head[28..32].try_into().unwrap());
        let ct_len = size - HEADER - TAG;

        job.report(Phase::DerivingKey, 0, 0);
        let key = pbkdf2_sha256(password, salt, iterations).map_err(|_| Error::NotAVault)?;

        // Cheap check (same as 1.x): a ZIP starts with "PK".
        let mut first = [0u8; 2];
        file.read_exact(&mut first).ctx(|| "cannot read vault".into())?;
        let mut ctr = Ctr::new(key.as_ref().into(), &data_counter_block(&iv).into());
        ctr.apply_keystream(&mut first);
        if &first != b"PK" {
            return Err(Error::WrongPassword);
        }

        // Full authentication before any plaintext is released.
        let mut stored = [0u8; 16];
        file.seek(SeekFrom::Start(size - TAG)).ctx(|| "cannot read vault".into())?;
        file.read_exact(&mut stored).ctx(|| "cannot read vault".into())?;
        let computed = compute_tag(&key, &iv, &mut file, ct_len, job)?;
        if !bool::from(computed.ct_eq(&stored)) {
            return Err(Error::Corrupted("authentication tag mismatch"));
        }

        let ctr = Ctr::new(key.as_ref().into(), &data_counter_block(&iv).into());
        drop(key);
        let reader = BufReader::with_capacity(256 * 1024, CtrReader { file, len: ct_len, pos: 0, ctr });
        let mut zip = zip::ZipArchive::new(reader).map_err(|_| Error::Corrupted("invalid archive inside vault"))?;

        let mut entries = Vec::with_capacity(zip.len());
        for index in 0..zip.len() {
            let f = zip.by_index_raw(index).map_err(|_| Error::Corrupted("invalid archive entry"))?;
            let path = f.name().to_string();
            entries.push(EntryInfo {
                index,
                unsafe_path: f.is_symlink() || sanitize_entry(&path).is_err(),
                is_dir: f.is_dir(),
                size: f.size(),
                modified: None,
                path,
            });
        }
        Ok(V1Vault { zip, entries, iterations })
    }

    pub fn entries(&self) -> &[EntryInfo] {
        &self.entries
    }

    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    pub fn is_executable(&mut self, index: usize) -> bool {
        self.zip.by_index_raw(index).ok().and_then(|f| f.unix_mode()).is_some_and(|m| m & 0o111 != 0)
    }

    pub fn copy_entry(&mut self, index: usize, out: &mut dyn Write, job: &Job, base: u64, total: u64) -> Result<u64> {
        let mut f = self.zip.by_index(index).map_err(|_| Error::EntryNotFound)?;
        if f.is_dir() {
            return Ok(0);
        }
        let mut buf = Zeroizing::new(vec![0u8; 256 * 1024]);
        let mut done = 0u64;
        loop {
            job.check_cancel()?;
            // The zip crate verifies each entry's CRC-32 when it reaches the end.
            let n = f.read(&mut buf).map_err(|_| Error::Corrupted("archive entry failed integrity check"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).ctx(|| "cannot write output".into())?;
            done += n as u64;
            job.report(Phase::Decrypting, base + done, total);
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::AeadInPlace;
    use aes_gcm::Aes256Gcm;

    /// Our CTR+GHASH composition must agree byte-for-byte with the aes-gcm crate.
    #[test]
    fn streaming_gcm_matches_aes_gcm_crate() {
        for len in [0usize, 1, 15, 16, 17, 1000, (1 << 20) + 7, 3 * (1 << 20)] {
            let key: Key = Zeroizing::new([7u8; 32]);
            let iv = [9u8; 12];
            let plain: Vec<u8> = (0..len).map(|i| (i * 13 % 256) as u8).collect();
            let mut ct = plain.clone();
            let tag = Aes256Gcm::new_from_slice(key.as_ref())
                .unwrap()
                .encrypt_in_place_detached((&iv).into(), b"", &mut ct)
                .unwrap();

            let mut tmp = tempfile::tempfile().unwrap();
            tmp.write_all(&[0u8; 32]).unwrap();
            tmp.write_all(&ct).unwrap();
            let ours = compute_tag(&key, &iv, &mut tmp, len as u64, &Job::none()).unwrap();
            assert_eq!(&ours[..], &tag[..], "tag mismatch at len {len}");

            let mut r = CtrReader {
                file: tmp,
                len: len as u64,
                pos: 0,
                ctr: Ctr::new(key.as_ref().into(), &data_counter_block(&iv).into()),
            };
            if len > 20 {
                // random access must decrypt correctly
                r.seek(SeekFrom::Start(17)).unwrap();
                let mut part = [0u8; 3];
                r.read_exact(&mut part).unwrap();
                assert_eq!(&part, &plain[17..20]);
                r.seek(SeekFrom::Start(0)).unwrap();
            }
            let mut back = Vec::new();
            r.read_to_end(&mut back).unwrap();
            assert_eq!(back, plain);
        }
    }
}
