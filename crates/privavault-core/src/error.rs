use std::io;
use std::path::PathBuf;

/// Errors produced by the vault engine.
///
/// Every variant maps to a stable machine-readable [`Error::code`] so the UI can show a
/// localised, *accurate* message (the 1.x app reported every failure as "wrong password").
/// Messages never contain passwords, keys or decrypted content.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the password is incorrect")]
    WrongPassword,

    #[error("the vault is damaged or has been modified ({0})")]
    Corrupted(&'static str),

    #[error("this file is not a PrivaVault vault")]
    NotAVault,

    #[error("unsupported vault format or parameters ({0})")]
    Unsupported(&'static str),

    #[error("the operation was cancelled")]
    Cancelled,

    #[error("unsafe path inside vault rejected: {0:?}")]
    UnsafePath(String),

    #[error("file changed while it was being encrypted: {0}")]
    FileChanged(PathBuf),

    #[error("the output would overwrite one of the input files: {0}")]
    OutputIsInput(PathBuf),

    #[error("nothing to encrypt")]
    EmptyInput,

    #[error("entry not found in vault")]
    EntryNotFound,

    #[error("invalid parameters: {0}")]
    InvalidParams(&'static str),

    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

impl Error {
    /// Stable identifier for UI localisation.
    pub fn code(&self) -> &'static str {
        match self {
            Error::WrongPassword => "wrong_password",
            Error::Corrupted(_) => "corrupted",
            Error::NotAVault => "not_a_vault",
            Error::Unsupported(_) => "unsupported",
            Error::Cancelled => "cancelled",
            Error::UnsafePath(_) => "unsafe_path",
            Error::FileChanged(_) => "file_changed",
            Error::OutputIsInput(_) => "output_is_input",
            Error::EmptyInput => "empty_input",
            Error::EntryNotFound => "entry_not_found",
            Error::InvalidParams(_) => "invalid_params",
            Error::Io { source, .. } => match source.kind() {
                io::ErrorKind::NotFound => "io_not_found",
                io::ErrorKind::PermissionDenied => "io_permission",
                io::ErrorKind::AlreadyExists => "io_exists",
                io::ErrorKind::StorageFull => "io_disk_full",
                _ => "io",
            },
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Attach human context to an `io::Error`.
pub(crate) trait IoContext<T> {
    fn ctx(self, context: impl FnOnce() -> String) -> Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn ctx(self, context: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|source| Error::Io { context: context(), source })
    }
}
