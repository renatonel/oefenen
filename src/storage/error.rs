//! Errors produced by the storage layer.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Everything that can go wrong while reading or writing data files.
///
/// Application level errors are wrapped with `anyhow`; this type stays
/// specific so callers can react to individual cases.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("kan het gegevenspad niet bepalen ({0})")]
    DataDirUnavailable(String),

    #[error("ongeldige {kind}-identifier {value:?}: {reason}")]
    InvalidId {
        kind: &'static str,
        value: String,
        reason: String,
    },

    #[error("pad {path:?} ligt buiten {base:?}")]
    PathEscapes { base: PathBuf, path: PathBuf },

    #[error("profiel {0:?} bestaat niet")]
    ProfileNotFound(String),

    #[error("er bestaat al een profiel met identifier {0:?}")]
    DuplicateId(String),

    #[error("onleesbaar TOML-bestand {path:?}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("TOML kon niet worden geschreven naar {path:?}: {source}")]
    TomlWrite {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },

    #[error("ongeldig profiel: {0}")]
    InvalidProfile(#[from] crate::profile::ValidationError),

    #[error("ongeldige JSON-regel {line_number} in {path:?}: {source}")]
    JsonLine {
        path: PathBuf,
        line_number: usize,
        #[source]
        source: serde_json::Error,
    },

    #[error("JSON kon niet worden geschreven naar {path:?}: {source}")]
    JsonWrite {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("onbekend schemaformaat (versie {found:?}, ondersteund: {supported})")]
    UnsupportedSchema { found: String, supported: u32 },

    #[error("bestandsoperatie op {path:?} mislukt: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl StorageError {
    /// Wraps an [`io::Error`] together with the path it happened on.
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
