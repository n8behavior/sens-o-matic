//! How a failure reaches the client.
//!
//! A handler fails with an [`AppError`]. Its [`IntoResponse`] implementation
//! turns the error into a status code and an [`ApiError`] body, so that every
//! failure of the API has the same JSON shape:
//!
//! ```json
//! {
//!   "error": "conflict",
//!   "message": "Cannot activate the hangout: the ping is in the gathering state, and this needs the venue_confirmed state. See sens_o_matic::models::ping for the lifecycle."
//! }
//! ```
//!
//! | [`AppError`] | Status | `error` | Used when |
//! |---|---|---|---|
//! | `NotFound` | 404 | `not_found` | an id refers to nothing |
//! | `BadRequest` | 400 | `bad_request` | the body is malformed or breaks a rule of a value (see [`value`](super::value)) |
//! | `Forbidden` | 403 | `forbidden` | the user is not allowed to do this |
//! | `Conflict` | 409 | `conflict` | the ping is in a state that does not allow this |

use axum::{
    Json,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use utoipa::ToSchema;

/// The JSON body of every error response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    /// Machine-readable code of the failure: `not_found`, `bad_request`,
    /// `forbidden` or `conflict`. A client can switch on it.
    pub error: String,
    /// Explanation for a person. Its wording is not stable; do not parse it.
    pub message: String,
}

/// A JSON request body that is rejected with `400 Bad Request` in every case.
///
/// It replaces [`axum::Json`] as the extractor of request bodies. The `Json`
/// extractor answers a body of the wrong type with `422` and a missing
/// content type with `415`; with `AppJson` the client gets the [`ApiError`]
/// shape and a `400` for each of them, and the message says what axum found
/// wrong.
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
    /// Creates an error body from a code and a message.
    ///
    /// Prefer the constructor of the code you mean ([`ApiError::not_found`],
    /// [`ApiError::bad_request`], [`ApiError::forbidden`],
    /// [`ApiError::conflict`]), so that the codes stay the four that the API
    /// documents.
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }

    /// Creates a `not_found` error; `resource` is the name of the thing that
    /// is missing, and the message becomes `"{resource} not found"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sens_o_matic::models::ApiError;
    ///
    /// let error = ApiError::not_found("Ping");
    /// assert_eq!(error.error, "not_found");
    /// assert_eq!(error.message, "Ping not found");
    /// ```
    pub fn not_found(resource: &str) -> Self {
        Self::new("not_found", format!("{} not found", resource))
    }

    /// Creates a `bad_request` error with the given message.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new("bad_request", message)
    }

    /// Creates a `forbidden` error with the given message.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new("forbidden", message)
    }

    /// Creates a `conflict` error with the given message.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("conflict", message)
    }
}

/// A failure of a request, with the status code it is answered with (see the
/// [module docs](self) for the table).
///
/// # Examples
///
/// ```
/// use axum::{http::StatusCode, response::IntoResponse};
/// use sens_o_matic::models::AppError;
///
/// let response = AppError::Conflict("The ping is in the wrong state".into()).into_response();
/// assert_eq!(response.status(), StatusCode::CONFLICT);
///
/// // `NotFound` carries the name of the missing resource, not a message.
/// let response = AppError::NotFound("Group".into()).into_response();
/// assert_eq!(response.status(), StatusCode::NOT_FOUND);
/// ```
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The resource does not exist. The payload is the name of the resource
    /// (`"User"`), which [`ApiError::not_found`] turns into the message.
    #[error("Not found: {0}")]
    NotFound(String),

    /// The body is not valid JSON of the expected type, or holds a value that
    /// breaks a rule (see [`value`](super::value)). The payload is the message
    /// for the client.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// The user is not allowed to do this. The payload is the message for the
    /// client.
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// The state of the ping does not allow this. The payload is the message
    /// for the client.
    #[error("Conflict: {0}")]
    Conflict(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, ApiError::not_found(&msg)),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, ApiError::bad_request(msg)),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, ApiError::forbidden(msg)),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, ApiError::conflict(msg)),
        };
        (status, Json(error)).into_response()
    }
}
