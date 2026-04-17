use std::path::PathBuf;

use thiserror::Error;

/// Result type for x0x-compute operations.
pub type Result<T> = std::result::Result<T, ComputeError>;

/// Error type for x0x-compute.
#[derive(Debug, Error)]
pub enum ComputeError {
    #[error("x0x identity error: {0}")]
    X0xIdentity(#[from] x0x::error::IdentityError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("toml parse error: {0}")]
    TomlDecode(#[from] toml::de::Error),

    #[error("toml encode error: {0}")]
    TomlEncode(#[from] toml::ser::Error),

    #[error("user identity is required by configuration but no user_id is bound")]
    UserIdentityRequired,

    #[error("missing parent directory for path {0}")]
    MissingParentDirectory(PathBuf),
}
