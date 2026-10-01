use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

/// Message used when a request fails unexpectedly (matches the Java edition).
pub const INTERNAL_ERROR: &str = "An unexpected internal server error occurred";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ResultType {
    Success,
    Error,
    Void,
}

/// Response envelope of every mutating API call:
/// `{"type": "SUCCESS" | "ERROR" | "VOID", "message": ..., "data": ..., "notification": bool}`.
///
/// `SUCCESS` maps to 200, `ERROR` to 400 and `VOID` to 202.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApiResult {
    #[serde(rename = "type")]
    pub kind: ResultType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Whether the UI should show the message as a toast.
    pub notification: bool,
}

impl ApiResult {
    fn new(kind: ResultType, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: Some(message.into()),
            data: None,
            notification: true,
        }
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(ResultType::Success, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(ResultType::Error, message)
    }

    /// `{"type":"ERROR","message":"Validation failed","data":{field: message}}`.
    pub fn validation(field: &str, message: &str) -> Self {
        Self::error("Validation failed").with_data(serde_json::json!({ field: message }))
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_notification(mut self, notification: bool) -> Self {
        self.notification = notification;
        self
    }

    pub fn is_success(&self) -> bool {
        self.kind == ResultType::Success
    }

    pub fn status(&self) -> StatusCode {
        match self.kind {
            ResultType::Success => StatusCode::OK,
            ResultType::Error => StatusCode::BAD_REQUEST,
            ResultType::Void => StatusCode::ACCEPTED,
        }
    }
}

impl IntoResponse for ApiResult {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}

/// `404 {"type":"ERROR","message":"Not found"}`.
pub fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(ApiResult::error("Not found"))).into_response()
}
