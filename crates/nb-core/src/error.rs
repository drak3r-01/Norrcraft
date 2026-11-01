//! Erreurs partagées entre crates.

use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("erreur d'E/S sur {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("données invalides dans {context}: {message}")]
    Data { context: String, message: String },
    #[error("format de sauvegarde non supporté: version {found}, attendue {expected}")]
    SaveVersion { found: u32, expected: u32 },
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    pub fn data(context: impl Into<String>, message: impl ToString) -> Self {
        Self::Data { context: context.into(), message: message.to_string() }
    }
}
