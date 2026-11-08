//! Vault format v2 (PrivaVault 2.x).
//!
//! ```text
//! offset size  field
//!   0     8    magic "PRIVAVLT"
//!   8     1    format version = 2
//!   9     1    KDF id         = 1 (scrypt)
//!  10     1    scrypt log2(N)
//!  11     1    scrypt r
//!  12     1    scrypt p
//!  13     1    cipher id      = 1 (AES-256-GCM, STREAM-BE32)
//!  14     1    log2(chunk size), 12..=24 (default 16 = 64 KiB)
//!  15     1    reserved (0)
//!  16    16    salt (random)
//!  32     7    STREAM nonce prefix (random)
//!  39     1    reserved (0)
//!  40     8    manifest length (u64 BE)
//!  48     8    total plaintext length = manifest + file data (u64 BE)
//!  56     8    reserved (0)
//!  64    16    key check = HMAC-SHA256(K_check, header[0..64])[..16]
//!  80     …    chunks: AES-256-GCM(K_payload, nonce = prefix ‖ u32be(i) ‖ last_flag, aad = header[0..80])
//! ```
//!
//! Keys: `master = scrypt(password, salt)`, `K_payload = HKDF-SHA256(master, "PrivaVault v2 payload")`,
//! `K_check = HKDF-SHA256(master, "PrivaVault v2 key check")`.
//!
//! The plaintext stream is `manifest JSON ‖ file₀ ‖ file₁ ‖ …`. The STREAM construction
//! (Hoang, Reyhanitabar, Rogaway, Vizár 2015 — RustCrypto `aead::stream::StreamBE32`) authenticates
//! every chunk *before* its plaintext is released, binds chunk order via the counter, and detects
//! truncation via the last-chunk flag. Since the total length is in the header, the exact file size
//! is known and any chunk can be decrypted independently (fast listing and single-file extraction).

use crate::error::{Error, IoContext, Result};
use crate::fsutil::AtomicFile;
use crate::kdf::{Key, ScryptParams};
use crate::paths::sanitize_entry;
use crate::{EntryInfo, Job, Phase};
use aead::stream::{NewStream, StreamBE32, StreamPrimitive};
use aes_gcm::{Aes256Gcm, KeyInit};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const MAGIC: &[u8; 8] = b"PRIVAVLT";
pub const VERSION: u8 = 2;
pub const HEADER_LEN: usize = 80;
const KDF_SCRYPT: u8 = 1;
const CIPHER_AES256GCM_STREAM_BE32: u8 = 1;
pub const DEFAULT_CHUNK_LOG2: u8 = 16;
const TAG_LEN: u64 = 16;
const MAX_MANIFEST_LEN: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
struct Header {
    kdf: ScryptParams,
    chunk_log2: u8,
    salt: [u8; 16],
    nonce: [u8; 7],
    manifest_len: u64,
    total_len: u64,
    key_check: [u8; 16],
}

impl Header {
    fn prefix(&self) -> [u8; 64] {
        let mut h = [0u8; 64];
        h[0..8].copy_from_slice(MAGIC);
        h[8] = VERSION;
        h[9] = KDF_SCRYPT;
        h[10] = self.kdf.log_n;
        h[11] = self.kdf.r as u8;
        h[12] = self.kdf.p as u8;
        h[13] = CIPHER_AES256GCM_STREAM_BE32;
        h[14] = self.chunk_log2;
        h[16..32].copy_from_slice(&self.salt);
        h[32..39].copy_from_slice(&self.nonce);
        h[40..48].copy_from_slice(&self.manifest_len.to_be_bytes());
        h[48..56].copy_from_slice(&self.total_len.to_be_bytes());
        h
    }

