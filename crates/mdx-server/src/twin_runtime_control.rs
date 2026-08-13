use crate::{RouteResponse, reject_unless_method};
use mdx_core::{GovernedWriteIdentity, MdxKernel, TwinRuntimeControlRequest, json_string_literal};
use std::sync::{Arc, RwLock};

struct RuntimeControlRoute {
    post_route: &'static str,
    projection_route: &'static str,
    post_name: &'static str,
    projection_name: &'static str,
    item_kind: &'static str,
    default_item_id: &'static str,
    receipt_kind: &'static str,
    save: fn(
        &mut MdxKernel,
        TwinRuntimeControlRequest<'_>,
        &GovernedWriteIdentity,
    ) -> Result<mdx_core::TwinRuntimeControlReport, mdx_core::TwinRuntimeControlError>,
}

const ROUTES: &[RuntimeControlRoute] = &[
    RuntimeControlRoute {
        post_route: "/twin/agent-mode-preflights.json",
        projection_route: "/twin/agent-mode-preflights/projection.json",
        post_name: "mdx-twin-agent-mode-preflight-local-post",
        projection_name: "mdx-twin-agent-mode-preflight-local-projection",
        item_kind: "agent_mode",
        default_item_id: "companion_copilot",
        receipt_kind: "twin.agent_mode.preflighted",
        save: MdxKernel::preflight_twin_agent_mode_local_with_identity,
    },
    RuntimeControlRoute {
        post_route: "/twin/hook-preflights.json",
        projection_route: "/twin/hook-preflights/projection.json",
        post_name: "mdx-twin-hook-preflight-local-post",
        projection_name: "mdx-twin-hook-preflight-local-projection",
        item_kind: "hook",
        default_item_id: "session_start",
        receipt_kind: "twin.hook.preflighted",
        save: MdxKernel::preflight_twin_hook_local_with_identity,
    },
    RuntimeControlRoute {
        post_route: "/twin/skill-preflights.json",
        projection_route: "/twin/skill-preflights/projection.json",
        post_name: "mdx-twin-skill-preflight-local-post",
        projection_name: "mdx-twin-skill-preflight-local-projection",
        item_kind: "skill",
        default_item_id: "grounded_answer",
        receipt_kind: "twin.skill.preflighted",
        save: MdxKernel::preflight_twin_skill_local_with_identity,
    },
    RuntimeControlRoute {
        post_route: "/twin/connector-preflights.json",
        projection_route: "/twin/connector-preflights/projection.json",
        post_name: "mdx-twin-connector-preflight-local-post",
        projection_name: "mdx-twin-connector-preflight-local-projection",
        item_kind: "connector",
        default_item_id: "pages_world_model",
        receipt_kind: "twin.connector.preflighted",
        save: MdxKernel::preflight_twin_connector_local_with_identity,
    },
    RuntimeControlRoute {
        post_route: "/twin/agent-handoff-preflights.json",
        projection_route: "/twin/agent-handoff-preflights/projection.json",
        post_name: "mdx-twin-agent-handoff-preflight-local-post",
        projection_name: "mdx-twin-agent-handoff-preflight-local-projection",
        item_kind: "agent_handoff",
        default_item_id: "forge_reviewer",
        receipt_kind: "twin.agent_handoff.preflighted",
        save: MdxKernel::preflight_twin_agent_handoff_local_with_identity,
    },
];

pub(crate) fn route_request(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    for route in ROUTES {
        if path == route.post_route {
            return Some(post(method, body, kernel, route));
        }
        if path == route.projection_route {
            return Some(get(method, kernel, route));
        }
    }
    None
}

fn post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    route: &RuntimeControlRoute,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_post_json(body, &mut kernel, route)?,
    ))
}

fn get(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    route: &RuntimeControlRoute,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_projection_json(&kernel, route)?,
    ))
}

