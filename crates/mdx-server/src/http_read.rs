//! HTTP request reading for the serving path.
//!
//! The serving path used to read one 8 KB buffer per connection and treat it as
//! the whole request. That truncated any request whose headers plus body exceed
//! 8 KB and any body that arrives in a second TCP segment, which is routine off
//! localhost. This module reads headers to completion and then reads the body to
//! its declared `Content-Length`, with explicit caps and fail-closed refusals:
//! oversized headers, oversized bodies, missing length on a chunked request, a
//! stalled socket, and a connection that closes mid-request each map to a
//! specific refusal instead of a silently truncated parse.
//!
//! The assembled request keeps the exact wire shape (`headers\r\n\r\nbody`) so
//! everything downstream - security context, routing, and the streaming route -
//! keeps consuming the same request text it always has.

use std::io::Read;
use std::time::Duration;

/// Header section cap. Real MDx clients send a request line, a handful of
/// headers, and a bearer token; 16 KB is far above any legitimate request.
pub(crate) const MAX_HEADER_BYTES: usize = 16 * 1024;

/// Body cap. Page bodies are the largest legitimate payload today; 1 MiB is an
/// order of magnitude above what any current surface sends.
pub(crate) const MAX_BODY_BYTES: usize = 1024 * 1024;

/// A socket that produces no bytes for this long forfeits the request. This is
/// the slowloris boundary: a connection cannot park a thread forever.
pub(crate) fn read_timeout() -> Duration {
    Duration::from_secs(env_secs("MDX_HTTP_READ_TIMEOUT_SECS", 10))
}

/// A response write that stalls this long forfeits the connection. Applies per
/// write call, so a healthy long-lived stream is unaffected.
pub(crate) fn write_timeout() -> Duration {
    Duration::from_secs(env_secs("MDX_HTTP_WRITE_TIMEOUT_SECS", 30))
}

fn env_secs(name: &str, default_secs: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(default_secs)
}

/// The outcome of reading one request off the wire. A refusal carries the HTTP
/// status line and a stable error code so the caller can answer without
/// guessing; refusals never echo request content back.
pub(crate) enum RequestRead {
    Complete(String),
    Refused {
        status: &'static str,
        code: &'static str,
    },
}

impl RequestRead {
    fn refused(status: &'static str, code: &'static str) -> Self {
        Self::Refused { status, code }
    }
}

/// Read one HTTP request: headers to the blank line, then exactly
/// `Content-Length` body bytes. Returns a refusal instead of a truncated or
/// partial request in every failure mode.
pub(crate) fn read_http_request(stream: &mut impl Read) -> Result<RequestRead, String> {
    let mut collected: Vec<u8> = Vec::with_capacity(2048);
    let mut chunk = [0_u8; 4096];

    // Headers: read until the blank line, refusing before the cap is exceeded.
    let header_end = loop {
        if let Some(end) = find_header_end(&collected) {
            if end > MAX_HEADER_BYTES {
                return Ok(RequestRead::refused(
                    "431 Request Header Fields Too Large",
                    "request_headers_too_large",
                ));
            }
            break end;
        }
        if collected.len() > MAX_HEADER_BYTES {
            return Ok(RequestRead::refused(
                "431 Request Header Fields Too Large",
                "request_headers_too_large",
            ));
        }
        match stream.read(&mut chunk) {
            Ok(0) => {
                if collected.is_empty() {
                    // The client connected and closed without sending a request.
                    return Ok(RequestRead::refused("400 Bad Request", "empty_request"));
                }
                return Ok(RequestRead::refused(
                    "400 Bad Request",
                    "incomplete_request_headers",
                ));
            }
            Ok(read) => collected.extend_from_slice(&chunk[..read]),
            Err(error) => return Ok(read_error_outcome(&error)),
        }
    };

    let headers_text = String::from_utf8_lossy(&collected[..header_end]).into_owned();

    // Chunked transfer is refused, not half-supported: every MDx client sends
    // `Content-Length`, and a silently ignored `Transfer-Encoding` would parse
    // chunk framing as the JSON body.
    if header_value(&headers_text, "transfer-encoding").is_some() {
        return Ok(RequestRead::refused(
            "411 Length Required",
            "content_length_required",
        ));
    }

    let body_start = header_end + 4;
    let declared_length = match header_value(&headers_text, "content-length") {
        None => {
            // No declared body. Whatever followed the blank line in the same
            // segment rides along unchanged, matching the previous behavior
            // for clients that send no Content-Length at all.
            return Ok(RequestRead::Complete(
                String::from_utf8_lossy(&collected).into_owned(),
            ));
        }
        Some(raw) => match raw.trim().parse::<usize>() {
            Ok(length) => length,
            Err(_) => {
                return Ok(RequestRead::refused(
                    "400 Bad Request",
                    "invalid_content_length",
                ));
            }
        },
    };

    if declared_length > MAX_BODY_BYTES {
        return Ok(RequestRead::refused(
            "413 Content Too Large",
            "request_body_too_large",
        ));
    }

    // Body: read until the declared length is fully buffered.
    while collected.len() < body_start + declared_length {
        match stream.read(&mut chunk) {
            Ok(0) => {
                return Ok(RequestRead::refused(
                    "400 Bad Request",
                    "incomplete_request_body",
                ));
            }
            Ok(read) => collected.extend_from_slice(&chunk[..read]),
            Err(error) => return Ok(read_error_outcome(&error)),
        }
    }
    collected.truncate(body_start + declared_length);

    Ok(RequestRead::Complete(
        String::from_utf8_lossy(&collected).into_owned(),
    ))
}

