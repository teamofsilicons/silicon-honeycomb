use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use honeycomb_core::ApiError;

#[derive(Debug)]
pub struct Error(pub StatusCode, pub ApiError);
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self(
            status,
            ApiError {
                code: code.into(),
                message: message.into(),
                details: vec![],
            },
        )
    }
    pub fn require_unheld(state: &str) -> Result<()> {
        if state == "held_identifier_migration" {
            return Err(Self::new(
                StatusCode::CONFLICT,
                "identifier_migration_hold",
                "This historical operation is preserved on hold. Operator reconciliation is required before retrying; no action was replayed.",
            ));
        }
        Ok(())
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Log in with an IAM short-lived token using honeycomb login <slt>.",
        )
    }
    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Current organization owner/admin or the required reviewer authority is needed.",
        )
    }
    pub fn missing() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "The resource does not exist or is not accessible.",
        )
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "revision_conflict", message)
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "integration_unavailable",
            message,
        )
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        if e.as_database_error()
            .is_some_and(|e| e.is_unique_violation())
        {
            return Self::conflict(
                "Resource or operation already exists. Re-read its state before retrying.",
            );
        }
        tracing::error!(error=%e,"database operation failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database_error",
            "Database operation failed.",
        )
    }
}
impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!(error=%e,"internal operation failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Operation could not be completed.",
        )
    }
}
