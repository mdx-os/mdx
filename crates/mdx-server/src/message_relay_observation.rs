use crate::RouteResponse;
use mdx_core::{GovernedWriteIdentity, MdxKernel, MessageRelayObservation, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/messages/relay-observations.json" {
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
            render_post_json(body, &mut kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/messages/relay-observations/projection.json" {
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
            render_projection_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

fn render_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
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
    let observation_id = json_string_field(body, "observation_id")
        .unwrap_or_else(|| "message_relay_observation_local_001".to_string());
    let realtime_preflight_receipt_id =
        json_string_field(body, "realtime_preflight_receipt_id").unwrap_or_default();
    let relay_driver = json_string_field(body, "relay_driver").unwrap_or_else(|| "valkey".into());
    let delivery_count = json_usize_field(body, "delivery_count").unwrap_or(24);
    let p99_post_start_to_delivery_ms =
        json_u32_field(body, "p99_post_start_to_delivery_ms").unwrap_or(12);
    let p99_post_finish_to_delivery_ms =
        json_u32_field(body, "p99_post_finish_to_delivery_ms").unwrap_or(2);
    let hot_cache_replay_status = json_string_field(body, "hot_cache_replay_status")
        .unwrap_or_else(|| "REPLAYED_FROM_RELAY_HOT_CACHE".into());
    let durable_replay_status = json_string_field(body, "durable_replay_status")
        .unwrap_or_else(|| "REPLAYED_FROM_DURABLE_RELAY_LOG_AFTER_RESTART".into());
    let typing_signal_status = json_string_field(body, "typing_signal_status")
        .unwrap_or_else(|| "TYPING_SIGNAL_PROVEN".into());
    let relay_health_status =
        json_string_field(body, "relay_health_status").unwrap_or_else(|| "LIVE-LOCAL-RELAY".into());
    let report = kernel
        .save_message_relay_observation_local_with_identity(
            MessageRelayObservation {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                observation_id: &observation_id,
                realtime_preflight_receipt_id: &realtime_preflight_receipt_id,
                relay_driver: &relay_driver,
                delivery_count,
                p99_post_start_to_delivery_ms,
                p99_post_finish_to_delivery_ms,
                hot_cache_replay_status: &hot_cache_replay_status,
                durable_replay_status: &durable_replay_status,
                typing_signal_status: &typing_signal_status,
                relay_health_status: &relay_health_status,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-message-relay-observation-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/messages/relay-observations.json","projection_route":"/messages/relay-observations/projection.json","observation_id":{},"relay_observation_receipt_id":{},"policy_decision_id":{},"realtime_preflight_receipt_id":{},"relay_driver":{},"delivery_count":{},"p99_post_start_to_delivery_ms":{},"p99_post_finish_to_delivery_ms":{},"p99_budget_ms":{},"hot_cache_replay_status":{},"durable_replay_status":{},"typing_signal_status":{},"relay_health_status":{},"local_websocket_fanout_allowed":{},"production_delivery_allowed":{},"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.observation_id),
        json_string_literal(&report.relay_observation_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.realtime_preflight_receipt_id),
        json_string_literal(&report.relay_driver),
        report.delivery_count,
        report.p99_post_start_to_delivery_ms,
        report.p99_post_finish_to_delivery_ms,
        report.p99_budget_ms,
        json_string_literal(&report.hot_cache_replay_status),
        json_string_literal(&report.durable_replay_status),
        json_string_literal(&report.typing_signal_status),
        json_string_literal(&report.relay_health_status),
        report.local_websocket_fanout_allowed,
        report.production_delivery_allowed,
        report.production_write_allowed
    ))
}

fn render_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let observations = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "message.relay.observed")
        .map(|receipt| {
            format!(
                r#"{{"observation_id":{},"relay_observation_receipt_id":{},"policy_decision_id":{},"realtime_preflight_receipt_id":{},"relay_driver":{},"delivery_count":{},"p99_post_start_to_delivery_ms":{},"p99_post_finish_to_delivery_ms":{},"p99_budget_ms":{},"hot_cache_replay_status":{},"durable_replay_status":{},"typing_signal_status":{},"relay_health_status":{},"local_websocket_fanout_allowed":true,"production_delivery_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "observation_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "realtime_preflight_receipt_id")),
                json_string_literal(payload_value(receipt, "relay_driver")),
                payload_value(receipt, "delivery_count"),
                payload_value(receipt, "p99_post_start_to_delivery_ms"),
                payload_value(receipt, "p99_post_finish_to_delivery_ms"),
                payload_value(receipt, "p99_budget_ms"),
                json_string_literal(payload_value(receipt, "hot_cache_replay_status")),
                json_string_literal(payload_value(receipt, "durable_replay_status")),
                json_string_literal(payload_value(receipt, "typing_signal_status")),
                json_string_literal(payload_value(receipt, "relay_health_status"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-message-relay-observation-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/relay-observations.json","projection_route":"/messages/relay-observations/projection.json","observation_count":{},"local_websocket_fanout_allowed":{},"production_delivery_allowed":false,"production_write_allowed":false,"observations":[{}]}}"#,
        observations.len(),
        !observations.is_empty(),
        observations.join(",")
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

fn json_usize_field(body: &str, key: &str) -> Option<usize> {
    json_number_field(body, key)?.parse().ok()
}

fn json_u32_field(body: &str, key: &str) -> Option<u32> {
    json_number_field(body, key)?.parse().ok()
}

fn json_number_field<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let end = after_colon
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(after_colon.len());
    (end > 0).then(|| &after_colon[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_relay_observation_routes_record_observed_relay_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/messages/relay-observations.json",
            r#"{"observation_id":"relay_route_test","delivery_count":24,"p99_post_start_to_delivery_ms":12,"p99_post_finish_to_delivery_ms":2}"#,
            &kernel,
        )
        .expect("observation route")
        .expect("observation response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-message-relay-observation-local-post\"",
            "\"status\":\"MESSAGE_RELAY_OBSERVED_LOCAL_PRODUCTION_BLOCKED\"",
            "\"observation_id\":\"relay_route_test\"",
            "\"local_websocket_fanout_allowed\":true",
            "\"production_delivery_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/messages/relay-observations/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        for expected in [
            "\"name\":\"mdx-message-relay-observation-local-projection\"",
            "\"observation_count\":1",
            "\"observation_id\":\"relay_route_test\"",
            "\"hot_cache_replay_status\":\"REPLAYED_FROM_RELAY_HOT_CACHE\"",
            "\"durable_replay_status\":\"REPLAYED_FROM_DURABLE_RELAY_LOG_AFTER_RESTART\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }
}