fn read_error_outcome(error: &std::io::Error) -> RequestRead {
    match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            RequestRead::refused("408 Request Timeout", "request_read_timeout")
        }
        _ => RequestRead::refused("400 Bad Request", "request_read_failed"),
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Case-insensitive single-header lookup over the raw header text.
fn header_value<'a>(headers_text: &'a str, name_lower: &str) -> Option<&'a str> {
    headers_text.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case(name_lower) {
            Some(value.trim())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that yields its script one small segment at a time, the way a
    /// real socket delivers a request that did not fit one TCP segment.
    struct SegmentedReader {
        segments: Vec<Vec<u8>>,
        next: usize,
    }

    impl SegmentedReader {
        fn new(segments: Vec<&[u8]>) -> Self {
            Self {
                segments: segments.into_iter().map(<[u8]>::to_vec).collect(),
                next: 0,
            }
        }
    }

    impl Read for SegmentedReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.next >= self.segments.len() {
                return Ok(0);
            }
            let segment = &self.segments[self.next];
            self.next += 1;
            buf[..segment.len()].copy_from_slice(segment);
            Ok(segment.len())
        }
    }

    fn request_with_body(body: &str) -> String {
        format!(
            "POST /pages/edit-drafts.json HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn complete_text(outcome: RequestRead) -> String {
        match outcome {
            RequestRead::Complete(text) => text,
            RequestRead::Refused { code, .. } => panic!("expected complete request, got {code}"),
        }
    }

    fn refusal_code(outcome: RequestRead) -> &'static str {
        match outcome {
            RequestRead::Refused { code, .. } => code,
            RequestRead::Complete(_) => panic!("expected refusal, got complete request"),
        }
    }

    #[test]
    fn body_split_across_segments_reassembles() {
        let body = "{\"page_id\":\"pg_1\",\"body\":\"segmented\"}";
        let wire = request_with_body(body);
        let (first, rest) = wire.split_at(wire.len() - 10);
        let mut reader = SegmentedReader::new(vec![first.as_bytes(), rest.as_bytes()]);
        let text = complete_text(read_http_request(&mut reader).expect("read"));
        assert!(text.ends_with(body));
    }

    #[test]
    fn headers_split_across_segments_reassemble() {
        let wire = request_with_body("{\"k\":\"v\"}");
        let segments: Vec<&[u8]> = wire.as_bytes().chunks(7).collect();
        let mut reader = SegmentedReader::new(segments);
        let text = complete_text(read_http_request(&mut reader).expect("read"));
        assert_eq!(text, wire);
    }

    #[test]
    fn body_larger_than_old_eight_kb_buffer_reads_fully() {
        let body = format!("{{\"body\":\"{}\"}}", "x".repeat(20_000));
        let wire = request_with_body(&body);
        let segments: Vec<&[u8]> = wire.as_bytes().chunks(4096).collect();
        let mut reader = SegmentedReader::new(segments);
        let text = complete_text(read_http_request(&mut reader).expect("read"));
        assert!(text.ends_with(&body));
        assert!(text.len() > 8192);
    }

    #[test]
    fn body_reads_exactly_declared_length() {
        let mut wire = request_with_body("{\"k\":\"v\"}");
        wire.push_str("TRAILING-NOISE");
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let text = complete_text(read_http_request(&mut reader).expect("read"));
        assert!(text.ends_with("{\"k\":\"v\"}"));
        assert!(!text.contains("TRAILING-NOISE"));
    }

    #[test]
    fn get_without_content_length_completes() {
        let wire = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let text = complete_text(read_http_request(&mut reader).expect("read"));
        assert_eq!(text, wire);
    }

    #[test]
    fn oversized_headers_refuse_431() {
        let wire = format!(
            "GET / HTTP/1.1\r\nX-Padding: {}\r\n\r\n",
            "h".repeat(MAX_HEADER_BYTES + 1)
        );
        let segments: Vec<&[u8]> = wire.as_bytes().chunks(4096).collect();
        let mut reader = SegmentedReader::new(segments);
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "request_headers_too_large");
    }

    #[test]
    fn oversized_body_refuses_413_before_reading_it() {
        let wire = format!(
            "POST /x HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "request_body_too_large");
    }

    #[test]
    fn chunked_transfer_refuses_411() {
        let wire = "POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "content_length_required");
    }

    #[test]
    fn invalid_content_length_refuses_400() {
        let wire = "POST /x HTTP/1.1\r\nContent-Length: not-a-number\r\n\r\n";
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "invalid_content_length");
    }

    #[test]
    fn connection_closed_mid_body_refuses_400() {
        let wire = "POST /x HTTP/1.1\r\nContent-Length: 50\r\n\r\nonly-part";
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "incomplete_request_body");
    }

    #[test]
    fn connection_closed_mid_headers_refuses_400() {
        let wire = "POST /x HTTP/1.1\r\nHost: loc";
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "incomplete_request_headers");
    }

    #[test]
    fn empty_connection_refuses_400() {
        let mut reader = SegmentedReader::new(vec![]);
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "empty_request");
    }

    struct TimeoutReader;

    impl Read for TimeoutReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "stalled",
            ))
        }
    }

    #[test]
    fn stalled_socket_refuses_408() {
        let mut reader = TimeoutReader;
        let code = refusal_code(read_http_request(&mut reader).expect("read"));
        assert_eq!(code, "request_read_timeout");
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let wire = "POST /x HTTP/1.1\r\ncOnTeNt-LeNgTh: 2\r\n\r\nok";
        let mut reader = SegmentedReader::new(vec![wire.as_bytes()]);
        let text = complete_text(read_http_request(&mut reader).expect("read"));
        assert!(text.ends_with("ok"));
    }
}
