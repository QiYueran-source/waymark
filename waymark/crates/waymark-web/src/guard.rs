//! Host and Origin validation for every request.
//!
//! The service is loopback-only, so `Host` must name `localhost` or a loopback
//! IP **at the port the listener actually bound**. Anything else is rejected
//! before routing, which defeats DNS rebinding: a hostile domain that resolves to
//! `127.0.0.1` still sends its own name in `Host`.
//!
//! When `Origin` is present (browser cross-origin requests, and every
//! `fetch` from another origin) it must be exactly `http://<host>:<port>` for the
//! host that passed the Host check. `Origin: null` and every mismatch are
//! rejected. Requests with no `Origin` — a top-level navigation, curl, the
//! browser's own same-origin navigation — proceed after the Host check. No CORS
//! response headers are emitted, so a rejected cross-origin caller gets no
//! readable answer either.
//!
//! # Status choice
//!
//! Both failures are `400 Bad Request`. They are malformed/forbidden request
//! authorities rather than a negotiation failure, and a single status is simpler
//! for a developer-facing launcher. The envelope's `code` distinguishes them:
//! `INVALID_HOST` and `INVALID_ORIGIN`.

use std::net::IpAddr;

use axum::extract::{Request, State};
use axum::http::header;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use serde_json::json;

use crate::respond;
use crate::server::AppState;

/// A validated request authority.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceHost {
    /// The host text as sent, used verbatim for the Origin comparison.
    host: String,
    port: u16,
}

/// Validate Host, then Origin, then method; pass the request through only when
/// all three agree with the running service.
pub(crate) async fn validate(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // Method first, so *every* non-GET/HEAD request is 405 without exception,
    // even one that also carries a hostile Host. Rejecting a write-shaped
    // request before any authority work is also the cheaper refusal.
    let method = request.method().clone();
    if method != axum::http::Method::GET && method != axum::http::Method::HEAD {
        return respond::method_not_allowed(&method);
    }
    let host = match validated_host(request.headers(), state.port) {
        Ok(host) => host,
        Err(reason) => {
            return respond::adapter_error(
                axum::http::StatusCode::BAD_REQUEST,
                "INVALID_HOST",
                reason,
                json!({ "expected_port": state.port }),
            )
        }
    };
    if let Err(reason) = validate_origin(request.headers(), &host) {
        return respond::adapter_error(
            axum::http::StatusCode::BAD_REQUEST,
            "INVALID_ORIGIN",
            reason,
            json!({ "expected_origin": format!("http://{}:{}", host.host, host.port) }),
        );
    }
    next.run(request).await
}

/// The `Host` header, when it names loopback at `port`.
fn validated_host(headers: &HeaderMap, port: u16) -> Result<ServiceHost, String> {
    let mut values = headers.get_all(header::HOST).iter();
    let Some(value) = values.next() else {
        return Err("the request has no Host header".to_owned());
    };
    if values.next().is_some() {
        return Err("the request has more than one Host header".to_owned());
    }
    let value = value
        .to_str()
        .map_err(|_| "the Host header is not valid ASCII".to_owned())?;
    let (host, sent_port) =
        parse_authority(value).ok_or_else(|| format!("Host {value:?} is not <host>:<port>"))?;
    if !is_loopback(&host) {
        return Err(format!(
            "Host {host:?} is not localhost or a loopback IP; this service only serves loopback"
        ));
    }
    if sent_port != port {
        return Err(format!(
            "Host port {sent_port} is not the listening port {port}"
        ));
    }
    Ok(ServiceHost { host, port })
}

/// When `Origin` is present it must be the service's own `http://host:port`.
fn validate_origin(headers: &HeaderMap, service: &ServiceHost) -> Result<(), String> {
    let mut values = headers.get_all(header::ORIGIN).iter();
    let Some(value) = values.next() else {
        return Ok(());
    };
    if values.next().is_some() {
        return Err("the request has more than one Origin header".to_owned());
    }
    let value = value
        .to_str()
        .map_err(|_| "the Origin header is not valid ASCII".to_owned())?;
    if value.eq_ignore_ascii_case("null") {
        return Err("Origin: null is not allowed".to_owned());
    }
    let authority = value
        .strip_prefix("http://")
        .ok_or_else(|| format!("Origin {value:?} is not this service's http scheme"))?;
    let (host, port) = parse_authority(authority)
        .ok_or_else(|| format!("Origin {value:?} is not http://<host>:<port>"))?;
    if host.eq_ignore_ascii_case(&service.host) && port == service.port {
        Ok(())
    } else {
        Err(format!(
            "Origin {value:?} does not match http://{}:{}",
            service.host, service.port
        ))
    }
}

/// Split `host:port` (or `[v6]:port`) into its parts.
fn parse_authority(value: &str) -> Option<(String, u16)> {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        let port = tail.strip_prefix(':')?;
        if host.is_empty() || port.is_empty() || port.contains(':') {
            return None;
        }
        return Some((format!("[{host}]"), port.parse().ok()?));
    }
    let (host, port) = value.rsplit_once(':')?;
    if host.is_empty() || host.contains(':') || port.is_empty() {
        return None;
    }
    Some((host.to_owned(), port.parse().ok()?))
}

/// `localhost` or any IP for which [`IpAddr::is_loopback`] holds.
fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let bare = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    bare.parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}
