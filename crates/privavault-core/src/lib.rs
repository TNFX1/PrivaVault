//! PrivaVault core: vault formats, cryptography and safe extraction.
//!
//! This crate has no GUI or platform-framework dependencies so it can be tested on any OS
//! with `cargo test` and reused by other front-ends.
//!
//! * [`create_vault`] — encrypt files/folders into a new v2 vault.
//! * [`Vault::open`] — open a v1 (PrivaVault 1.x) or v2 vault, list its entries, extract.

pub mod error;
pub mod fsutil;
pub mod kdf;
pub mod paths;
pub mod v1;
pub mod v2;

pub use error::{Error, Result};
pub use fsutil::remove_originals;
pub use kdf::{ScryptParams, Strength};
pub use v2::CreateReport;

use error::IoContext;
use fsutil::AtomicFile;
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// What the engine is currently doing (for progress display).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    DerivingKey,
    Verifying,
    Encrypting,
    Decrypting,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
}

/// Cancellation flag and progress sink for a long-running operation.
pub struct Job<'a> {
    cancel: Option<&'a AtomicBool>,
    progress: Option<&'a dyn Fn(Progress)>,
}

impl<'a> Job<'a> {
    pub fn new(cancel: &'a AtomicBool, progress: &'a dyn Fn(Progress)) -> Self {
        Self { cancel: Some(cancel), progress: Some(progress) }
    }

    pub fn none() -> Job<'static> {
        Job { cancel: None, progress: None }
    }

    pub(crate) fn check_cancel(&self) -> Result<()> {
        match self.cancel {
            Some(c) if c.load(Ordering::Relaxed) => Err(Error::Cancelled),
            _ => Ok(()),
        }
    }

    pub(crate) fn report(&self, phase: Phase, done: u64, total: u64) {
        if let Some(p) = self.progress {
            p(Progress { phase, done, total });
        }
    }
}

/// One file or folder inside a vault.
#[derive(Debug, Clone, Serialize)]
pub struct EntryInfo {
    pub index: usize,
    /// Path inside the vault, `/`-separated. Untrusted: display as text only.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since the Unix epoch, if recorded.
    pub modified: Option<i64>,
    /// The name is unsafe to extract (traversal, absolute, symlink…) and will be skipped.
    pub unsafe_path: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    V1,
    V2,
}

/// Identify a vault's format from its header without a password.
pub fn detect_format(path: &Path) -> Result<Format> {
    let mut f = File::open(path).ctx(|| format!("cannot open {}", path.display()))?;
    let mut magic = [0u8; 8];
    let n = f.read(&mut magic).ctx(|| format!("cannot read {}", path.display()))?;
    if n == 8 && &magic == v2::MAGIC {
        return Ok(Format::V2);
    }
    // v1 has no magic number; it is the fallback for anything big enough to hold its header.
    let len = f.metadata().ctx(|| format!("cannot read {}", path.display()))?.len();
    if len >= 52 {
        Ok(Format::V1)
    } else {
        Err(Error::NotAVault)
    }
}

/// Encrypt `inputs` (files and/or folders) into a new v2 vault at `output`.
///
/// `output` is written atomically; it replaces an existing file only because the caller
/// obtained it from a save dialog that already confirmed the overwrite.
pub fn create_vault(
    inputs: &[PathBuf],
    output: &Path,
    password: &str,
    kdf: ScryptParams,
    job: &Job,
) -> Result<CreateReport> {
    v2::create(inputs, output, password, kdf, job)
}

pub enum Vault {
    V1(v1::V1Vault),
    V2(v2::V2Vault),
}

#[derive(Debug, Serialize)]
pub struct ExtractReport {
    pub root: PathBuf,
    pub files: usize,
    pub folders: usize,
    pub bytes: u64,
    /// Entries refused because their names were unsafe.
    pub skipped_unsafe: Vec<String>,
}

impl Vault {
    pub fn open(path: &Path, password: &str, job: &Job) -> Result<Vault> {
        if password.is_empty() {
            return Err(Error::InvalidParams("empty password"));
        }
        let format = detect_format(path)?;
        let file = File::open(path).ctx(|| format!("cannot open {}", path.display()))?;
        Ok(match format {
            Format::V1 => Vault::V1(v1::V1Vault::open(file, password, job)?),
            Format::V2 => Vault::V2(v2::V2Vault::open(file, password, job)?),
        })
    }

    pub fn format(&self) -> Format {
        match self {
            Vault::V1(_) => Format::V1,
            Vault::V2(_) => Format::V2,
        }
    }

