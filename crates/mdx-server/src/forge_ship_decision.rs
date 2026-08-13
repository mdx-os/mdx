// Rail Slice 2: the governed server WRITE route for the human ship decision.
//
// POST /forge/ship-decisions.json parses the request body and calls the kernel
// method kernel.record_forge_human_ship_decision(&ForgeShipDecisionRequest).
// The route NEVER constructs or emits receipts itself and NEVER sets any
// authority true on its own - the kernel is the single authority for ship
// ratification. deployment_authority_granted and production_write_allowed are
// always false here; this door ratifies a ship, never a deployment or a
// production write. The response carries only the decision verdict and the
// receipt and policy ids, never a secret, prompt, model output, diff body, or
// command output.
use crate::RouteResponse;
use mdx_core::{
    ForgeShipDecisionRequest, GovernedWriteIdentity, HumanShipDecision, MdxKernel, Receipt,
    json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/forge/ship-decisions.json" {
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
    if path == "/forge/ship-decisions/projection.json" {
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
    // The verified trusted session is the human making the call; otherwise the
    // local-demo fixture (kernel-save human_actor_id `human:local_owner`, the
    // response auth fields their own `local_tenant`/`local_user`/`owner`).
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
    let human_actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "human_actor_id").unwrap_or_else(|| "human:local_owner".to_string())
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&human_actor_id)
    };
    let human_actor_kind =
        json_string_field(body, "human_actor_kind").unwrap_or_else(|| "human".to_string());
    let decision_raw = json_string_field(body, "decision").unwrap_or_else(|| "ratify".to_string());
    let requested_authority = json_string_field(body, "requested_authority")
        .unwrap_or_else(|| "ship_ratification".to_string());
    let authorization_receipt_id =
        json_string_field(body, "authorization_receipt_id").unwrap_or_default();
    let review_packet_receipt_id =
        json_string_field(body, "review_packet_receipt_id").unwrap_or_default();
    let ship_readiness_receipt_id =
        json_string_field(body, "ship_readiness_receipt_id").unwrap_or_default();
    let worker_build_receipt_id =
        json_string_field(body, "worker_build_receipt_id").unwrap_or_default();
    let ci_evidence_receipt_id =
        json_string_field(body, "ci_evidence_receipt_id").unwrap_or_default();
    let target_commit_sha = json_string_field(body, "target_commit_sha").unwrap_or_default();
    let scope = json_string_field(body, "scope").unwrap_or_default();
    let decision_reason = json_string_field(body, "decision_reason").unwrap_or_default();
    let decided_at = json_string_field(body, "decided_at").unwrap_or_default();
    let expires_at = json_string_field(body, "expires_at").unwrap_or_default();

    // An unknown decision verb is a route-level shape refusal, not a kernel call.
    let Some(decision) = HumanShipDecision::parse_decision(&decision_raw) else {
        return Err(format!("unknown ship decision: {decision_raw}"));
    };

    let report = kernel.record_forge_human_ship_decision_with_identity(
        &ForgeShipDecisionRequest {
            tenant_id: &tenant_id,
            human_actor_id: &human_actor_id,
            human_actor_kind: &human_actor_kind,
            decision,
            requested_authority: &requested_authority,
            authorization_receipt_id: &authorization_receipt_id,
            review_packet_receipt_id: &review_packet_receipt_id,
            ship_readiness_receipt_id: &ship_readiness_receipt_id,
            worker_build_receipt_id: &worker_build_receipt_id,
            ci_evidence_receipt_id: &ci_evidence_receipt_id,
            target_commit_sha: &target_commit_sha,
            scope: &scope,
            decision_reason: &decision_reason,
            decided_at: &decided_at,
            expires_at: &expires_at,
        },
        &identity,
    )?;

    // Derive the implicit learning signal from the disposition just recorded.
    // A human read the diff and either accepted it, rejected it, or sent it
    // back for revision - the highest-signal learning source, at zero extra
    // cost. Only the three real dispositions map to a signal; a blocked or
    // escalated decision is not a disposition on the change itself. Fail-soft:
    // the ship door must never break because its learning signal could not be
    // derived.
    if let Some(signal_kind) = disposition_signal_kind(&report.terminal_state) {
        let work_item = if scope.trim().is_empty() {
            target_commit_sha.clone()
        } else {
            scope.clone()
        };
        let lesson = crate::learning_routes::implicit_signal::disposition_lesson(
            signal_kind,
            &work_item,
            &decision_reason,
        );
        crate::learning_routes::implicit_signal::derive_implicit_signal(
            kernel,
            &identity,
            mdx_core::LearningImplicitSignal {
                tenant_id: &tenant_id,
                actor_id: &human_actor_id,
                source_receipt_id: &report.decision_receipt_id,
                signal_kind,
                work_item: &work_item,
                delta_summary: &decision_reason,
                lesson_candidate: &lesson,
            },
        );
    }

    Ok(format!(
        r#"{{"name":"mdx-forge-ship-decision-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/forge/ship-decisions.json","projection_route":"/forge/ship-decisions/projection.json","decision":{},"terminal_state":{},"decision_receipt_id":{},"policy_decision_id":{},"denied_reason":{},"next_safe_action":{},"forge_ship_ratified":{},"ship_authority_allowed":{},"deployment_authority_granted":false,"deployment_allowed":false,"live_substrate_required":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.decision),
        json_string_literal(&report.terminal_state),
        json_string_literal(&report.decision_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(report.denied_reason.as_deref().unwrap_or("")),
        json_string_literal(&report.next_safe_action),
        report.forge_ship_ratified,
        report.ship_authority_allowed,
    ))
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let decisions = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_forge_ship_decision_receipt(receipt))
        .map(|receipt| {
            let ratified = receipt.kind == "forge.ship.ratified";
            format!(
                r#"{{"decision_receipt_id":{},"policy_decision_id":{},"decision":{},"terminal_state":{},"denied_reason":{},"forge_ship_ratified":{},"ship_authority_allowed":{},"deployment_authority_granted":false,"deployment_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "decision")),
                json_string_literal(payload_value(receipt, "terminal_state")),
                json_string_literal(payload_value(receipt, "denied_reason")),
                ratified,
                ratified,
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-forge-ship-decision-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/forge/ship-decisions.json","ship_decision_count":{},"deployment_authority_granted":false,"deployment_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"decisions":[{}]}}"#,
        decisions.len(),
        decisions.join(",")
    ))
}

