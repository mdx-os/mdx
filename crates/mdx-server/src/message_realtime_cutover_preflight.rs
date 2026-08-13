use crate::RouteResponse;
use mdx_core::{
    GovernedWriteIdentity, MdxKernel, MessageDeliveryReplayBatch, MessageRealtimeCutoverPreflight,
    MessageServiceRoleFanoutRefusal, MessageSubscriptionIsolationCheck, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/messages/realtime-cutover-preflights.json" {
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
    if path == "/messages/relay-bridge.json" {
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
            render_relay_bridge_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/realtime-cutover-preflights/projection.json" {
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
    if path == "/messages/delivery-replay-batches.json" {
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
            render_delivery_replay_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/delivery-replay-batches/projection.json" {
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
            render_delivery_replay_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/subscription-isolation-checks.json" {
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
            render_subscription_isolation_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/subscription-isolation-checks/projection.json" {
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
            render_subscription_isolation_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/service-role-fanout-refusals.json" {
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
            render_service_role_refusal_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/service-role-fanout-refusals/projection.json" {
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
            render_service_role_refusal_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

pub(crate) fn render_relay_bridge_json(kernel: &MdxKernel) -> Result<String, String> {
    let message_watermark = sequence_high_watermark(kernel, "message.human.posted");
    let source_receipts = kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .filter(|receipt| {
            [
                "message.human.posted",
                "message.realtime.cutover.preflighted",
                "message.delivery.replay.recorded",
                "message.subscription.isolation.checked",
                "message.service_role.fanout.refused",
            ]
            .contains(&receipt.kind.as_str())
        })
        .take(6)
        .map(|receipt| json_string_literal(&receipt.receipt_id))
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-message-relay-bridge-metadata","status":"LIVE-LOCAL-BRIDGE-METADATA-READY","route":"/messages/relay-bridge.json","read_only":true,"writes_allowed":false,"execution_allowed":false,"relay_runtime":"mdx-message-relay","relay_transport":"websocket_valkey_pubsub","relay_contract_route":"/messages/relay-bridge.json","relay_health_route":"/health","client_ws_env_key":"VITE_MDX_MESSAGE_RELAY_WS","server_relay_url_env_key":"MDX_MESSAGE_RELAY_URL","ws_url_source":"client_env","bridge_metadata_source":"kernel_route","descriptor_count":3,"message_sequence_high_watermark":{},"cursor_policy":{{"cursor_kind":"message_sequence","metadata_cursor_use":"first_subscribe_seed_only","client_owned_cursor_on_reconnect":true,"server_watermark_on_reconnect_allowed":false,"dedupe_required":true,"reason":"kernel saves before relay publish; reconnecting from a fresh server watermark can skip outage messages"}},"authority":{{"local_websocket_relay_metadata_allowed":true,"typing_signal_metadata_allowed":true,"production_delivery_allowed":false,"service_role_fanout_allowed":false,"presence_mutation_allowed":false,"provider_call_allowed":false,"production_write_allowed":false}},"descriptors":[{}],"ui_handoff":{{"target_owner":"claude","target_surface":"apps/mdx Message module","safe_to_replace_local_descriptor":true,"must_not_construct_tenant_or_topic_in_ui":true,"must_not_fetch_fresh_watermark_on_reconnect":true,"must_not_claim_production_realtime_cutover":true}},"evidence":{{"source_contract":"docs/MESSAGE-RELAY-BRIDGE-CONTRACT.md","source_routes":["/messages/thread-messages/projection.json","/messages/realtime-cutover-preflights/projection.json","/messages/delivery-replay-batches/projection.json","/messages/subscription-isolation-checks/projection.json","/messages/service-role-fanout-refusals/projection.json"],"source_receipt_ids":[{}],"check":"make message-relay-bridge-check"}}}}"#,
        message_watermark,
        relay_descriptors_json(message_watermark),
        source_receipts.join(",")
    ))
}

fn relay_descriptors_json(message_watermark: usize) -> String {
    [
        relay_descriptor_json(
            "message_local_ops",
            "message",
            "message_sequence",
            message_watermark,
            r#"{"type":"subscribe","tenant_id":"local_tenant","channel_ids":["local-ops"]}"#,
            "mdx:message:local_tenant:local-ops",
            "/messages/thread-messages/projection.json",
            true,
        ),
        relay_descriptor_json(
            "ctx_event_stream",
            "ctx",
            "ctx_event_sequence",
            0,
            r#"{"type":"subscribe","tenant_id":"local_tenant","channel_ids":["events"],"stream_id":"ctx_event_stream"}"#,
            "mdx:ctx:local_tenant:events",
            "/ctx/events.json",
            false,
        ),
        relay_descriptor_json(
            "dxr_event_stream",
            "dxr",
            "dxr_event_sequence",
            0,
            r#"{"type":"subscribe","tenant_id":"local_tenant","channel_ids":["events"],"stream_id":"dxr_event_stream"}"#,
            "mdx:dxr:local_tenant:events",
            "/dxr/events.json",
            false,
        ),
    ]
    .join(",")
}

#[allow(clippy::too_many_arguments)]
fn relay_descriptor_json(
    stream_id: &str,
    domain: &str,
    cursor_kind: &str,
    initial_after_cursor: usize,
    subscribe_payload: &str,
    topic: &str,
    projection_route: &str,
    typing_signal_allowed: bool,
) -> String {
    format!(
        r#"{{"stream_id":{},"domain":{},"relay_driver":"mdx-message-relay","topic":{},"cursor_kind":{},"initial_after_cursor":{},"subscribe_payload":{},"sequence_field":"sequence","envelope_shape":"relay_envelope_v1","reconnect_policy":"client_owned_cursor","metadata_cursor_use":"first_subscribe_seed_only","server_watermark_on_reconnect_allowed":false,"dedupe_key":"message_receipt_id_or_event_id","projection_route":{},"typing_signal_allowed":{},"local_websocket_fanout_allowed":true,"production_delivery_allowed":false}}"#,
        json_string_literal(stream_id),
        json_string_literal(domain),
        json_string_literal(topic),
        json_string_literal(cursor_kind),
        initial_after_cursor,
        subscribe_payload,
        json_string_literal(projection_route),
        typing_signal_allowed
    )
}

fn sequence_high_watermark(kernel: &MdxKernel, receipt_kind: &str) -> usize {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == receipt_kind)
        .filter_map(|receipt| payload_value(receipt, "sequence").parse::<usize>().ok())
        .max()
        .unwrap_or(0)
}

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let preflight_id = json_string_field(body, "preflight_id")
        .unwrap_or_else(|| "message_realtime_preflight_local_001".to_string());
    let presence_request_receipt_id =
        json_string_field(body, "presence_request_receipt_id").unwrap_or_default();
    let thread_id =
        json_string_field(body, "thread_id").unwrap_or_else(|| "thread_local_receipts".to_string());
    let channel_id =
        json_string_field(body, "channel_id").unwrap_or_else(|| "local-ops".to_string());
    let requested_realtime_scope = json_string_field(body, "requested_realtime_scope")
        .unwrap_or_else(|| "tenant channel replay".to_string());
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
        .save_message_realtime_cutover_preflight_local_with_identity(
            MessageRealtimeCutoverPreflight {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                preflight_id: &preflight_id,
                presence_request_receipt_id: &presence_request_receipt_id,
                thread_id: &thread_id,
                channel_id: &channel_id,
                requested_realtime_scope: &requested_realtime_scope,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-realtime-cutover-preflight-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/messages/realtime-cutover-preflights.json","projection_route":"/messages/realtime-cutover-preflights/projection.json","preflight_id":{},"preflight_receipt_id":{},"policy_decision_id":{},"presence_request_receipt_id":{},"thread_id":{},"channel_id":{},"tenant_subscription_isolation_proven":{},"service_role_fanout_refused":{},"realtime_provider_turn_on_observed":{},"presence_mutation_allowed":{},"typing_indicator_allowed":{},"websocket_fanout_allowed":{},"production_delivery_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.preflight_id),
        json_string_literal(&report.preflight_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.presence_request_receipt_id),
        json_string_literal(&report.thread_id),
        json_string_literal(&report.channel_id),
        report.tenant_subscription_isolation_proven,
        report.service_role_fanout_refused,
        report.realtime_provider_turn_on_observed,
        report.presence_mutation_allowed,
        report.typing_indicator_allowed,
        report.websocket_fanout_allowed,
        report.production_delivery_allowed,
        report.production_write_allowed
    ))
}