    fn encode(&self) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[..64].copy_from_slice(&self.prefix());
        h[64..80].copy_from_slice(&self.key_check);
        h
    }

    fn parse(h: &[u8; HEADER_LEN]) -> Result<Header> {
        if &h[0..8] != MAGIC {
            return Err(Error::NotAVault);
        }
        if h[8] != VERSION {
            return Err(Error::Unsupported("newer vault format version"));
        }
        if h[9] != KDF_SCRYPT || h[13] != CIPHER_AES256GCM_STREAM_BE32 {
            return Err(Error::Unsupported("unknown KDF or cipher"));
        }
        if h[15] != 0 || h[39] != 0 || h[56..64].iter().any(|&b| b != 0) {
            return Err(Error::Unsupported("reserved header bytes set"));
        }
        let kdf = ScryptParams { log_n: h[10], r: h[11] as u32, p: h[12] as u32 };
        kdf.validate()?;
        let chunk_log2 = h[14];
        if !(12..=24).contains(&chunk_log2) {
            return Err(Error::Unsupported("chunk size out of range"));
        }
        let u64_at = |o: usize| u64::from_be_bytes(h[o..o + 8].try_into().unwrap());
        let header = Header {
            kdf,
            chunk_log2,
            salt: h[16..32].try_into().unwrap(),
            nonce: h[32..39].try_into().unwrap(),
            manifest_len: u64_at(40),
            total_len: u64_at(48),
            key_check: h[64..80].try_into().unwrap(),
        };
        if header.manifest_len == 0 || header.manifest_len > MAX_MANIFEST_LEN || header.manifest_len > header.total_len
        {
            return Err(Error::Corrupted("invalid manifest length"));
        }
        let chunks = header.chunk_count();
        if chunks > u32::MAX as u64 {
            return Err(Error::Unsupported("vault too large"));
        }
        Ok(header)
    }

    fn chunk_size(&self) -> u64 {
        1u64 << self.chunk_log2
    }

    fn chunk_count(&self) -> u64 {
        self.total_len.div_ceil(self.chunk_size()).max(1)
    }

    fn expected_file_len(&self) -> Option<u64> {
        let overhead = self.chunk_count().checked_mul(TAG_LEN)?;
        (HEADER_LEN as u64).checked_add(self.total_len)?.checked_add(overhead)
    }
}

struct Keys {
    payload: Key,
    check: Key,
}

fn derive_keys(master: &Key) -> Keys {
    let hk = hkdf::Hkdf::<Sha256>::new(None, master.as_ref());
    let mut payload = Zeroizing::new([0u8; 32]);
    let mut check = Zeroizing::new([0u8; 32]);
    hk.expand(b"PrivaVault v2 payload", payload.as_mut()).expect("32 bytes is a valid HKDF length");
    hk.expand(b"PrivaVault v2 key check", check.as_mut()).expect("32 bytes is a valid HKDF length");
    Keys { payload, check }
}

fn key_check(check_key: &Key, prefix: &[u8; 64]) -> [u8; 16] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(check_key.as_ref()).expect("HMAC accepts any key length");
    mac.update(prefix);
    let tag = mac.finalize().into_bytes();
    tag[..16].try_into().unwrap()
}

fn new_stream(keys: &Keys, nonce: &[u8; 7]) -> StreamBE32<Aes256Gcm> {
    let aead = Aes256Gcm::new_from_slice(keys.payload.as_ref()).expect("32-byte key");
    StreamBE32::from_aead(aead, nonce.into())
}

// ---------------------------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Dir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exec: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    entries: Vec<ManifestEntry>,
}

// ---------------------------------------------------------------------------------------------
// Input collection
// ---------------------------------------------------------------------------------------------

/// Files and folders selected for encryption, flattened into manifest order.
#[derive(Debug, Default)]
pub struct Collected {
    pub entries: Vec<ManifestEntry>,
    pub sources: Vec<Option<PathBuf>>, // None for directories
    /// Symlinks, sockets, devices and non-UTF-8 names that were not included.
    pub skipped: Vec<PathBuf>,
    pub total_bytes: u64,
}

