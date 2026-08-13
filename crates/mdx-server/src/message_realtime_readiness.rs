use crate::RouteResponse;
use mdx_core::{MdxKernel, Receipt, json_string_literal};
use std::sync::{Arc, RwLock};

const ROUTE: &str = "/messages/realtime-readiness.json";
const PREFLIGHT_KIND: &str = "message.realtime.cutover.preflighted";
const DELIVERY_REPLAY_KIND: &str = "message.delivery.replay.recorded";
const SUBSCRIPTION_ISOLATION_KIND: &str = "message.subscription.isolation.checked";
const SERVICE_ROLE_REFUSAL_KIND: &str = "message.service_role.fanout.refused";
const RELAY_OBSERVATION_KIND: &str = "message.relay.observed";

pub(crate) fn route_response(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != ROUTE {
        return None;
    }
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
    Some(render_realtime_readiness_json(&kernel).map(|body| RouteResponse::json("200 OK", body)))
}

pub(crate) fn render_realtime_readiness_json(kernel: &MdxKernel) -> Result<String, String> {
    let preflight_count = receipt_count(kernel, PREFLIGHT_KIND);
    let delivery_replay_count = receipt_count(kernel, DELIVERY_REPLAY_KIND);
    let subscription_isolation_count = receipt_count(kernel, SUBSCRIPTION_ISOLATION_KIND);
    let service_role_refusal_count = receipt_count(kernel, SERVICE_ROLE_REFUSAL_KIND);
    let relay_observation_count = receipt_count(kernel, RELAY_OBSERVATION_KIND);
    let governed_ladder_ready = preflight_count > 0
        && delivery_replay_count > 0
        && subscription_isolation_count > 0
        && service_role_refusal_count > 0;
    let local_ready = governed_ladder_ready && relay_observation_count > 0;
    let status = if local_ready {
        "LIVE-LOCAL-REALTIME-READY-PRODUCTION-BLOCKED"
    } else if governed_ladder_ready {
        "LOCAL-REALTIME-MISSING-RELAY-OBSERVATION"
    } else {
        "LOCAL-REALTIME-MISSING-GOVERNED-EVIDENCE"
    };
    Ok(format!(
        r#"{{"name":"mdx-message-realtime-readiness","status":{},"route":"{}","read_only":true,"receipt_backed":true,"local_ready":{},"relay_runtime":{{"crate":"mdx-message-relay","driver":"valkey","transport":"websocket_valkey_pubsub","local_live_proof_check":"make message-realtime-relay-check","health_route":"/health","websocket_route":"/messages/realtime/ws","jwt_subprotocol_required":true,"bounded_hot_cache":true,"durable_replay_driver":"local_jsonl_relay_replay_log","p99_budget_ms":20}},"bridge":{{"route":"/messages/relay-bridge.json","descriptor_source":"kernel_route","descriptor_count":3,"metadata_cursor_use":"first_subscribe_seed_only","client_owned_cursor_on_reconnect":true,"server_watermark_on_reconnect_allowed":false}},"governed_ladder":{{"preflight_count":{},"delivery_replay_count":{},"subscription_isolation_count":{},"service_role_refusal_count":{},"required_receipt_kinds":["{}","{}","{}","{}"],"source_receipt_ids":[{}]}},"relay_observation":{{"write_route":"/messages/relay-observations.json","projection_route":"/messages/relay-observations/projection.json","observation_count":{},"required_receipt_kind":"{}","observed_under_budget":{},"source_receipt_ids":[{}]}},"local_authority":{{"websocket_fanout_allowed":{},"typing_signal_allowed":{},"rollback_safe_delivery_replay_proven":{},"tenant_subscription_isolation_proven":{},"service_role_fanout_refused":{}}},"blocked_authority":{{"production_delivery_allowed":false,"service_role_fanout_allowed":false,"presence_mutation_allowed":false,"provider_call_allowed":false,"production_write_allowed":false,"production_cutover_allowed":false}},"source_routes":["/messages/relay-bridge.json","/messages/thread-messages/projection.json","/messages/realtime-cutover-preflights/projection.json","/messages/delivery-replay-batches/projection.json","/messages/subscription-isolation-checks/projection.json","/messages/service-role-fanout-refusals/projection.json","/messages/relay-observations/projection.json"],"ui_handoff":{{"target_owner":"claude","target_surface":"apps/mdx Message module","safe_to_show_local_realtime_ready":{},"must_fetch_bridge_descriptors":true,"must_preserve_client_owned_cursor":true,"must_show_relay_observation_receipt":true,"must_not_construct_tenant_or_topic_in_ui":true,"must_not_claim_production_realtime_cutover":true,"must_not_enable_service_role_fanout":true}},"checks":["make message-realtime-readiness-check","make message-realtime-relay-check","make message-relay-bridge-check","make message-relay-observation-check"]}}"#,
        json_string_literal(status),
        ROUTE,
        local_ready,
        preflight_count,
        delivery_replay_count,
        subscription_isolation_count,
        service_role_refusal_count,
        PREFLIGHT_KIND,
        DELIVERY_REPLAY_KIND,
        SUBSCRIPTION_ISOLATION_KIND,
        SERVICE_ROLE_REFUSAL_KIND,
        source_receipt_ids_json(kernel, false),
        relay_observation_count,
        RELAY_OBSERVATION_KIND,
        relay_observation_count > 0,
        source_receipt_ids_json(kernel, true),
        relay_observation_count > 0,
        relay_observation_count > 0,
        delivery_replay_count > 0,
        subscription_isolation_count > 0,
        service_role_refusal_count > 0,
        local_ready
    ))
}

fn receipt_count(kernel: &MdxKernel, kind: &str) -> usize {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .count()
}

