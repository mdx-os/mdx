use crate::{RouteResponse, reject_unless_method};
use mdx_core::{GovernedWriteIdentity, MdxKernel, TwinBoundaryRefusalRequest, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_request(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/twin/provider-call-refusals.json" => Some(post(
            method,
            body,
            kernel,
            render_provider_call_refusal_post_json,
        )),
        "/twin/production-memory-refusals.json" => Some(post(
            method,
            body,
            kernel,
            render_production_memory_refusal_post_json,
        )),
        "/twin/tool-execution-refusals.json" => Some(post(
            method,
            body,
            kernel,
            render_tool_execution_refusal_post_json,
        )),
        "/twin/provider-call-refusals/projection.json" => Some(get(
            method,
            kernel,
            render_provider_call_refusal_projection_json,
        )),
        "/twin/production-memory-refusals/projection.json" => Some(get(
            method,
            kernel,
            render_production_memory_refusal_projection_json,
        )),
        "/twin/tool-execution-refusals/projection.json" => Some(get(
            method,
            kernel,
            render_tool_execution_refusal_projection_json,
        )),
        _ => None,
    }
}

fn post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    render: impl FnOnce(&str, &mut MdxKernel) -> Result<String, String>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json("200 OK", render(body, &mut kernel)?))
}

fn get(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    render: impl FnOnce(&MdxKernel) -> Result<String, String>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json("200 OK", render(&kernel)?))
}

fn render_provider_call_refusal_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    render_boundary_refusal_post_json(
        body,
        kernel,
        "mdx-twin-provider-call-refusal-local-post",
        "twin_session_provider_refusal_001",
        |kernel, request, identity| {
            kernel.save_twin_provider_call_refusal_local_with_identity(request, identity)
        },
    )
}

fn render_production_memory_refusal_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    render_boundary_refusal_post_json(
        body,
        kernel,
        "mdx-twin-production-memory-refusal-local-post",
        "twin_session_memory_refusal_001",
        |kernel, request, identity| {
            kernel.save_twin_production_memory_refusal_local_with_identity(request, identity)
        },
    )
}

fn render_tool_execution_refusal_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    render_boundary_refusal_post_json(
        body,
        kernel,
        "mdx-twin-tool-execution-refusal-local-post",
        "twin_session_tool_refusal_001",
        |kernel, request, identity| {
            kernel.save_twin_tool_execution_refusal_local_with_identity(request, identity)
        },
    )
}

fn render_provider_call_refusal_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    render_boundary_refusal_projection_json(
        kernel,
        "mdx-twin-provider-call-refusal-local-projection",
        "/twin/provider-call-refusals.json",
        "twin.session.provider_call.refused",
    )
}

fn render_production_memory_refusal_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    render_boundary_refusal_projection_json(
        kernel,
        "mdx-twin-production-memory-refusal-local-projection",
        "/twin/production-memory-refusals.json",
        "twin.session.production_memory.refused",
    )
}

fn render_tool_execution_refusal_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    render_boundary_refusal_projection_json(
        kernel,
        "mdx-twin-tool-execution-refusal-local-projection",
        "/twin/tool-execution-refusals.json",
        "twin.session.tool_execution.refused",
    )
}

fn render_boundary_refusal_post_json(
    body: &str,
    kernel: &mut MdxKernel,
    name: &str,
    default_session_id: &str,
    save: impl FnOnce(
        &mut MdxKernel,
        TwinBoundaryRefusalRequest<'_>,
        &GovernedWriteIdentity,
    )
        -> Result<mdx_core::TwinBoundaryRefusalReport, mdx_core::TwinBoundaryRefusalError>,
) -> Result<String, String> {
    // The verified trusted session is the identity; otherwise the local-demo
    // fixture (kernel-save tenant `tenant_local`, actor `local_user`).
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";
    let tenant_id = if verified {
        resolved.tenant_id.clone()
    } else {
        json_string_field(body, "tenant_id").unwrap_or_else(|| "tenant_local".to_string())
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "actor_id").unwrap_or_else(|| "local_user".to_string())
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&actor_id)
    };
    let session_id =
        json_string_field(body, "session_id").unwrap_or_else(|| default_session_id.to_string());
    let source_draft_receipt_id =
        json_string_field(body, "source_draft_receipt_id").unwrap_or_default();
    let report = save(
        kernel,
        TwinBoundaryRefusalRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            session_id: &session_id,
            source_draft_receipt_id: &source_draft_receipt_id,
        },
        &identity,
    )
    .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":{},"status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","session_id":{},"source_draft_receipt_id":{},"refusal_receipt_id":{},"refused_capability":{},"terminal_state":{},"live_provider_call_allowed":false,"production_memory_write_allowed":false,"tool_execution_allowed":false,"hidden_service_role_allowed":false,"provider_call_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(name),
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.session_id),
        json_string_literal(&report.source_draft_receipt_id),
        json_string_literal(&report.refusal_receipt_id),
        json_string_literal(report.refused_capability),
        json_string_literal(report.terminal_state)
    ))
}

fn render_boundary_refusal_projection_json(
    kernel: &MdxKernel,
    name: &str,
    writes_route: &str,
    receipt_kind: &str,
) -> Result<String, String> {
    let refusals = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == receipt_kind)
        .map(|receipt| {
            format!(
                r#"{{"session_id":{},"source_draft_receipt_id":{},"refusal_receipt_id":{},"refused_capability":{},"terminal_state":{},"live_provider_call_allowed":false,"production_memory_write_allowed":false,"tool_execution_allowed":false,"hidden_service_role_allowed":false,"provider_call_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "session_id")),
                json_string_literal(payload_value(receipt, "source_draft_receipt_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(payload_value(receipt, "refused_capability")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":{},"status":"OK","auth_session_route":"/local/auth-session.json","writes_route":{},"refusal_count":{},"live_provider_call_allowed":false,"production_memory_write_allowed":false,"tool_execution_allowed":false,"hidden_service_role_allowed":false,"provider_call_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false,"refusals":[{}]}}"#,
        json_string_literal(name),
        json_string_literal(writes_route),
        refusals.len(),
        refusals.join(",")
    ))
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn twin_boundary_routes_record_refusals_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let provider = route_request("POST", "/twin/provider-call-refusals.json", r#"{"session_id":"route_twin_boundary"}"#, &kernel).expect("provider route").expect("provider response");
        assert_eq!(provider.status, "200 OK");
        assert!(provider.body.contains("\"refused_capability\":\"live_provider_call\""));
        let source = provider.body.split("\"source_draft_receipt_id\":\"").nth(1).and_then(|value| value.split('"').next()).expect("source receipt").to_string();
        for (path, capability) in [("/twin/production-memory-refusals.json", "production_memory_write"), ("/twin/tool-execution-refusals.json", "tool_execution")] { let body = format!(r#"{{"session_id":"route_twin_boundary","source_draft_receipt_id":"{}"}}"#, source); let response = route_request("POST", path, &body, &kernel).expect("route").expect("response"); assert!(response.body.contains(&format!("\"refused_capability\":\"{}\"", capability))); }
        let projection = route_request("GET", "/twin/provider-call-refusals/projection.json", "", &kernel).expect("projection route").expect("projection response");
        assert!(projection.body.contains("\"refusal_count\":1"));
        assert!(projection.body.contains("\"production_write_allowed\":false"));
    }
}