/// Walk the selected paths. Symlinks *inside* folders are never followed (no loops, no escaping
/// the selected tree); special files are skipped and reported.
pub fn collect_inputs(inputs: &[PathBuf]) -> Result<Collected> {
    collect_inputs_beside(inputs, std::collections::HashSet::new())
}

/// Like [`collect_inputs`], but top-level names already in `taken` (lower-cased) get a ` (2)` suffix.
pub fn collect_inputs_beside(
    inputs: &[PathBuf],
    mut top_names: std::collections::HashSet<String>,
) -> Result<Collected> {
    let mut c = Collected::default();
    for input in inputs {
        let meta = std::fs::metadata(input).ctx(|| format!("cannot read {}", input.display()))?;
        let Some(base) = input.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            c.skipped.push(input.clone());
            continue;
        };
        // Two selected items with the same name (from different folders) must not collide.
        let mut name = base.clone();
        let mut n = 2;
        while !top_names.insert(name.to_lowercase()) {
            name = match base.rfind('.') {
                Some(i) if i > 0 => format!("{} ({n}){}", &base[..i], &base[i..]),
                _ => format!("{base} ({n})"),
            };
            n += 1;
        }
        add_path(&mut c, input, &name, &meta)?;
    }
    if c.entries.is_empty() {
        return Err(Error::EmptyInput);
    }
    Ok(c)
}

fn add_path(c: &mut Collected, src: &Path, name: &str, meta: &std::fs::Metadata) -> Result<()> {
    if name.contains('\\') || sanitize_entry(name).is_err() {
        c.skipped.push(src.to_path_buf());
        return Ok(());
    }
    let mtime =
        meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64);
    if meta.is_dir() {
        c.entries.push(ManifestEntry { path: name.to_string(), kind: EntryKind::Dir, size: 0, mtime, exec: false });
        c.sources.push(None);
        let mut children: Vec<_> = std::fs::read_dir(src)
            .ctx(|| format!("cannot list {}", src.display()))?
            .collect::<std::io::Result<_>>()
            .ctx(|| format!("cannot list {}", src.display()))?;
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let path = child.path();
            let cmeta = std::fs::symlink_metadata(&path).ctx(|| format!("cannot read {}", path.display()))?;
            let Some(cname) = child.file_name().to_str().map(str::to_owned) else {
                c.skipped.push(path);
                continue;
            };
            if cmeta.file_type().is_symlink() || !(cmeta.is_file() || cmeta.is_dir()) {
                c.skipped.push(path);
                continue;
            }
            add_path(c, &path, &format!("{name}/{cname}"), &cmeta)?;
        }
    } else if meta.is_file() {
        #[cfg(unix)]
        let exec = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let exec = false;
        c.total_bytes += meta.len();
        c.entries.push(ManifestEntry { path: name.to_string(), kind: EntryKind::File, size: meta.len(), mtime, exec });
        c.sources.push(Some(src.to_path_buf()));
    } else {
        c.skipped.push(src.to_path_buf());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct CreateReport {
    pub output: PathBuf,
    pub files: usize,
    pub folders: usize,
    pub bytes: u64,
    pub skipped: Vec<PathBuf>,
    /// The files from disk that are now inside the vault (for "delete originals").
    pub encrypted: Vec<PathBuf>,
}

struct ChunkSink<'a, W: Write> {
    stream: StreamBE32<Aes256Gcm>,
    aad: [u8; HEADER_LEN],
    chunk: usize,
    total: u64,
    done: u64,
    index: u32,
    buf: Zeroizing<Vec<u8>>,
    out: &'a mut W,
}