fn source_receipt_ids_json(kernel: &MdxKernel, relay_only: bool) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            if relay_only {
                receipt.kind == RELAY_OBSERVATION_KIND
            } else {
                [
                    PREFLIGHT_KIND,
                    DELIVERY_REPLAY_KIND,
                    SUBSCRIPTION_ISOLATION_KIND,
                    SERVICE_ROLE_REFUSAL_KIND,
                ]
                .contains(&receipt.kind.as_str())
            }
        })
        .map(receipt_id_json)
        .collect::<Vec<_>>()
        .join(",")
}

fn receipt_id_json(receipt: &Receipt) -> String {
    json_string_literal(&receipt.receipt_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_realtime_readiness_reports_missing_then_ready_with_production_blocked() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let empty = route_response("GET", ROUTE, &kernel)
            .expect("readiness route")
            .expect("route response");
        assert_eq!(empty.status, "200 OK");
        for expected in [
            "\"status\":\"LOCAL-REALTIME-MISSING-GOVERNED-EVIDENCE\"",
            "\"local_ready\":false",
            "\"preflight_count\":0",
            "\"observation_count\":0",
            "\"safe_to_show_local_realtime_ready\":false",
            "\"production_delivery_allowed\":false",
            "\"production_cutover_allowed\":false",
        ] {
            assert!(empty.body.contains(expected), "{expected}");
        }

        let preflight = crate::message_realtime_cutover_preflight::route_response(
            "POST",
            "/messages/realtime-cutover-preflights.json",
            r#"{"preflight_id":"message_realtime_readiness_test","thread_id":"thread_local_receipts","channel_id":"local-ops","requested_realtime_scope":"tenant channel replay"}"#,
            &kernel,
        )
        .expect("preflight route")
        .expect("preflight response");
        let preflight_receipt_id =
            json_field(&preflight.body, "preflight_receipt_id").expect("preflight receipt");
        for (path, body) in [
            (
                "/messages/delivery-replay-batches.json",
                format!(
                    r#"{{"delivery_replay_batch_id":"message_readiness_delivery_replay","realtime_preflight_receipt_id":"{}","channel_id":"local-ops","replay_scope":"tenant channel delivery replay"}}"#,
                    preflight_receipt_id
                ),
            ),
            (
                "/messages/subscription-isolation-checks.json",
                format!(
                    r#"{{"subscription_isolation_check_id":"message_readiness_subscription_isolation","realtime_preflight_receipt_id":"{}","channel_id":"local-ops","isolation_scope":"tenant channel subscription isolation"}}"#,
                    preflight_receipt_id
                ),
            ),
            (
                "/messages/service-role-fanout-refusals.json",
                format!(
                    r#"{{"service_role_fanout_refusal_id":"message_readiness_service_role_refusal","realtime_preflight_receipt_id":"{}","refusal_reason":"service-role fanout is not a local runtime authority"}}"#,
                    preflight_receipt_id
                ),
            ),
        ] {
            let response = crate::message_realtime_cutover_preflight::route_response(
                "POST", path, &body, &kernel,
            )
            .expect("ladder route")
            .expect("ladder response");
            assert_eq!(response.status, "200 OK");
        }

        let ready = route_response("GET", ROUTE, &kernel)
            .expect("readiness route")
            .expect("route response");
        assert_eq!(ready.status, "200 OK");
        for expected in [
            "\"status\":\"LOCAL-REALTIME-MISSING-RELAY-OBSERVATION\"",
            "\"local_ready\":false",
            "\"observation_count\":0",
            "\"must_show_relay_observation_receipt\":true",
        ] {
            assert!(ready.body.contains(expected), "{expected}");
        }

        let observation = crate::message_relay_observation::route_response(
            "POST",
            "/messages/relay-observations.json",
            &format!(
                r#"{{"observation_id":"message_readiness_relay_observation","realtime_preflight_receipt_id":"{}","delivery_count":24,"p99_post_start_to_delivery_ms":12,"p99_post_finish_to_delivery_ms":2}}"#,
                preflight_receipt_id
            ),
            &kernel,
        )
        .expect("observation route")
        .expect("observation response");
        assert_eq!(observation.status, "200 OK");

        let ready = route_response("GET", ROUTE, &kernel)
            .expect("readiness route")
            .expect("route response");
        assert_eq!(ready.status, "200 OK");
        for expected in [
            "\"status\":\"LIVE-LOCAL-REALTIME-READY-PRODUCTION-BLOCKED\"",
            "\"local_ready\":true",
            "\"preflight_count\":1",
            "\"delivery_replay_count\":1",
            "\"subscription_isolation_count\":1",
            "\"service_role_refusal_count\":1",
            "\"observation_count\":1",
            "\"observed_under_budget\":true",
            "\"local_live_proof_check\":\"make message-realtime-relay-check\"",
            "\"websocket_fanout_allowed\":true",
            "\"rollback_safe_delivery_replay_proven\":true",
            "\"tenant_subscription_isolation_proven\":true",
            "\"service_role_fanout_refused\":true",
            "\"must_fetch_bridge_descriptors\":true",
            "\"must_not_claim_production_realtime_cutover\":true",
            "\"production_delivery_allowed\":false",
            "\"service_role_fanout_allowed\":false",
            "\"production_write_allowed\":false",
            "\"production_cutover_allowed\":false",
        ] {
            assert!(ready.body.contains(expected), "{expected}");
        }
    }

    fn json_field(body: &str, key: &str) -> Option<String> {
        let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
        let after_colon = after_key.split_once(':')?.1.trim_start();
        let value = after_colon.strip_prefix('"')?;
        Some(value.split('"').next()?.to_string())
    }
}
