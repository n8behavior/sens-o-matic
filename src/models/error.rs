use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

/// Custom JSON extractor that returns 400 instead of 422 for JSON errors
#[derive(Debug)]
pub struct AppJson<T>(pub T);

impl<S, T> FromRequest<S> for AppJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(AppJson(value)),
            Err(rejection) => Err(AppError::BadRequest(rejection.body_text())),
        }
    }
}

impl ApiError {
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }

    pub fn not_found(resource: &str) -> Self {
        Self::new("not_found", format!("{} not found", resource))
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new("bad_request", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new("forbidden", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("conflict", message)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, ApiError::not_found(&msg)),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, ApiError::bad_request(msg)),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, ApiError::forbidden(msg)),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, ApiError::conflict(msg)),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, ApiError::bad_request(msg)),
        };
        (status, Json(error)).into_response()
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(errors: validator::ValidationErrors) -> Self {
        AppError::Validation(errors.to_string())
    }
}
