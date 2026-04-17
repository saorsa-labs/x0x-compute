use std::path::PathBuf;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
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

    #[error("invalid {field} encoding: {details}")]
    InvalidIdentityEncoding {
        field: &'static str,
        details: String,
    },

    #[error("missing parent directory for path {0}")]
    MissingParentDirectory(PathBuf),

    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error(
        "model capacity exceeded for {model}: requested {requested_slots} slot(s), available {available_slots}"
    )]
    ModelCapacityExceeded {
        model: String,
        requested_slots: u16,
        available_slots: u16,
    },

    #[error("reservation not found: {0}")]
    ReservationNotFound(String),

    #[error("invalid reservation request: {0}")]
    InvalidReservationRequest(String),

    #[error("invalid chat request: {0}")]
    InvalidChatRequest(String),

    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
}

impl IntoResponse for ComputeError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            Self::ModelNotFound(_) | Self::ReservationNotFound(_) => {
                (StatusCode::NOT_FOUND, "not_found")
            }
            Self::ModelCapacityExceeded { .. } => (StatusCode::CONFLICT, "capacity_exceeded"),
            Self::InvalidReservationRequest(_)
            | Self::InvalidChatRequest(_)
            | Self::UserIdentityRequired
            | Self::InvalidIdentityEncoding { .. }
            | Self::MissingParentDirectory(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::UnsupportedFeature(_) => (StatusCode::NOT_IMPLEMENTED, "unsupported_feature"),
            Self::X0xIdentity(_)
            | Self::Io(_)
            | Self::Json(_)
            | Self::TomlDecode(_)
            | Self::TomlEncode(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let body = Json(serde_json::json!({
            "error": {
                "type": error_type,
                "message": self.to_string(),
            }
        }));

        (status, body).into_response()
    }
}
