use crate::RouteResponse;
use crate::studio::json_string_field;
use mdx_core::{ClaimInstallOwner, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

// HTTP for claiming the install owner. POST records who owns this install
// (first claim wins, no password); GET reports the current ownership so the
// first-run screen can show it. The claim is a governed write; the GET is a
// read-only projection.
pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/install/owner-claims.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(claim(body, &mut kernel).map(|body| RouteResponse::json("200 OK", body)));
    }
    if path == "/install/owner.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json("200 OK", projection(&kernel))));
    }
    None
}

fn identity(body: &str) -> crate::request_security::ResolvedWriteIdentity {
    crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    )
}

fn claim(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let resolved = identity(body);
    let owner_name = json_string_field(body, "owner_name").unwrap_or_default();
    // A bad claim (e.g. a blank name) is refused gracefully as a 200 with a
    // reason, never a hard error - the surface stays truthful and legible.
    let report = match kernel.claim_install_owner_with_identity(
        ClaimInstallOwner {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            owner_name: &owner_name,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-install-owner-claim-local-post","status":"INSTALL_OWNER_REFUSED","claimed":false,"already_claimed":false,"reason":{},"production_write_allowed":false}}"#,
                json_string_literal(&error.message())
            ));
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-install-owner-claim-local-post","status":{},"claimed":{},"already_claimed":{},"owner_name":{},"owner_actor_id":{},"receipt_id":{},"projection_route":"/install/owner.json","production_write_allowed":false}}"#,
        json_string_literal(report.status),
        report.claimed,
        report.already_claimed,
        json_string_literal(&report.owner_name),
        json_string_literal(&report.owner_actor_id),
        json_string_literal(&report.receipt_id),
    ))
}

fn projection(kernel: &MdxKernel) -> String {
    let state = kernel.install_owner_state();
    format!(
        r#"{{"name":"mdx-install-owner","status":"OK","writes_route":"/install/owner-claims.json","claimed":{},"owner_name":{},"owner_actor_id":{},"receipt_id":{},"production_write_allowed":false}}"#,
        state.claimed,
        json_string_literal(&state.owner_name),
        json_string_literal(&state.owner_actor_id),
        json_string_literal(&state.receipt_id),
    )
}
