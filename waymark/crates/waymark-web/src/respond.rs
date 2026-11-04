//! JSON responses, one envelope shape for every answer.
//!
//! Successful answers reuse [`waymark_core::envelope::success_envelope`]; domain
//! failures reuse [`waymark_core::envelope::failure_envelope`] and add a `_meta`
//! block carrying whatever diagnostics the error already has. Adapter-level
//! failures (bad query, bad Host, unknown path) use `adapter_failure_envelope`,
//! exactly like the CLI's usage errors, so no adapter invents a registry code.

use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};

use waymark_core::envelope::{adapter_failure_envelope, failure_envelope, success_envelope, Meta};
use waymark_core::error::{Code, WaymarkError};

const JSON: &str = "application/json; charset=utf-8";

/// Serialize `value` with the given status.
pub(crate) fn json(status: StatusCode, value: Value) -> Response {
    (status, [(header::CONTENT_TYPE, JSON)], value.to_string()).into_response()
}

/// A successful envelope.
pub(crate) fn success(data: Value, meta: Meta) -> Response {
    json(StatusCode::OK, success_envelope(data, meta))
}

/// A domain failure: registered code, details, and available diagnostics.
pub(crate) fn domain_error(error: &WaymarkError) -> Response {
    let status = match error.code() {
        Code::ItemNotFound | Code::ProjectNotFound => StatusCode::NOT_FOUND,
        Code::InvalidId | Code::ValidationError => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let diagnostics = error
        .details()
        .get("diagnostics")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let mut envelope = failure_envelope(error);
    if let Value::Object(object) = &mut envelope {
        object.insert(
            "_meta".to_owned(),
            json!({
                "source": "memory_projection",
                "completeness": "scoped",
                "freshness": "unknown",
                "diagnostics": diagnostics,
                "warnings": [],
            }),
        );
    }
    json(status, envelope)
}

/// An adapter-level failure that is not a domain error.
pub(crate) fn adapter_error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
    details: Value,
) -> Response {
    json(
        status,
        adapter_failure_envelope(code, &message.into(), details, None, false),
    )
}

/// Bad request (malformed query or path).
pub(crate) fn bad_request(code: &str, message: impl Into<String>) -> Response {
    adapter_error(StatusCode::BAD_REQUEST, code, message, json!({}))
}

/// Not found; used both for unknown routes and missing assets.
pub(crate) fn not_found() -> Response {
    adapter_error(
        StatusCode::NOT_FOUND,
        "NOT_FOUND",
        "the read-only Web service has no route for that path",
        json!({}),
    )
}

/// `405` for every method that is not `GET` or `HEAD`.
pub(crate) fn method_not_allowed(method: &Method) -> Response {
    let mut response = adapter_error(
        StatusCode::METHOD_NOT_ALLOWED,
        "METHOD_NOT_ALLOWED",
        "the read-only Web service accepts only GET and HEAD",
        json!({ "method": method.as_str() }),
    );
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
    response
}