    /// Human-readable description of the key derivation protecting this vault.
    pub fn kdf_description(&self) -> String {
        match self {
            Vault::V1(v) => format!("PBKDF2-HMAC-SHA256, {} iterations", v.iterations()),
            Vault::V2(v) => {
                let k = v.kdf();
                format!("scrypt (N=2^{}, r={}, p={}, {} MiB)", k.log_n, k.r, k.p, k.memory_bytes() >> 20)
            }
        }
    }

    pub fn entries(&self) -> &[EntryInfo] {
        match self {
            Vault::V1(v) => v.entries(),
            Vault::V2(v) => v.entries(),
        }
    }

    fn is_executable(&mut self, index: usize) -> bool {
        match self {
            Vault::V1(v) => v.is_executable(index),
            Vault::V2(v) => v.is_executable(index),
        }
    }

    fn copy_entry(&mut self, index: usize, out: &mut dyn Write, job: &Job, base: u64, total: u64) -> Result<u64> {
        match self {
            Vault::V1(v) => v.copy_entry(index, out, job, base, total),
            Vault::V2(v) => v.copy_entry(index, out, job, base, total),
        }
    }

    /// Write this vault's contents — minus `remove` (vault paths; a folder takes its contents with
    /// it), plus `add` (files/folders from disk, at the top level) — to a new v2 vault at `output`
    /// protected by `password`. Used to add/remove files and to change the password; v1 vaults come
    /// out as v2. `output` may be this vault's own file: it is only replaced once the new one is
    /// complete, and the caller reopens it afterwards.
    pub fn rewrite(
        &mut self,
        output: &Path,
        password: &str,
        kdf: ScryptParams,
        remove: &[String],
        add: &[PathBuf],
        job: &Job,
    ) -> Result<CreateReport> {
        let removed = |path: &str| remove.iter().any(|r| path == r || path.starts_with(&format!("{r}/")));
        let mut entries = Vec::new();
        let mut sources: Vec<Option<usize>> = Vec::new(); // index in this vault, None for folders
        let mut taken = std::collections::HashSet::new();
        for e in self.entries().to_vec() {
            if removed(&e.path) {
                continue;
            }
            taken.insert(e.path.split('/').next().unwrap_or("").to_lowercase());
            let exec = !e.is_dir && self.is_executable(e.index);
            entries.push(v2::ManifestEntry {
                path: e.path.clone(),
                kind: if e.is_dir { v2::EntryKind::Dir } else { v2::EntryKind::File },
                size: if e.is_dir { 0 } else { e.size },
                mtime: e.modified,
                exec,
            });
            sources.push((!e.is_dir).then_some(e.index));
        }
        let added = if add.is_empty() { v2::Collected::default() } else { v2::collect_inputs_beside(add, taken)? };
        if let Some(src) = added.sources.iter().flatten().find(|s| v2::same_file_pub(s, output)) {
            return Err(Error::OutputIsInput(src.clone()));
        }
        let first_added = entries.len();
        entries.extend(added.entries.iter().cloned());
        if entries.is_empty() {
            return Err(Error::EmptyInput);
        }
        let total: u64 = entries.iter().filter(|e| e.kind == v2::EntryKind::File).map(|e| e.size).sum();
        let mut done = 0u64;
        let output = v2::write_vault(&entries, output, password, kdf, job, |i, out, job| {
            if i < first_added {
                let index = sources[i].expect("file entries have a source");
                done += self.copy_entry(index, out, job, done, total)?;
                Ok(())
            } else {
                let src = added.sources[i - first_added].as_ref().expect("file entries have a source");
                v2::copy_file(src, entries[i].size, out, job, &mut done, total)
            }
        })?;
        Ok(CreateReport {
            output,
            files: entries.iter().filter(|e| e.kind == v2::EntryKind::File).count(),
            folders: entries.iter().filter(|e| e.kind == v2::EntryKind::Dir).count(),
            bytes: total,
            encrypted: added.sources.iter().flatten().cloned().collect(),
            skipped: added.skipped,
        })
    }

    /// Decrypt and authenticate every file without writing anything. Returns the bytes checked.
    pub fn verify(&mut self, job: &Job) -> Result<u64> {
        let files: Vec<(usize, u64)> = self.entries().iter().filter(|e| !e.is_dir).map(|e| (e.index, e.size)).collect();
        let total = files.iter().map(|f| f.1).sum();
        let mut done = 0;
        for (index, _) in files {
            done += self.copy_entry(index, &mut std::io::sink(), job, done, total)?;
        }
        Ok(done)
    }

    /// One file's contents in memory (for previews), refused above `max` bytes.
    pub fn read_entry(&mut self, index: usize, max: u64, job: &Job) -> Result<zeroize::Zeroizing<Vec<u8>>> {
        let e = self.entry(index)?;
        if e.is_dir {
            return Err(Error::InvalidParams("entry is a folder"));
        }
        if e.size > max {
            return Err(Error::InvalidParams("too large to preview"));
        }
        let mut buf = zeroize::Zeroizing::new(Vec::with_capacity(e.size as usize));
        self.copy_entry(index, &mut *buf, job, 0, e.size)?;
        Ok(buf)
    }