pub(crate) fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let preflights = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "message.realtime.cutover.preflighted")
        .map(|receipt| {
            format!(
                r#"{{"preflight_id":{},"preflight_receipt_id":{},"policy_decision_id":{},"presence_request_receipt_id":{},"thread_id":{},"channel_id":{},"requested_realtime_scope":{},"terminal_state":{},"tenant_subscription_isolation_proven":true,"service_role_fanout_refused":true,"realtime_provider_turn_on_observed":false,"presence_mutation_allowed":false,"typing_indicator_allowed":false,"websocket_fanout_allowed":false,"production_delivery_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "preflight_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "presence_request_receipt_id")),
                json_string_literal(payload_value(receipt, "thread_id")),
                json_string_literal(payload_value(receipt, "channel_id")),
                json_string_literal(payload_value(receipt, "requested_realtime_scope")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-message-realtime-cutover-preflight-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/realtime-cutover-preflights.json","preflight_count":{},"tenant_subscription_isolation_proven":true,"service_role_fanout_refused":true,"realtime_provider_turn_on_observed":false,"presence_mutation_allowed":false,"typing_indicator_allowed":false,"websocket_fanout_allowed":false,"production_delivery_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"preflights":[{}]}}"#,
        preflights.len(),
        preflights.join(",")
    ))
}