// The five decision receipts and the first-class blocked decision are the ship
// decisions this projection reports. They are the only kinds emitted by
// kernel.record_forge_human_ship_decision.
fn is_forge_ship_decision_receipt(receipt: &Receipt) -> bool {
    matches!(
        receipt.kind.as_str(),
        "forge.ship.ratified"
            | "forge.ship.rejected"
            | "forge.ship.revision_requested"
            | "forge.ship.tests_rerun_requested"
            | "forge.ship.escalated"
            | "forge.ship.decision_blocked"
    )
}

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

/// The implicit-signal disposition class for a ship decision, or None for
/// decisions that are not a disposition on the change itself (a blocked
/// decision, an escalation, or a tests-rerun request).
fn disposition_signal_kind(terminal_state: &str) -> Option<&'static str> {
    match terminal_state {
        "SHIP_RATIFIED" => Some("change_accepted"),
        "SHIP_REJECTED" => Some("change_rejected"),
        "SHIP_REVISION_REQUESTED" => Some("revision_requested"),
        _ => None,
    }
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
    fn ship_decision_route_blocks_without_the_evidence_chain_and_grants_no_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/ship-decisions.json",
            r#"{"decision":"ratify","human_actor_id":"human_platform_lead","human_actor_kind":"human","authorization_receipt_id":"missing","review_packet_receipt_id":"missing","ship_readiness_receipt_id":"missing","worker_build_receipt_id":"missing","ci_evidence_receipt_id":"missing","target_commit_sha":"deadbeef","scope":"crates/mdx-core","decision_reason":"shape only","decided_at":"2026-06-08T01:00:00Z","expires_at":"2026-06-08T02:00:00Z"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-forge-ship-decision-local-post\"",
            "\"status\":\"SHIP_DECISION_BLOCKED\"",
            "\"terminal_state\":\"SHIP_DECISION_BLOCKED\"",
            "\"forge_ship_ratified\":false",
            "\"ship_authority_allowed\":false",
            "\"deployment_authority_granted\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.body_text().contains(expected), "{expected}");
        }
        // A first-class blocked decision still records a decision receipt id.
        assert!(
            response
                .body_text()
                .contains("\"decision_receipt_id\":\"forge_human_ship_decision_receipt_"),
            "expected a recorded decision receipt id: {}",
            response.body_text()
        );
        // The route returns the denial reason, never a secret or output.
        assert!(
            response
                .body_text()
                .contains("\"denied_reason\":\"missing_")
        );

        let projection =
            route_response("GET", "/forge/ship-decisions/projection.json", "", &kernel)
                .expect("projection route")
                .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-forge-ship-decision-local-projection\"",
            "\"ship_decision_count\":1",
            "\"writes_route\":\"/forge/ship-decisions.json\"",
            "\"production_write_allowed\":false",
        ] {
            assert!(projection.body_text().contains(expected), "{expected}");
        }
    }

    #[test]
    fn ship_decision_route_refuses_an_unknown_decision_verb() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let result = route_response(
            "POST",
            "/forge/ship-decisions.json",
            r#"{"decision":"deploy"}"#,
            &kernel,
        )
        .expect("route");
        assert!(result.is_err());
    }

    #[test]
    fn ship_decision_route_rejects_a_non_post_method() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response("GET", "/forge/ship-decisions.json", "", &kernel)
            .expect("route")
            .expect("response");
        assert_eq!(response.status, "405 Method Not Allowed");
    }
}
