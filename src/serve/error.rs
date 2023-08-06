use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

// Make our own error that wraps `anyhow::Error`.
pub struct BoxError(color_eyre::Report);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for BoxError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for BoxError
where
    E: Into<color_eyre::Report>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