fn render_delivery_replay_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let delivery_replay_batch_id = json_string_field(body, "delivery_replay_batch_id")
        .unwrap_or_else(|| "message_delivery_replay_local_001".to_string());
    let realtime_preflight_receipt_id =
        json_string_field(body, "realtime_preflight_receipt_id").unwrap_or_default();
    let channel_id =
        json_string_field(body, "channel_id").unwrap_or_else(|| "local-ops".to_string());
    let replay_scope = json_string_field(body, "replay_scope")
        .unwrap_or_else(|| "tenant channel delivery replay".to_string());
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
        .save_message_delivery_replay_batch_local_with_identity(
            MessageDeliveryReplayBatch {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                delivery_replay_batch_id: &delivery_replay_batch_id,
                realtime_preflight_receipt_id: &realtime_preflight_receipt_id,
                channel_id: &channel_id,
                replay_scope: &replay_scope,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-delivery-replay-batch-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/messages/delivery-replay-batches.json","projection_route":"/messages/delivery-replay-batches/projection.json","delivery_replay_batch_id":{},"delivery_replay_receipt_id":{},"policy_decision_id":{},"realtime_preflight_receipt_id":{},"channel_id":{},"replay_state":{},"rollback_safe_delivery_replay_proven":{},"websocket_fanout_allowed":{},"production_delivery_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.delivery_replay_batch_id),
        json_string_literal(&report.delivery_replay_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.realtime_preflight_receipt_id),
        json_string_literal(&report.channel_id),
        json_string_literal(&report.replay_state),
        report.rollback_safe_delivery_replay_proven,
        report.websocket_fanout_allowed,
        report.production_delivery_allowed,
        report.production_write_allowed
    ))
}

fn render_subscription_isolation_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let subscription_isolation_check_id =
        json_string_field(body, "subscription_isolation_check_id")
            .unwrap_or_else(|| "message_subscription_isolation_local_001".to_string());
    let realtime_preflight_receipt_id =
        json_string_field(body, "realtime_preflight_receipt_id").unwrap_or_default();
    let channel_id =
        json_string_field(body, "channel_id").unwrap_or_else(|| "local-ops".to_string());
    let isolation_scope = json_string_field(body, "isolation_scope")
        .unwrap_or_else(|| "tenant channel subscription isolation".to_string());
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
        .save_message_subscription_isolation_check_local_with_identity(
            MessageSubscriptionIsolationCheck {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                subscription_isolation_check_id: &subscription_isolation_check_id,
                realtime_preflight_receipt_id: &realtime_preflight_receipt_id,
                channel_id: &channel_id,
                isolation_scope: &isolation_scope,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-subscription-isolation-check-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/messages/subscription-isolation-checks.json","projection_route":"/messages/subscription-isolation-checks/projection.json","subscription_isolation_check_id":{},"subscription_isolation_receipt_id":{},"policy_decision_id":{},"realtime_preflight_receipt_id":{},"channel_id":{},"isolation_status":{},"tenant_subscription_isolation_proven":{},"service_role_fanout_allowed":{},"production_delivery_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.subscription_isolation_check_id),
        json_string_literal(&report.subscription_isolation_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.realtime_preflight_receipt_id),
        json_string_literal(&report.channel_id),
        json_string_literal(&report.isolation_status),
        report.tenant_subscription_isolation_proven,
        report.service_role_fanout_allowed,
        report.production_delivery_allowed,
        report.production_write_allowed
    ))
}

