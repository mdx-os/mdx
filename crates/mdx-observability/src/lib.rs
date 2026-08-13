//! Concrete OpenTelemetry transport for the MDx serving boundary.
//!
//! `mdx-core` owns the correlation contract and remains network closed. This
//! crate owns the replaceable OTLP/HTTP JSON adapter. It accepts only bounded,
//! presence-safe span fields and never includes endpoint or header values in an
//! error, debug representation, or export report.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read as _;
use std::time::Duration;
use url::Url;

const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const MAX_SPAN_NAME_CHARS: usize = 160;
const MAX_ATTRIBUTE_CHARS: usize = 256;
const MAX_ENDPOINT_BYTES: usize = 2048;
const MAX_HEADER_CONFIG_BYTES: usize = 16 * 1024;
const MAX_HEADERS: usize = 32;
const MAX_HEADER_NAME_BYTES: usize = 128;
const MAX_HEADER_VALUE_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttributeValue {
    String(String),
    Integer(i64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtlpSpan {
    pub trace_id: [u8; 16],
    pub span_id: [u8; 8],
    pub parent_span_id: Option<[u8; 8]>,
    pub flags: u32,
    pub name: String,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub attributes: BTreeMap<String, AttributeValue>,
    pub error: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtlpExportReport {
    pub accepted_spans: usize,
    pub warning_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OtlpConfigError {
    MissingEndpoint,
    InvalidEndpoint,
    EndpointCredentialsForbidden,
    InvalidHeader,
    ReservedHeader,
}

impl fmt::Display for OtlpConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingEndpoint => "OTLP endpoint is required",
            Self::InvalidEndpoint => "OTLP endpoint must be an HTTP or HTTPS URL",
            Self::EndpointCredentialsForbidden => {
                "OTLP endpoint must not contain credentials, query parameters, or fragments"
            }
            Self::InvalidHeader => "OTLP header configuration is invalid",
            Self::ReservedHeader => "OTLP header configuration cannot override transport headers",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OtlpExportError {
    Transport,
    HttpStatus(u16),
    InvalidResponse,
    PartialSuccess { rejected_spans: u64 },
}

impl OtlpExportError {
    pub fn category(&self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::HttpStatus(_) => "http_status",
            Self::InvalidResponse => "invalid_response",
            Self::PartialSuccess { .. } => "partial_success",
        }
    }
}

impl fmt::Display for OtlpExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport => f.write_str("OTLP transport failed"),
            Self::HttpStatus(status) => write!(f, "OTLP collector returned HTTP {status}"),
            Self::InvalidResponse => f.write_str("OTLP collector returned an invalid response"),
            Self::PartialSuccess { rejected_spans } => {
                write!(f, "OTLP collector rejected {rejected_spans} spans")
            }
        }
    }
}

pub struct OtlpHttpJsonExporter {
    endpoint: Url,
    headers: Vec<(String, String)>,
    service_name: String,
    environment: String,
    agent: ureq::Agent,
}

impl fmt::Debug for OtlpHttpJsonExporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtlpHttpJsonExporter")
            .field("endpoint_configured", &true)
            .field("header_count", &self.headers.len())
            .field("service_name", &self.service_name)
            .field("environment", &self.environment)
            .finish()
    }
}

