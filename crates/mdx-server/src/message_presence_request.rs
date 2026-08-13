use crate::RouteResponse;
use mdx_core::{GovernedWriteIdentity, MdxKernel, MessagePresenceRequest, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/messages/presence-requests.json" {
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
        return Some(
            render_local_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/presence-requests/projection.json" {
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
        return Some(
            render_local_projection_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let presence_request_id = json_string_field(body, "presence_request_id")
        .unwrap_or_else(|| "message_presence_request_local_001".to_string());
    let fanout_request_receipt_id =
        json_string_field(body, "fanout_request_receipt_id").unwrap_or_default();
    let thread_id =
        json_string_field(body, "thread_id").unwrap_or_else(|| "thread_local_receipts".to_string());
    let channel_id =
        json_string_field(body, "channel_id").unwrap_or_else(|| "local-ops".to_string());
    let presence_scope = json_string_field(body, "presence_scope")
        .unwrap_or_else(|| "local projection only".to_string());
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
        json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string())
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string())
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&actor_id)
    };
    let report = kernel
        .save_message_presence_request_local_with_identity(
            MessagePresenceRequest {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                presence_request_id: &presence_request_id,
                fanout_request_receipt_id: &fanout_request_receipt_id,
                thread_id: &thread_id,
                channel_id: &channel_id,
                presence_scope: &presence_scope,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-presence-request-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/messages/presence-requests.json","projection_route":"/messages/presence-requests/projection.json","presence_request_id":{},"presence_request_receipt_id":{},"policy_decision_id":{},"fanout_request_receipt_id":{},"thread_id":{},"channel_id":{},"realtime_presence_allowed":{},"websocket_fanout_allowed":{},"typing_indicator_allowed":{},"provider_call_allowed":{},"production_delivery_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.presence_request_id),
        json_string_literal(&report.presence_request_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.fanout_request_receipt_id),
        json_string_literal(&report.thread_id),
        json_string_literal(&report.channel_id),
        report.realtime_presence_allowed,
        report.websocket_fanout_allowed,
        report.typing_indicator_allowed,
        report.provider_call_allowed,
        report.production_delivery_allowed,
        report.production_write_allowed
    ))
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let requests = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "message.presence.requested")
        .map(|receipt| {
            format!(
                r#"{{"presence_request_id":{},"presence_request_receipt_id":{},"policy_decision_id":{},"fanout_request_receipt_id":{},"thread_id":{},"channel_id":{},"presence_scope":{},"terminal_state":{},"realtime_presence_allowed":false,"websocket_fanout_allowed":false,"typing_indicator_allowed":false,"provider_call_allowed":false,"production_delivery_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "presence_request_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "fanout_request_receipt_id")),
                json_string_literal(payload_value(receipt, "thread_id")),
                json_string_literal(payload_value(receipt, "channel_id")),
                json_string_literal(payload_value(receipt, "presence_scope")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-message-presence-request-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/presence-requests.json","presence_request_count":{},"realtime_presence_allowed":false,"websocket_fanout_allowed":false,"typing_indicator_allowed":false,"provider_call_allowed":false,"production_delivery_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"requests":[{}]}}"#,
        requests.len(),
        requests.join(",")
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