impl<W: Write> Write for ChunkSink<'_, W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.push(data).map_err(std::io::Error::other)?;
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<W: Write> ChunkSink<'_, W> {
    fn push(&mut self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            let take = (self.chunk - self.buf.len()).min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == self.chunk {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    fn flush_chunk(&mut self) -> Result<()> {
        let after = self.done + self.buf.len() as u64;
        if after > self.total {
            return Err(Error::InvalidParams("more data than declared"));
        }
        let last = after == self.total;
        self.stream
            .encrypt_in_place(self.index, last, &self.aad, &mut *self.buf)
            .map_err(|_| Error::InvalidParams("encryption failed"))?;
        self.out.write_all(&self.buf).ctx(|| "cannot write vault".into())?;
        self.buf.clear();
        self.done = after;
        self.index = self.index.checked_add(1).ok_or(Error::Unsupported("vault too large"))?;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        if !self.buf.is_empty() || self.done < self.total {
            self.flush_chunk()?;
        }
        if self.done != self.total {
            return Err(Error::InvalidParams("less data than declared"));
        }
        Ok(())
    }
}

/// Write a new v2 vault with `entries`, asking `emit(i, out, job)` for the bytes of each file entry
/// (exactly `entries[i].size` of them). Fresh salt and nonce every time, so rewriting a vault never
/// reuses a key/nonce pair. The output is atomic: nothing replaces `output` unless all went well.
pub(crate) fn write_vault(
    entries: &[ManifestEntry],
    output: &Path,
    password: &str,
    kdf: ScryptParams,
    job: &Job,
    mut emit: impl FnMut(usize, &mut dyn Write, &Job) -> Result<()>,
) -> Result<PathBuf> {
    if password.is_empty() {
        return Err(Error::InvalidParams("empty password"));
    }
    kdf.validate()?;
    let total_files: u64 = entries.iter().filter(|e| e.kind == EntryKind::File).map(|e| e.size).sum();
    let manifest = Manifest { version: 1, entries: entries.to_vec() };
    let manifest_json = Zeroizing::new(serde_json::to_vec(&manifest).expect("manifest serialises"));
    let mut header = Header {
        kdf,
        chunk_log2: DEFAULT_CHUNK_LOG2,
        salt: random()?,
        nonce: random()?,
        manifest_len: manifest_json.len() as u64,
        total_len: manifest_json.len() as u64 + total_files,
        key_check: [0; 16],
    };
    if header.manifest_len > MAX_MANIFEST_LEN {
        return Err(Error::InvalidParams("too many files in one vault"));
    }

    job.report(Phase::DerivingKey, 0, 0);
    let master = kdf.derive(password, &header.salt)?;
    let keys = derive_keys(&master);
    drop(master);
    header.key_check = key_check(&keys.check, &header.prefix());
    let aad = header.encode();

    let mut out = AtomicFile::create(output)?;
    out.writer().write_all(&aad).ctx(|| "cannot write vault".into())?;
    let chunk = header.chunk_size() as usize;
    let mut buf = Zeroizing::new(Vec::with_capacity(chunk + TAG_LEN as usize));
    buf.clear();
    let mut sink = ChunkSink {
        stream: new_stream(&keys, &header.nonce),
        aad,
        chunk,
        total: header.total_len,
        done: 0,
        index: 0,
        buf,
        out: out.writer(),
    };
    sink.push(&manifest_json)?;
    job.report(Phase::Encrypting, 0, total_files);
    for (i, entry) in entries.iter().enumerate() {
        if entry.kind != EntryKind::File {
            continue;
        }
        job.check_cancel()?;
        let before = sink.done + sink.buf.len() as u64;
        emit(i, &mut sink, job)?;
        if sink.done + sink.buf.len() as u64 - before != entry.size {
            return Err(Error::InvalidParams("entry size changed while writing"));
        }
    }
    sink.finish()?;
    job.check_cancel()?;
    out.commit(true)
}

/// Copy exactly `size` bytes of `src` to `out`; a file that shrank or grew meanwhile is an error.
pub(crate) fn copy_file(
    src: &Path,
    size: u64,
    out: &mut dyn Write,
    job: &Job,
    done: &mut u64,
    total: u64,
) -> Result<()> {
    let mut f = File::open(src).ctx(|| format!("cannot open {}", src.display()))?;
    let mut read_buf = Zeroizing::new(vec![0u8; 1 << 20]);
    let mut remaining = size;
    while remaining > 0 {
        job.check_cancel()?;
        let want = remaining.min(read_buf.len() as u64) as usize;
        let n = f.read(&mut read_buf[..want]).ctx(|| format!("cannot read {}", src.display()))?;
        if n == 0 {
            return Err(Error::FileChanged(src.to_path_buf()));
        }
        out.write_all(&read_buf[..n]).ctx(|| "cannot write vault".into())?;
        remaining -= n as u64;
        *done += n as u64;
        job.report(Phase::Encrypting, *done, total);
    }
    if f.read(&mut read_buf[..1]).ctx(|| format!("cannot read {}", src.display()))? != 0 {
        return Err(Error::FileChanged(src.to_path_buf()));
    }
    Ok(())
}

pub fn create(inputs: &[PathBuf], output: &Path, password: &str, kdf: ScryptParams, job: &Job) -> Result<CreateReport> {
    if password.is_empty() {
        return Err(Error::InvalidParams("empty password"));
    }
    let collected = collect_inputs(inputs)?;
    if let Some(src) = collected.sources.iter().flatten().find(|s| same_file(s, output)) {
        return Err(Error::OutputIsInput(src.clone()));
    }
    let mut done = 0u64;
    let total = collected.total_bytes;
    let output = write_vault(&collected.entries, output, password, kdf, job, |i, out, job| {
        let src = collected.sources[i].as_ref().expect("file entries have a source");
        copy_file(src, collected.entries[i].size, out, job, &mut done, total)
    })?;
    Ok(CreateReport {
        output,
        files: collected.sources.iter().filter(|s| s.is_some()).count(),
        folders: collected.sources.iter().filter(|s| s.is_none()).count(),
        bytes: collected.total_bytes,
        encrypted: collected.sources.iter().flatten().cloned().collect(),
        skipped: collected.skipped,
    })
}

pub(crate) fn same_file_pub(a: &Path, b: &Path) -> bool {
    same_file(a, b)
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| Error::InvalidParams("OS random generator failed"))?;
    Ok(b)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------------------------

pub struct V2Vault {
    file: File,
    header: Header,
    aad: [u8; HEADER_LEN],
    stream: StreamBE32<Aes256Gcm>,
    entries: Vec<EntryInfo>,
    /// Plaintext offset of each entry's data.
    offsets: Vec<u64>,
    exec: Vec<bool>,
}

impl V2Vault {
    pub fn open(mut file: File, password: &str, job: &Job) -> Result<V2Vault> {
        let mut raw = [0u8; HEADER_LEN];
        file.seek(SeekFrom::Start(0)).ctx(|| "cannot read vault".into())?;
        file.read_exact(&mut raw).map_err(|_| Error::NotAVault)?;
        let header = Header::parse(&raw)?;
        let actual = file.metadata().ctx(|| "cannot read vault".into())?.len();
        if Some(actual) != header.expected_file_len() {
            return Err(Error::Corrupted("file is truncated or has extra data"));
        }

        job.report(Phase::DerivingKey, 0, 0);
        let master = header.kdf.derive(password, &header.salt)?;
        let keys = derive_keys(&master);
        drop(master);
        let expected = key_check(&keys.check, &header.prefix());
        if !bool::from(expected.ct_eq(&header.key_check)) {
            return Err(Error::WrongPassword);
        }

        let mut v = V2Vault {
            stream: new_stream(&keys, &header.nonce),
            file,
            aad: raw,
            header,
            entries: Vec::new(),
            offsets: Vec::new(),
            exec: Vec::new(),
        };
        let mut manifest_json = Zeroizing::new(Vec::with_capacity(v.header.manifest_len as usize));
        v.copy_range(0, v.header.manifest_len, &mut *manifest_json, &Job::none(), Phase::Decrypting, 0, 0)?;
        let manifest: Manifest =
            serde_json::from_slice(&manifest_json).map_err(|_| Error::Corrupted("invalid manifest"))?;
        if manifest.version != 1 {
            return Err(Error::Unsupported("unknown manifest version"));
        }

        let mut offset = v.header.manifest_len;
        for (index, e) in manifest.entries.into_iter().enumerate() {
            let size = if e.kind == EntryKind::Dir { 0 } else { e.size };
            v.offsets.push(offset);
            offset = offset.checked_add(size).ok_or(Error::Corrupted("entry sizes overflow"))?;
            v.exec.push(e.exec);
            v.entries.push(EntryInfo {
                index,
                unsafe_path: sanitize_entry(&e.path).is_err(),
                is_dir: e.kind == EntryKind::Dir,
                path: e.path,
                size,
                modified: e.mtime,
            });
        }
        if offset != v.header.total_len {
            return Err(Error::Corrupted("manifest sizes do not match vault length"));
        }
        Ok(v)
    }

    pub fn entries(&self) -> &[EntryInfo] {
        &self.entries
    }

    pub fn is_executable(&self, index: usize) -> bool {
        self.exec.get(index).copied().unwrap_or(false)
    }

    pub fn kdf(&self) -> ScryptParams {
        self.header.kdf
    }

    pub fn copy_entry(&mut self, index: usize, out: &mut dyn Write, job: &Job, base: u64, total: u64) -> Result<u64> {
        let e = self.entries.get(index).ok_or(Error::EntryNotFound)?;
        if e.is_dir {
            return Ok(0);
        }
        let (start, len) = (self.offsets[index], e.size);
        self.copy_range(start, len, out, job, Phase::Decrypting, base, total)?;
        Ok(len)
    }

    /// Decrypt plaintext bytes `[start, start+len)` into `out`. Every chunk is authenticated
    /// before any of its bytes are written.
    #[allow(clippy::too_many_arguments)]
    fn copy_range(
        &mut self,
        start: u64,
        len: u64,
        out: &mut dyn Write,
        job: &Job,
        phase: Phase,
        base: u64,
        total: u64,
    ) -> Result<()> {
        if len == 0 {
            return Ok(());
        }
        let cs = self.header.chunk_size();
        let end = start + len;
        let mut buf = Zeroizing::new(Vec::with_capacity(cs as usize + TAG_LEN as usize));
        let mut idx = start / cs;
        let mut written = 0u64;
        while idx * cs < end {
            job.check_cancel()?;
            self.read_chunk(idx, &mut buf)?;
            let chunk_start = idx * cs;
            let from = start.saturating_sub(chunk_start) as usize;
            let to = ((end - chunk_start).min(buf.len() as u64)) as usize;
            out.write_all(&buf[from..to]).ctx(|| "cannot write output".into())?;
            written += (to - from) as u64;
            job.report(phase, base + written, total);
            idx += 1;
        }
        Ok(())
    }

    fn read_chunk(&mut self, idx: u64, buf: &mut Zeroizing<Vec<u8>>) -> Result<()> {
        let cs = self.header.chunk_size();
        let count = self.header.chunk_count();
        let plain_len = if idx + 1 == count { self.header.total_len - idx * cs } else { cs };
        let pos = HEADER_LEN as u64 + idx * (cs + TAG_LEN);
        buf.clear();
        buf.resize((plain_len + TAG_LEN) as usize, 0);
        self.file.seek(SeekFrom::Start(pos)).ctx(|| "cannot read vault".into())?;
        self.file.read_exact(&mut buf[..]).map_err(|_| Error::Corrupted("unexpected end of file"))?;
        self.stream
            .decrypt_in_place(idx as u32, idx + 1 == count, &self.aad, &mut **buf)
            .map_err(|_| Error::Corrupted("chunk authentication failed"))
    }
}