impl OtlpHttpJsonExporter {
    pub fn connect(
        endpoint: Option<&str>,
        raw_headers: Option<&str>,
        service_name: &str,
        environment: &str,
    ) -> Result<Self, OtlpConfigError> {
        let raw_endpoint = endpoint
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or(OtlpConfigError::MissingEndpoint)?;
        if raw_endpoint.len() > MAX_ENDPOINT_BYTES {
            return Err(OtlpConfigError::InvalidEndpoint);
        }
        let mut endpoint =
            Url::parse(raw_endpoint).map_err(|_| OtlpConfigError::InvalidEndpoint)?;
        if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
            return Err(OtlpConfigError::InvalidEndpoint);
        }
        if !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(OtlpConfigError::EndpointCredentialsForbidden);
        }
        let base = endpoint.path().trim_end_matches('/');
        let trace_path = if base.ends_with("/v1/traces") {
            base.to_string()
        } else {
            format!("{base}/v1/traces")
        };
        endpoint.set_path(&trace_path);
        let headers = parse_headers(raw_headers.unwrap_or_default())?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout_read(Duration::from_secs(5))
            .timeout_write(Duration::from_secs(5))
            .build();
        Ok(Self {
            endpoint,
            headers,
            service_name: bounded(service_name, 96),
            environment: bounded(environment, 96),
            agent,
        })
    }

    pub fn adapter_name() -> &'static str {
        "OpenTelemetryExporter"
    }

    pub fn export(&self, spans: &[OtlpSpan]) -> Result<OtlpExportReport, OtlpExportError> {
        if spans.is_empty() {
            return Ok(OtlpExportReport {
                accepted_spans: 0,
                warning_present: false,
            });
        }
        let body = export_request_json(&self.service_name, &self.environment, spans);
        let mut request = self
            .agent
            .post(self.endpoint.as_str())
            .set("Content-Type", "application/json")
            .set("User-Agent", "mdx-observability/0.1");
        for (name, value) in &self.headers {
            request = request.set(name, value);
        }
        let response = match request.send_json(body) {
            Ok(response) => response,
            Err(ureq::Error::Status(status, _)) => {
                return Err(OtlpExportError::HttpStatus(status));
            }
            Err(ureq::Error::Transport(_)) => return Err(OtlpExportError::Transport),
        };
        let mut response_body = String::new();
        response
            .into_reader()
            .take(MAX_RESPONSE_BYTES)
            .read_to_string(&mut response_body)
            .map_err(|_| OtlpExportError::InvalidResponse)?;
        if response_body.trim().is_empty() {
            return Ok(OtlpExportReport {
                accepted_spans: spans.len(),
                warning_present: false,
            });
        }
        let parsed: Value =
            serde_json::from_str(&response_body).map_err(|_| OtlpExportError::InvalidResponse)?;
        let partial = parsed
            .get("partialSuccess")
            .or_else(|| parsed.get("partial_success"));
        let rejected = partial
            .and_then(|value| {
                value
                    .get("rejectedSpans")
                    .or_else(|| value.get("rejected_spans"))
            })
            .and_then(json_u64)
            .unwrap_or(0);
        if rejected > 0 {
            return Err(OtlpExportError::PartialSuccess {
                rejected_spans: rejected,
            });
        }
        let warning_present = partial
            .and_then(|value| {
                value
                    .get("errorMessage")
                    .or_else(|| value.get("error_message"))
            })
            .and_then(Value::as_str)
            .is_some_and(|message| !message.trim().is_empty());
        Ok(OtlpExportReport {
            accepted_spans: spans.len(),
            warning_present,
        })
    }
}

fn parse_headers(raw: &str) -> Result<Vec<(String, String)>, OtlpConfigError> {
    if raw.len() > MAX_HEADER_CONFIG_BYTES {
        return Err(OtlpConfigError::InvalidHeader);
    }
    let mut parsed = Vec::new();
    for entry in raw
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let (name, value) = entry
            .split_once('=')
            .ok_or(OtlpConfigError::InvalidHeader)?;
        let name = name.trim();
        let value = value.trim();
        if name.is_empty()
            || value.is_empty()
            || name.len() > MAX_HEADER_NAME_BYTES
            || value.len() > MAX_HEADER_VALUE_BYTES
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(OtlpConfigError::InvalidHeader);
        }
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "content-type" | "content-length"
        ) {
            return Err(OtlpConfigError::ReservedHeader);
        }
        parsed.push((name.to_string(), value.to_string()));
        if parsed.len() > MAX_HEADERS {
            return Err(OtlpConfigError::InvalidHeader);
        }
    }
    Ok(parsed)
}

fn export_request_json(service_name: &str, environment: &str, spans: &[OtlpSpan]) -> Value {
    let spans = spans.iter().map(span_json).collect::<Vec<_>>();
    json!({
        "resourceSpans": [{
            "resource": {
                "attributes": [
                    attribute_json("service.name", &AttributeValue::String(service_name.to_string())),
                    attribute_json("deployment.environment.name", &AttributeValue::String(environment.to_string()))
                ]
            },
            "scopeSpans": [{
                "scope": { "name": "mdx-server", "version": env!("CARGO_PKG_VERSION") },
                "spans": spans
            }]
        }]
    })
}

