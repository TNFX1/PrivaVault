//! Crash-safe output files.
//!
//! Output is written to a hidden `*.pvpart` sibling created with `O_EXCL` and owner-only
//! permissions, then renamed into place only after everything succeeded. If the process fails,
//! is cancelled or panics, the partial file is removed on drop, so a half-written vault or a
//! half-extracted plaintext file is never left behind under the real name.

use crate::error::{Error, IoContext, Result};
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

pub struct AtomicFile {
    tmp: PathBuf,
    dest: PathBuf,
    writer: Option<BufWriter<File>>,
    /// Only meaningful on Unix (mode 0700); Windows has no exec bit.
    #[cfg_attr(not(unix), allow(dead_code))]
    executable: bool,
}

impl AtomicFile {
    pub fn create(dest: &Path) -> Result<Self> {
        let dir = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut rnd = [0u8; 6];
        getrandom::fill(&mut rnd).map_err(|_| Error::InvalidParams("OS random generator failed"))?;
        let suffix: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
        let tmp = dir.join(format!(".{name}.{suffix}.pvpart"));
        let file = open_private(&tmp).ctx(|| format!("cannot create {}", tmp.display()))?;
        Ok(Self {
            tmp,
            dest: dest.to_path_buf(),
            writer: Some(BufWriter::with_capacity(256 * 1024, file)),
            executable: false,
        })
    }

    pub fn writer(&mut self) -> &mut BufWriter<File> {
        self.writer.as_mut().expect("writer present until commit")
    }

    pub fn set_executable(&mut self, exec: bool) {
        self.executable = exec;
    }

    /// Flush, fsync and move into place.
    ///
    /// `overwrite = true` is used only when the user explicitly picked the destination in a
    /// native save dialog (which already asked for confirmation). Otherwise an existing file is
    /// never replaced.
    pub fn commit(mut self, overwrite: bool) -> Result<PathBuf> {
        let w = self.writer.take().expect("commit once");
        let file = w.into_inner().map_err(|e| e.into_error()).ctx(|| "cannot flush output".into())?;
        file.sync_all().ctx(|| "cannot sync output to disk".into())?;
        drop(file);
        #[cfg(unix)]
        if self.executable {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.tmp, std::fs::Permissions::from_mode(0o700));
        }
        if overwrite {
            std::fs::rename(&self.tmp, &self.dest).ctx(|| format!("cannot write {}", self.dest.display()))?;
        } else {
            rename_no_clobber(&self.tmp, &self.dest)?;
        }
        Ok(std::mem::take(&mut self.dest))
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        // Not committed (error, cancel or panic): remove the partial output.
        if self.writer.is_some() || self.tmp.exists() {
            self.writer.take();
            let _ = std::fs::remove_file(&self.tmp);
        }
    }
}

/// Atomic "create, never replace": a hard link fails with `AlreadyExists` if `dest` exists.
/// Filesystems without hard links (FAT/exFAT USB sticks) fall back to check-then-rename.
fn rename_no_clobber(tmp: &Path, dest: &Path) -> Result<()> {
    match std::fs::hard_link(tmp, dest) {
        Ok(()) => {
            let _ = std::fs::remove_file(tmp);
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            Err(e).ctx(|| format!("{} already exists", dest.display()))
        }
        Err(_) => {
            if std::fs::symlink_metadata(dest).is_ok() {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists))
                    .ctx(|| format!("{} already exists", dest.display()));
            }
            std::fs::rename(tmp, dest).ctx(|| format!("cannot write {}", dest.display()))
        }
    }
}

/// Create a new file readable/writable by the owner only (0600 on Unix).
/// On Windows, files inherit the ACL of the destination directory (per-user by default
/// under the profile); see docs/SECURITY_AUDIT.md.
pub fn open_private(path: &Path) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}

/// Create a directory (and parents) with owner-only permissions on Unix.
pub fn create_private_dir_all(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

/// Write `data` to `dest` atomically with private permissions, replacing any existing file.
pub fn write_private_atomic(dest: &Path, data: &[u8]) -> Result<()> {
    let mut f = AtomicFile::create(dest)?;
    f.writer().write_all(data).ctx(|| format!("cannot write {}", dest.display()))?;
    f.commit(true)?;
    Ok(())
}

/// Delete the files a vault was just made from, after the caller verified the vault.
///
/// Only regular files that are listed in `files` are deleted; then folders among `inputs` are
/// removed bottom-up **only if they ended up empty** (anything skipped during encryption — a
/// symlink, a file that couldn't be read — keeps its folder). Returns the paths left behind.
/// Note: on SSDs and copy-on-write filesystems deleted data may stay recoverable.
pub fn remove_originals(inputs: &[std::path::PathBuf], files: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let mut left = Vec::new();
    for f in files {
        if std::fs::symlink_metadata(f).map(|m| m.is_file()).unwrap_or(false) && std::fs::remove_file(f).is_err() {
            left.push(f.clone());
        }
    }
    fn prune(dir: &std::path::Path, left: &mut Vec<std::path::PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for child in read.flatten() {
            if child.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                prune(&child.path(), left);
            }
        }
        if std::fs::remove_dir(dir).is_err() {
            left.push(dir.to_path_buf());
        }
    }
    for input in inputs {
        if std::fs::symlink_metadata(input).map(|m| m.is_dir()).unwrap_or(false) {
            prune(input, &mut left);
        }
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncommitted_file_is_removed() {
        let d = tempfile::tempdir().unwrap();
        let dest = d.path().join("out.bin");
        {
            let mut f = AtomicFile::create(&dest).unwrap();
            f.writer().write_all(b"partial").unwrap();
        }
        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0, "no .pvpart left behind");
    }

    #[test]
    fn commit_never_clobbers_unless_asked() {
        let d = tempfile::tempdir().unwrap();
        let dest = d.path().join("out.bin");
        std::fs::write(&dest, b"user data").unwrap();
        let mut f = AtomicFile::create(&dest).unwrap();
        f.writer().write_all(b"new").unwrap();
        assert_eq!(f.commit(false).unwrap_err().code(), "io_exists");
        assert_eq!(std::fs::read(&dest).unwrap(), b"user data");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);

        let mut f = AtomicFile::create(&dest).unwrap();
        f.writer().write_all(b"new").unwrap();
        f.commit(true).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let dest = d.path().join("secret");
        write_private_atomic(&dest, b"x").unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