fn render_service_role_refusal_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let service_role_fanout_refusal_id = json_string_field(body, "service_role_fanout_refusal_id")
        .unwrap_or_else(|| "message_service_role_refusal_local_001".to_string());
    let realtime_preflight_receipt_id =
        json_string_field(body, "realtime_preflight_receipt_id").unwrap_or_default();
    let refusal_reason = json_string_field(body, "refusal_reason")
        .unwrap_or_else(|| "service-role fanout is not a local runtime authority".to_string());
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
        .save_message_service_role_fanout_refusal_local_with_identity(
            MessageServiceRoleFanoutRefusal {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                service_role_fanout_refusal_id: &service_role_fanout_refusal_id,
                realtime_preflight_receipt_id: &realtime_preflight_receipt_id,
                refusal_reason: &refusal_reason,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-service-role-fanout-refusal-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/messages/service-role-fanout-refusals.json","projection_route":"/messages/service-role-fanout-refusals/projection.json","service_role_fanout_refusal_id":{},"service_role_fanout_refusal_receipt_id":{},"policy_decision_id":{},"realtime_preflight_receipt_id":{},"refusal_reason":{},"service_role_fanout_refused":{},"presence_mutation_allowed":{},"typing_indicator_allowed":{},"production_delivery_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.service_role_fanout_refusal_id),
        json_string_literal(&report.service_role_fanout_refusal_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.realtime_preflight_receipt_id),
        json_string_literal(&report.refusal_reason),
        report.service_role_fanout_refused,
        report.presence_mutation_allowed,
        report.typing_indicator_allowed,
        report.production_delivery_allowed,
        report.production_write_allowed
    ))
}

fn render_delivery_replay_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let batches = projection_items(
        kernel,
        "message.delivery.replay.recorded",
        &[
            ("delivery_replay_batch_id", "delivery_replay_batch_id"),
            (
                "realtime_preflight_receipt_id",
                "realtime_preflight_receipt_id",
            ),
            ("channel_id", "channel_id"),
            ("replay_state", "replay_state"),
        ],
        &[
            ("rollback_safe_delivery_replay_proven", true),
            ("websocket_fanout_allowed", false),
            ("production_delivery_allowed", false),
            ("production_write_allowed", false),
        ],
    );
    Ok(format!(
        r#"{{"name":"mdx-message-delivery-replay-batch-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/delivery-replay-batches.json","batch_count":{},"rollback_safe_delivery_replay_proven":true,"websocket_fanout_allowed":false,"production_delivery_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"batches":[{}]}}"#,
        batches.0, batches.1
    ))
}

fn render_subscription_isolation_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let checks = projection_items(
        kernel,
        "message.subscription.isolation.checked",
        &[
            (
                "subscription_isolation_check_id",
                "subscription_isolation_check_id",
            ),
            (
                "realtime_preflight_receipt_id",
                "realtime_preflight_receipt_id",
            ),
            ("channel_id", "channel_id"),
            ("isolation_status", "isolation_status"),
        ],
        &[
            ("tenant_subscription_isolation_proven", true),
            ("service_role_fanout_allowed", false),
            ("production_delivery_allowed", false),
            ("production_write_allowed", false),
        ],
    );
    Ok(format!(
        r#"{{"name":"mdx-message-subscription-isolation-check-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/subscription-isolation-checks.json","check_count":{},"tenant_subscription_isolation_proven":true,"service_role_fanout_allowed":false,"production_delivery_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"checks":[{}]}}"#,
        checks.0, checks.1
    ))
}