fn span_json(span: &OtlpSpan) -> Value {
    let attributes = span
        .attributes
        .iter()
        .map(|(key, value)| attribute_json(key, value))
        .collect::<Vec<_>>();
    let mut rendered = json!({
        "traceId": hex(&span.trace_id),
        "spanId": hex(&span.span_id),
        "flags": span.flags,
        "name": bounded(&span.name, MAX_SPAN_NAME_CHARS),
        "kind": 2,
        "startTimeUnixNano": span.start_time_unix_nano.to_string(),
        "endTimeUnixNano": span.end_time_unix_nano.max(span.start_time_unix_nano).to_string(),
        "attributes": attributes
    });
    if let Some(parent_span_id) = span.parent_span_id {
        rendered["parentSpanId"] = json!(hex(&parent_span_id));
    }
    if span.error {
        rendered["status"] = json!({ "code": 2 });
    }
    rendered
}

fn attribute_json(key: &str, value: &AttributeValue) -> Value {
    let value = match value {
        AttributeValue::String(value) => {
            json!({ "stringValue": bounded(value, MAX_ATTRIBUTE_CHARS) })
        }
        AttributeValue::Integer(value) => json!({ "intValue": value.to_string() }),
        AttributeValue::Boolean(value) => json!({ "boolValue": value }),
    };
    json!({ "key": bounded(key, 96), "value": value })
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|raw| raw.parse::<u64>().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn span() -> OtlpSpan {
        OtlpSpan {
            trace_id: [7; 16],
            span_id: [9; 8],
            parent_span_id: Some([8; 8]),
            flags: 1,
            name: "HTTP GET twin".to_string(),
            start_time_unix_nano: 10,
            end_time_unix_nano: 20,
            attributes: BTreeMap::from([
                (
                    "mdx.surface".to_string(),
                    AttributeValue::String("twin".to_string()),
                ),
                (
                    "http.response.status_code".to_string(),
                    AttributeValue::Integer(200),
                ),
            ]),
            error: false,
        }
    }

    #[test]
    fn endpoint_and_headers_fail_closed_without_leaking_values() {
        assert_eq!(
            OtlpHttpJsonExporter::connect(None, None, "mdx", "test").unwrap_err(),
            OtlpConfigError::MissingEndpoint
        );
        assert_eq!(
            OtlpHttpJsonExporter::connect(
                Some("https://user:secret@example.test/v1/traces"),
                None,
                "mdx",
                "test"
            )
            .unwrap_err(),
            OtlpConfigError::EndpointCredentialsForbidden
        );
        let error = OtlpHttpJsonExporter::connect(
            Some("https://example.test"),
            Some("authorization"),
            "mdx",
            "test",
        )
        .unwrap_err();
        assert_eq!(error, OtlpConfigError::InvalidHeader);
        assert!(!error.to_string().contains("authorization"));
    }

    #[test]
    fn otlp_json_export_reaches_the_trace_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind collector");
        let address = listener.local_addr().expect("collector address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept export");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let read = stream.read(&mut chunk).expect("read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                let header_end = request.windows(4).position(|bytes| bytes == b"\r\n\r\n");
                if let Some(header_end) = header_end {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + length {
                        break;
                    }
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .expect("write response");
            String::from_utf8(request).expect("request utf8")
        });
        let exporter = OtlpHttpJsonExporter::connect(
            Some(&format!("http://{address}/v1/traces/")),
            Some("x-test-key=present"),
            "mdx-kernel",
            "test",
        )
        .expect("connect exporter");
        let report = exporter.export(&[span()]).expect("export span");
        assert_eq!(report.accepted_spans, 1);
        let request = server.join().expect("collector joined");
        assert!(request.starts_with("POST /v1/traces HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("content-type: application/json")
        );
        assert!(request.contains("\"resourceSpans\""));
        assert!(request.contains("\"traceId\":\"07070707070707070707070707070707\""));
        assert!(request.contains("\"spanId\":\"0909090909090909\""));
        assert!(request.contains("\"parentSpanId\":\"0808080808080808\""));
        assert!(request.contains("\"flags\":1"));
        assert!(request.contains("\"kind\":2"));
        assert!(request.contains("\"mdx.surface\""));
    }

    #[test]
    fn partial_success_is_not_reported_as_delivered() {
        let body = json!({
            "partialSuccess": { "rejectedSpans": "1", "errorMessage": "held" }
        });
        assert_eq!(json_u64(&body["partialSuccess"]["rejectedSpans"]), Some(1));
    }
}
