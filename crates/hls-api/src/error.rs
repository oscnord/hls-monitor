use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_kind, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg),
        };

        let body = ErrorBody {
            error: error_kind.to_string(),
            message,
        };

        (status, axum::Json(body)).into_response()
    }
}
