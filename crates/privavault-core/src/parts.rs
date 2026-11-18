//! Split vaults: a finished v2 vault cut into `name.pvault.001`, `.002`, … The bytes are not
//! changed, so the parts read back as one file and `cat name.pvault.0* > name.pvault` rejoins them.
//! Nothing in a part says how many parts there are; a missing last part is caught by the v2
//! header's expected length.

use crate::error::{Error, IoContext, Result};
use crate::fsutil::AtomicFile;
use crate::{Job, Phase};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Copy buffer for splitting.
const BUF: usize = 1 << 20;

/// If `path` is one part of a split vault (`*.NNN`, three digits), all its parts in order,
/// starting at `.001` and stopping at the first missing number.
pub fn part_paths(path: &Path) -> Option<Vec<PathBuf>> {
    let name = path.file_name()?.to_str()?;
    let (base, num) = name.rsplit_once('.')?;
    if num.len() != 3 || !num.bytes().all(|b| b.is_ascii_digit()) || base.is_empty() {
        return None;
    }
    let dir = path.parent().unwrap_or(Path::new(""));
    let parts: Vec<PathBuf> = (1..=999)
        .map(|i| dir.join(format!("{base}.{i:03}")))
        .take_while(|p| p.is_file())
        .collect();
    (!parts.is_empty()).then_some(parts)
}

/// Cut the vault at `vault` into parts of at most `part_size` bytes next to it, then remove the
/// single file. Refuses if any `vault.NNN` already exists, so old parts can never mix in.
pub fn split_vault(vault: &Path, part_size: u64, job: &Job) -> Result<Vec<PathBuf>> {
    if part_size == 0 {
        return Err(Error::InvalidParams("part size must be at least one byte"));
    }
    let name = vault.file_name().and_then(|n| n.to_str()).ok_or(Error::InvalidParams("vault name is not valid text"))?;
    let dir = vault.parent().unwrap_or(Path::new(""));
    let part = |i: u64| dir.join(format!("{name}.{i:03}"));
    if part(1).exists() {
        return Err(Error::Io {
            context: format!("{} already exists", part(1).display()),
            source: io::Error::from(io::ErrorKind::AlreadyExists),
        });
    }

    let mut src = File::open(vault).ctx(|| format!("cannot open {}", vault.display()))?;
    let total = src.metadata().ctx(|| format!("cannot read {}", vault.display()))?.len();
    let count = total.div_ceil(part_size).max(1);
    if count > 999 {
        return Err(Error::InvalidParams("more than 999 parts; choose a bigger part size"));
    }

    let mut written: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<()> {
        let mut buf = vec![0u8; BUF];
        let mut done = 0u64;
        for i in 1..=count {
            let dest = part(i);
            let mut out = AtomicFile::create(&dest)?;
            let mut left = part_size.min(total - done);
            while left > 0 {
                job.check_cancel()?;
                let n = (left as usize).min(BUF);
                src.read_exact(&mut buf[..n]).ctx(|| format!("cannot read {}", vault.display()))?;
                out.writer().write_all(&buf[..n]).ctx(|| format!("cannot write {}", dest.display()))?;
                left -= n as u64;
                done += n as u64;
                job.report(Phase::Encrypting, done, total);
            }
            written.push(out.commit(false)?);
        }
        Ok(())
    })();
    if let Err(e) = result {
        for p in &written {
            let _ = std::fs::remove_file(p);
        }
        return Err(e);
    }
    std::fs::remove_file(vault).ctx(|| format!("cannot remove {}", vault.display()))?;
    Ok(written)
}

/// A vault on disk: one file, or the parts of a split vault read as one stream.
pub struct VaultFile {
    parts: Vec<(File, u64)>,
    pos: u64,
}

impl VaultFile {
    pub fn open(path: &Path) -> Result<Self> {
        let paths = part_paths(path).unwrap_or_else(|| vec![path.to_path_buf()]);
        let parts = paths
            .iter()
            .map(|p| {
                let f = File::open(p).ctx(|| format!("cannot open {}", p.display()))?;
                let len = f.metadata().ctx(|| format!("cannot read {}", p.display()))?.len();
                Ok((f, len))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(VaultFile { parts, pos: 0 })
    }

    pub fn len(&self) -> u64 {
        self.parts.iter().map(|p| p.1).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of files this vault is stored in (1 unless split).
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }
}

impl Read for VaultFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut start = 0u64;
        for (file, len) in &mut self.parts {
            if self.pos < start + *len {
                let off = self.pos - start;
                file.seek(SeekFrom::Start(off))?;
                let want = buf.len().min((*len - off) as usize);
                let n = file.read(&mut buf[..want])?;
                self.pos += n as u64;
                return Ok(n);
            }
            start += *len;
        }
        Ok(0)
    }
}

impl Seek for VaultFile {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let new = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len().checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = new.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        Ok(self.pos)
    }
}