fn render_post_json(
    body: &str,
    kernel: &mut MdxKernel,
    route: &RuntimeControlRoute,
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
        json_string_field(body, "session_id").unwrap_or_else(|| "twin_runtime_control".to_string());
    let item_id =
        json_string_field(body, "item_id").unwrap_or_else(|| route.default_item_id.to_string());
    let source_draft_receipt_id =
        json_string_field(body, "source_draft_receipt_id").unwrap_or_default();
    let report = (route.save)(
        kernel,
        TwinRuntimeControlRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            session_id: &session_id,
            item_id: &item_id,
            source_draft_receipt_id: &source_draft_receipt_id,
        },
        &identity,
    )
    .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":{},"status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":{},"projection_route":{},"session_id":{},"item_kind":{},"item_id":{},"source_draft_receipt_id":{},"control_receipt_id":{},"terminal_state":{},"local_preview_allowed":{},"live_provider_call_allowed":false,"tool_execution_allowed":false,"connector_execution_allowed":false,"network_allowed":false,"secret_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false,"worker_spawn_allowed":false,"agent_swarm_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(route.post_name),
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(route.post_route),
        json_string_literal(route.projection_route),
        json_string_literal(&report.session_id),
        json_string_literal(report.item_kind),
        json_string_literal(&report.item_id),
        json_string_literal(&report.source_draft_receipt_id),
        json_string_literal(&report.control_receipt_id),
        json_string_literal(report.terminal_state),
        report.local_preview_allowed
    ))
}

fn render_projection_json(
    kernel: &MdxKernel,
    route: &RuntimeControlRoute,
) -> Result<String, String> {
    let controls = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == route.receipt_kind)
        .map(|receipt| {
            format!(
                r#"{{"session_id":{},"item_kind":{},"item_id":{},"source_draft_receipt_id":{},"control_receipt_id":{},"required_receipt_kind":{},"terminal_state":{},"local_preview_allowed":{},"live_provider_call_allowed":false,"tool_execution_allowed":false,"connector_execution_allowed":false,"network_allowed":false,"secret_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false,"worker_spawn_allowed":false,"agent_swarm_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "session_id")),
                json_string_literal(payload_value(receipt, "item_kind")),
                json_string_literal(payload_value(receipt, "item_id")),
                json_string_literal(payload_value(receipt, "source_draft_receipt_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(payload_value(receipt, "required_receipt_kind")),
                json_string_literal(payload_value(receipt, "terminal_state")),
                payload_value(receipt, "local_preview_allowed") == "true"
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":{},"status":"OK","auth_session_route":"/local/auth-session.json","writes_route":{},"projection_route":{},"item_kind":{},"control_count":{},"live_provider_call_allowed":false,"tool_execution_allowed":false,"connector_execution_allowed":false,"network_allowed":false,"secret_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false,"worker_spawn_allowed":false,"agent_swarm_allowed":false,"production_write_allowed":false,"controls":[{}]}}"#,
        json_string_literal(route.projection_name),
        json_string_literal(route.post_route),
        json_string_literal(route.projection_route),
        json_string_literal(route.item_kind),
        controls.len(),
        controls.join(",")
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
    fn twin_runtime_control_routes_record_all_lanes_and_projection_blocks_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let bodies = [
            (
                "/twin/agent-mode-preflights.json",
                r#"{"session_id":"runtime_control","item_id":"research_scout"}"#,
            ),
            (
                "/twin/hook-preflights.json",
                r#"{"session_id":"runtime_control","item_id":"pre_provider_call"}"#,
            ),
            (
                "/twin/skill-preflights.json",
                r#"{"session_id":"runtime_control","item_id":"provider_reasoning"}"#,
            ),
            (
                "/twin/connector-preflights.json",
                r#"{"session_id":"runtime_control","item_id":"provider_gateway"}"#,
            ),
            (
                "/twin/agent-handoff-preflights.json",
                r#"{"session_id":"runtime_control","item_id":"forge_reviewer"}"#,
            ),
        ];
        for (path, body) in bodies {
            let response = route_request("POST", path, body, &kernel)
                .expect("route")
                .expect("response");
            assert_eq!(response.status, "200 OK");
            assert!(response.body.contains("\"production_write_allowed\":false"));
            assert!(response.body.contains("\"agent_swarm_allowed\":false"));
        }
        let projection = route_request(
            "GET",
            "/twin/agent-handoff-preflights/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert!(projection.body.contains("\"control_count\":1"));
        assert!(projection.body.contains("\"item_id\":\"forge_reviewer\""));
        assert!(projection.body.contains("\"agent_swarm_allowed\":false"));
    }
}