fn render_service_role_refusal_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let refusals = projection_items(
        kernel,
        "message.service_role.fanout.refused",
        &[
            (
                "service_role_fanout_refusal_id",
                "service_role_fanout_refusal_id",
            ),
            (
                "realtime_preflight_receipt_id",
                "realtime_preflight_receipt_id",
            ),
            ("refusal_reason", "refusal_reason"),
            ("terminal_state", "terminal_state"),
        ],
        &[
            ("service_role_fanout_refused", true),
            ("presence_mutation_allowed", false),
            ("typing_indicator_allowed", false),
            ("production_delivery_allowed", false),
            ("production_write_allowed", false),
        ],
    );
    Ok(format!(
        r#"{{"name":"mdx-message-service-role-fanout-refusal-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/service-role-fanout-refusals.json","refusal_count":{},"service_role_fanout_refused":true,"presence_mutation_allowed":false,"typing_indicator_allowed":false,"production_delivery_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"refusals":[{}]}}"#,
        refusals.0, refusals.1
    ))
}

fn projection_items(
    kernel: &MdxKernel,
    receipt_kind: &str,
    string_fields: &[(&str, &str)],
    bool_fields: &[(&str, bool)],
) -> (usize, String) {
    let items = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == receipt_kind)
        .map(|receipt| {
            let mut fields = vec![
                format!(
                    r#""receipt_id":{}"#,
                    json_string_literal(&receipt.receipt_id)
                ),
                format!(
                    r#""policy_decision_id":{}"#,
                    json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or(""))
                ),
            ];
            for (json_key, payload_key) in string_fields {
                fields.push(format!(
                    r#""{}":{}"#,
                    json_key,
                    json_string_literal(payload_value(receipt, payload_key))
                ));
            }
            for (json_key, value) in bool_fields {
                fields.push(format!(r#""{}":{}"#, json_key, value));
            }
            format!("{{{}}}", fields.join(","))
        })
        .collect::<Vec<_>>();
    (items.len(), items.join(","))
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
pub(crate) mod tests {
    use super::*;
    use mdx_core::MessageThreadMessage;

    #[test]
    fn message_realtime_cutover_preflight_route_records_preflight_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/messages/realtime-cutover-preflights.json",
            r#"{"preflight_id":"message_realtime_route_test","thread_id":"thread_local_receipts","channel_id":"local-ops","requested_realtime_scope":"tenant channel replay"}"#,
            &kernel,
        )
        .expect("message realtime preflight route")
        .expect("route response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-message-realtime-cutover-preflight-local-post\"",
            "\"status\":\"REALTIME_CUTOVER_PREFLIGHT_RECORDED_PROVIDER_BLOCKED\"",
            "\"preflight_id\":\"message_realtime_route_test\"",
            "\"tenant_subscription_isolation_proven\":true",
            "\"service_role_fanout_refused\":true",
            "\"realtime_provider_turn_on_observed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/messages/realtime-cutover-preflights/projection.json",
            "",
            &kernel,
        )
        .expect("message realtime projection route")
        .expect("route response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-message-realtime-cutover-preflight-local-projection\"",
            "\"preflight_count\":1",
            "\"writes_route\":\"/messages/realtime-cutover-preflights.json\"",
            "\"preflight_id\":\"message_realtime_route_test\"",
            "\"terminal_state\":\"REALTIME_CUTOVER_PREFLIGHT_RECORDED_PROVIDER_BLOCKED\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn message_realtime_evidence_ladder_routes_record_local_proofs() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let preflight = route_response(
            "POST",
            "/messages/realtime-cutover-preflights.json",
            r#"{"preflight_id":"message_realtime_ladder_route_test","thread_id":"thread_local_receipts","channel_id":"local-ops","requested_realtime_scope":"tenant channel replay"}"#,
            &kernel,
        )
        .expect("message realtime preflight route")
        .expect("route response");
        assert_eq!(preflight.status, "200 OK");

        let replay = route_response(
            "POST",
            "/messages/delivery-replay-batches.json",
            r#"{"delivery_replay_batch_id":"delivery_replay_route_test","channel_id":"local-ops","replay_scope":"tenant channel delivery replay"}"#,
            &kernel,
        )
        .expect("delivery replay route")
        .expect("route response");
        assert_eq!(replay.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-message-delivery-replay-batch-local-post\"",
            "\"status\":\"ROLLBACK_SAFE_REPLAY_RECORDED_DELIVERY_BLOCKED\"",
            "\"rollback_safe_delivery_replay_proven\":true",
            "\"websocket_fanout_allowed\":false",
            "\"production_delivery_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(replay.body.contains(expected), "{expected}");
        }

        let isolation = route_response(
            "POST",
            "/messages/subscription-isolation-checks.json",
            r#"{"subscription_isolation_check_id":"subscription_isolation_route_test","channel_id":"local-ops","isolation_scope":"tenant channel subscription isolation"}"#,
            &kernel,
        )
        .expect("subscription isolation route")
        .expect("route response");
        assert_eq!(isolation.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-message-subscription-isolation-check-local-post\"",
            "\"status\":\"TENANT_CHANNEL_ISOLATION_PROVEN_LOCAL_ONLY\"",
            "\"tenant_subscription_isolation_proven\":true",
            "\"service_role_fanout_allowed\":false",
            "\"production_delivery_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(isolation.body.contains(expected), "{expected}");
        }

        let refusal = route_response(
            "POST",
            "/messages/service-role-fanout-refusals.json",
            r#"{"service_role_fanout_refusal_id":"service_role_refusal_route_test","refusal_reason":"service-role fanout is not a local runtime authority"}"#,
            &kernel,
        )
        .expect("service-role refusal route")
        .expect("route response");
        assert_eq!(refusal.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-message-service-role-fanout-refusal-local-post\"",
            "\"status\":\"SERVICE_ROLE_FANOUT_REFUSED_LOCAL_ONLY\"",
            "\"service_role_fanout_refused\":true",
            "\"presence_mutation_allowed\":false",
            "\"typing_indicator_allowed\":false",
            "\"production_delivery_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(refusal.body.contains(expected), "{expected}");
        }

        for (path, expected) in [
            (
                "/messages/delivery-replay-batches/projection.json",
                "\"batch_count\":1",
            ),
            (
                "/messages/subscription-isolation-checks/projection.json",
                "\"check_count\":1",
            ),
            (
                "/messages/service-role-fanout-refusals/projection.json",
                "\"refusal_count\":1",
            ),
        ] {
            let projection = route_response("GET", path, "", &kernel)
                .expect("projection route")
                .expect("route response");
            assert_eq!(projection.status, "200 OK");
            assert!(projection.body.contains(expected), "{expected}");
            assert!(
                projection
                    .body
                    .contains("\"production_write_allowed\":false")
            );
        }
    }

    #[test]
    fn message_relay_bridge_route_exposes_kernel_owned_descriptors() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel lock");
            kernel.run_evals_runner_agent().expect("seed evidence");
            let source_receipt_id = kernel.ledger().entries()[0].receipt_id.clone();
            kernel
                .save_message_thread_message_local(MessageThreadMessage {
                    tenant_id: "local_tenant",
                    actor_id: "human:local_user",
                    thread_id: "thread_local_receipts",
                    channel_id: "local-ops",
                    message_id: "bridge_descriptor_seed",
                    content_type: "text",
                    body: "Bridge descriptor should seed from the saved kernel sequence.",
                    content_payload: "Bridge descriptor should seed from the saved kernel sequence.",
                    source_receipt_id: &source_receipt_id,
                    outbox_topic: "message.human.posted",
                })
                .expect("message seed");
        }

        let bridge = route_response("GET", "/messages/relay-bridge.json", "", &kernel)
            .expect("bridge route")
            .expect("route response");
        assert_eq!(bridge.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-message-relay-bridge-metadata\"",
            "\"status\":\"LIVE-LOCAL-BRIDGE-METADATA-READY\"",
            "\"client_ws_env_key\":\"VITE_MDX_MESSAGE_RELAY_WS\"",
            "\"server_relay_url_env_key\":\"MDX_MESSAGE_RELAY_URL\"",
            "\"descriptor_count\":3",
            "\"message_sequence_high_watermark\":1",
            "\"stream_id\":\"message_local_ops\"",
            "\"stream_id\":\"ctx_event_stream\"",
            "\"stream_id\":\"dxr_event_stream\"",
            "\"cursor_kind\":\"message_sequence\"",
            "\"initial_after_cursor\":1",
            "\"metadata_cursor_use\":\"first_subscribe_seed_only\"",
            "\"client_owned_cursor_on_reconnect\":true",
            "\"server_watermark_on_reconnect_allowed\":false",
            "\"must_not_construct_tenant_or_topic_in_ui\":true",
            "\"must_not_claim_production_realtime_cutover\":true",
            "\"production_delivery_allowed\":false",
        ] {
            assert!(bridge.body.contains(expected), "{expected}");
        }
    }
}
