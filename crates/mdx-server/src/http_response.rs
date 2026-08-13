use crate::{JSON_CONTENT_TYPE, RouteResponse, request_security};
use mdx_core::DeploymentMode;
use std::io::Write;
use std::net::TcpStream;

/// Refuse a connection past the cap with a minimal 503. Best effort: a client
/// that is gone before the refusal lands is not an error worth surfacing.
pub(crate) fn refuse_over_capacity(mut stream: TcpStream) {
    let body = "{\"error\":\"server_at_connection_capacity\"}";
    let wire = format!(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: {JSON_CONTENT_TYPE}\r\nRetry-After: 1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(wire.as_bytes());
}

pub(crate) fn reject_production_stream(
    stream: &mut TcpStream,
    mode: DeploymentMode,
    request: &str,
    path: &str,
    security: &request_security::RequestSecurity,
) -> Result<bool, String> {
    if !mode.requires_trusted_session() {
        return Ok(false);
    }
    if let Some(response) = security.classify_read("GET", path) {
        write_route_response(stream, mode, request, response)?;
        return Ok(true);
    }
    if security.has_verified_session() {
        return Ok(false);
    }
    let refusal = security
        .refusal_code()
        .unwrap_or("production_stream_requires_verified_session");
    let body = format!(
        "{{\"error\":\"production_stream_requires_verified_session\",\"refusal\":\"{}\",\"production_write_allowed\":false}}",
        refusal
    );
    let wire = format!(
        "HTTP/1.1 401 Unauthorized\r\nContent-Type: {}\r\n{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        JSON_CONTENT_TYPE,
        request_security::cors_headers(mode, request),
        body.len(),
        body
    );
    stream
        .write_all(wire.as_bytes())
        .map_err(|error| format!("write response: {error}"))?;
    Ok(true)
}

pub(crate) fn write_route_response(
    stream: &mut TcpStream,
    mode: DeploymentMode,
    request: &str,
    response: RouteResponse,
) -> Result<(), String> {
    let wire = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\n{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.content_type,
        request_security::cors_headers(mode, request),
        response.body.len(),
        response.body
    );
    stream
        .write_all(wire.as_bytes())
        .map_err(|error| format!("write response: {error}"))
}
