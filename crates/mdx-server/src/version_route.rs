use crate::RouteResponse;
use mdx_core::{json_string_literal, local_http_routes};

pub(crate) fn route_response(method: &str, path: &str) -> Option<Result<RouteResponse, String>> {
    if path != "/version.json" {
        return None;
    }
    Some(handle_version(method))
}

fn handle_version(method: &str) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    Ok(RouteResponse::json("200 OK", render_version_json()))
}

fn render_version_json() -> String {
    let routes = local_http_routes();
    let fingerprint = contract_fingerprint();
    format!(
        r#"{{"name":"mdx-kernel-version","status":"OK","route":"/version.json","kernel_version":{},"contract_fingerprint":{},"route_count":{},"min_app_version":"0.9.0","app_contract":"native_macos_beta","production_write_allowed":false}}"#,
        json_string_literal(env!("CARGO_PKG_VERSION")),
        json_string_literal(&fingerprint),
        routes.len(),
    )
}

fn contract_fingerprint() -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for route in local_http_routes() {
        for part in [
            route.method,
            route.local_path,
            route.operation_id,
            route.surface,
            route.response_content_type,
            route.response_schema_path.unwrap_or(""),
            if route.receipt_backed {
                "receipt"
            } else {
                "plain"
            },
        ] {
            fnv1a(&mut hash, part.as_bytes());
            fnv1a(&mut hash, &[0xff]);
        }
    }
    format!("routes:{}:{hash:016x}", local_http_routes().len())
}

fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_route_returns_contract_fingerprint() {
        let response = route_response("GET", "/version.json")
            .expect("version route")
            .expect("version response");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(
            response
                .body_text()
                .contains("\"kernel_version\":\"0.1.0\"")
        );
        assert!(
            response
                .body_text()
                .contains("\"contract_fingerprint\":\"routes:")
        );
        assert!(
            response
                .body_text()
                .contains("\"min_app_version\":\"0.9.0\"")
        );
    }
}