    fn entry(&self, index: usize) -> Result<EntryInfo> {
        self.entries().get(index).cloned().ok_or(Error::EntryNotFound)
    }

    /// Decrypt one file to `dest` (chosen by the user in a save dialog).
    pub fn extract_entry(&mut self, index: usize, dest: &Path, job: &Job) -> Result<PathBuf> {
        let e = self.entry(index)?;
        if e.unsafe_path {
            return Err(Error::UnsafePath(e.path));
        }
        if e.is_dir {
            return Err(Error::InvalidParams("entry is a folder"));
        }
        let mut out = AtomicFile::create(dest)?;
        self.copy_entry(index, out.writer(), job, 0, e.size)?;
        out.set_executable(self.is_executable(index));
        job.check_cancel()?;
        out.commit(true)
    }

    /// Extract every safe entry into a **new** folder `parent/folder_name` (suffixed `(2)`, `(3)`…
    /// if it exists). Existing files are never overwritten; unsafe entries are skipped and reported.
    pub fn extract_all(&mut self, parent: &Path, folder_name: &str, job: &Job) -> Result<ExtractReport> {
        let root = paths::unique_path(parent, folder_name);
        fsutil::create_private_dir_all(&root).ctx(|| format!("cannot create {}", root.display()))?;
        let result = self.extract_into(&root, job);
        if result.is_err() {
            // Leave nothing half-extracted behind (only removes the folder we just created).
            let _ = std::fs::remove_dir_all(&root);
        }
        result
    }

    fn extract_into(&mut self, root: &Path, job: &Job) -> Result<ExtractReport> {
        let entries = self.entries().to_vec();
        let total: u64 = entries.iter().filter(|e| !e.unsafe_path).map(|e| e.size).sum();
        let mut report =
            ExtractReport { root: root.to_path_buf(), files: 0, folders: 0, bytes: 0, skipped_unsafe: Vec::new() };
        for e in &entries {
            job.check_cancel()?;
            if e.unsafe_path {
                report.skipped_unsafe.push(e.path.clone());
                continue;
            }
            let target = paths::join_safe(root, &e.path)?;
            if e.is_dir {
                std::fs::create_dir_all(&target).ctx(|| format!("cannot create {}", target.display()))?;
                report.folders += 1;
                continue;
            }
            let parent = target.parent().unwrap_or(root);
            std::fs::create_dir_all(parent).ctx(|| format!("cannot create {}", parent.display()))?;
            // Duplicate names inside one archive get a numbered sibling instead of clobbering.
            let name = paths::display_name(&target);
            let target = paths::unique_path(parent, &name);
            let mut out = AtomicFile::create(&target)?;
            report.bytes += self.copy_entry(e.index, out.writer(), job, report.bytes, total)?;
            out.set_executable(self.is_executable(e.index));
            out.commit(false)?;
            report.files += 1;
        }
        Ok(report)
    }

    /// Decrypt everything into a standard (unencrypted) ZIP file chosen by the user.
    pub fn export_zip(&mut self, dest: &Path, job: &Job) -> Result<ExtractReport> {
        use zip::write::SimpleFileOptions;
        let entries = self.entries().to_vec();
        let total: u64 = entries.iter().filter(|e| !e.unsafe_path).map(|e| e.size).sum();
        let mut report =
            ExtractReport { root: dest.to_path_buf(), files: 0, folders: 0, bytes: 0, skipped_unsafe: Vec::new() };
        let mut out = AtomicFile::create(dest)?;
        {
            let mut zw = zip::ZipWriter::new(out.writer());
            let zerr = |_| Error::Io { context: "cannot write ZIP".into(), source: std::io::Error::other("zip") };
            for e in &entries {
                job.check_cancel()?;
                if e.unsafe_path {
                    report.skipped_unsafe.push(e.path.clone());
                    continue;
                }
                let name = paths::sanitize_entry(&e.path)?.join("/");
                if e.is_dir {
                    zw.add_directory(name, SimpleFileOptions::default()).map_err(zerr)?;
                    report.folders += 1;
                    continue;
                }
                let mode = if self.is_executable(e.index) { 0o700 } else { 0o600 };
                let opts = SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored)
                    .large_file(e.size >= u32::MAX as u64)
                    .unix_permissions(mode);
                zw.start_file(name, opts).map_err(zerr)?;
                report.bytes += self.copy_entry(e.index, &mut zw, job, report.bytes, total)?;
                report.files += 1;
            }
            zw.finish().map_err(zerr)?;
        }
        out.commit(true)?;
        Ok(report)
    }
}
