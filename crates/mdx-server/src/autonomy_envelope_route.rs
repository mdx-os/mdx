// The on-switch for governed autonomy (ADR 0488). An autonomy envelope is a
// human-owned, revocable, expiring grant; the registry that records it existed
// and was tested, but nothing HTTP exposed it, so the dial had no live switch.
// This route is that switch: a human records an envelope (including a
// self-delivery scope) that lets bounded autonomous work - up to and including
// a run opening, converging, and merging its own PR - complete without a
// per-change human click. It never grants deployment authority; that stays a
// separate human decision (ADR 0139).
use crate::RouteResponse;
use mdx_core::{
    AutonomyEnvelopeRecordRequest, AutonomyEnvelopeRevokeRequest, AutonomyPolicyEnvelope,
    AutonomyRiskClass, HarnessApprovalMode, HarnessBudgetPolicy, HarnessPermissionPolicy,
    HarnessRunManifest, HarnessRunMode, HarnessTrustBoundary, LocalAutonomyEnvelopeRegistry,
    MdxKernel, SelfDeliveryScope, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/autonomy/envelopes.json" && path != "/autonomy/envelopes/revocations.json" {
        return None;
    }
    if path == "/autonomy/envelopes/revocations.json" {
        if let Some(response) = crate::reject_unless_method(method, "POST") {
            return Some(Ok(response));
        }
        return Some(handle_revoke(body, kernel));
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    Some(handle(body, kernel))
}

fn handle_revoke(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "human:owner",
        "owner",
    );
    let envelope_id = json_string_field(body, "envelope_id").unwrap_or_default();
    if envelope_id.trim().is_empty() {
        return Ok(refusal("name the envelope_id"));
    }
    let reason = json_string_field(body, "reason")
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "operator revocation".to_string());

    // Locate most recent autonomy.envelope.recorded receipt for envelope_id
    let envelope_receipt_id = {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        guard
            .ledger()
            .query()
            .by_kind("autonomy.envelope.recorded")
            .iter()
            .rev()
            .find(|r| {
                r.payload.get("envelope_id").map(String::as_str) == Some(envelope_id.as_str())
            })
            .map(|r| r.receipt_id.clone())
            .ok_or_else(|| format!("no recorded envelope {}", envelope_id))
    };
    let envelope_receipt_id = match envelope_receipt_id {
        Ok(id) => id,
        Err(msg) => return Ok(refusal(&msg)),
    };

    let manifest = envelope_record_manifest(&resolved.tenant_id, &resolved.actor_id);
    let revocation_receipt_id = {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        match LocalAutonomyEnvelopeRegistry.revoke(
            &mut guard,
            &manifest,
            &AutonomyEnvelopeRevokeRequest {
                envelope_receipt_id: &envelope_receipt_id,
                revoked_by: &resolved.actor_id,
                revocation_reason: &reason,
            },
        ) {
            Ok(id) => id,
            Err(error) => return Ok(refusal(&error)),
        }
    };

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            "{{\"name\":\"mdx-autonomy-envelope-revocation\",\"status\":\"ENVELOPE_REVOKED\",\"envelope_id\":{},\"revocation_receipt_id\":{},\"deployment_authority_granted\":false}}",
            json_string_literal(&envelope_id),
            json_string_literal(&revocation_receipt_id),
        ),
    ))
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value[key].as_str().map(str::to_string)
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            "{{\"name\":\"mdx-autonomy-envelope\",\"status\":\"REFUSED\",\"reason\":{}}}",
            json_string_literal(reason)
        ),
    )
}

// Accept a list field as either a JSON array of strings or a comma-separated
// string, so the switch is easy to throw from a shell or a form.
fn list_field(body: &str, key: &str) -> Vec<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(array) = value[key].as_array()
    {
        return array
            .iter()
            .filter_map(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect();
    }
    json_string_field(body, key)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn handle(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "human:owner",
        "owner",
    );
    let envelope_id = json_string_field(body, "envelope_id").unwrap_or_default();
    if envelope_id.trim().is_empty() {
        return Ok(refusal("name the envelope_id"));
    }
    let owner_id = json_string_field(body, "owner_id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| resolved.actor_id.clone());
    let scope = SelfDeliveryScope::from_wire(
        &json_string_field(body, "self_delivery_scope").unwrap_or_default(),
    );
    let allowed_path_scopes = list_field(body, "allowed_path_scopes");
    if allowed_path_scopes.is_empty() {
        return Ok(refusal(
            "an envelope must name at least one allowed_path_scope prefix",
        ));
    }
    let preapproved = {
        let mut classes = list_field(body, "preapproved_work_classes");
        if classes.is_empty() {
            classes.push("self_improvement".to_string());
        }
        classes
    };
    let escalation_triggers = list_field(body, "escalation_triggers");
    let expires_at = json_string_field(body, "expires_at")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "2030-01-01T00:00:00Z".to_string());
    let max_risk_class = match json_string_field(body, "max_risk_class")
        .unwrap_or_default()
        .trim()
    {
        "high" => AutonomyRiskClass::High,
        "medium" => AutonomyRiskClass::Medium,
        _ => AutonomyRiskClass::Low,
    };

    // Borrow the parsed lists for the envelope's &[&str] fields.
    let allowed_refs: Vec<&str> = allowed_path_scopes.iter().map(String::as_str).collect();
    let preapproved_refs: Vec<&str> = preapproved.iter().map(String::as_str).collect();
    let trigger_refs: Vec<&str> = escalation_triggers.iter().map(String::as_str).collect();

    let envelope = AutonomyPolicyEnvelope {
        envelope_id: &envelope_id,
        owner_id: &owner_id,
        preapproved_work_classes: &preapproved_refs,
        max_risk_class,
        allowed_path_scopes: &allowed_refs,
        rollback_required: false,
        eval_required: false,
        escalation_triggers: &trigger_refs,
        expires_at: &expires_at,
        active: true,
        self_delivery_scope: scope,
    };

    let manifest = envelope_record_manifest(&resolved.tenant_id, &owner_id);
    let receipt_id = {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        match LocalAutonomyEnvelopeRegistry.record(
            &mut guard,
            &manifest,
            &AutonomyEnvelopeRecordRequest {
                envelope,
                recorded_by: &resolved.actor_id,
            },
        ) {
            Ok(receipt_id) => receipt_id,
            Err(error) => return Ok(refusal(&error)),
        }
    };

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            "{{\"name\":\"mdx-autonomy-envelope\",\"status\":\"ENVELOPE_RECORDED\",\"envelope_id\":{},\"owner_id\":{},\"self_delivery_scope\":\"{}\",\"allowed_path_scopes\":[{}],\"expires_at\":{},\"receipt_id\":{},\"ship_ratification_grantable\":{},\"deployment_authority_granted\":false,\"production_write_allowed\":false}}",
            json_string_literal(&envelope_id),
            json_string_literal(&owner_id),
            scope.as_str(),
            allowed_path_scopes
                .iter()
                .map(|scope| json_string_literal(scope))
                .collect::<Vec<_>>()
                .join(","),
            json_string_literal(&expires_at),
            json_string_literal(&receipt_id),
            scope.is_self_delivery(),
        ),
    ))
}

// A minimal valid manifest; the registry reads only tenant_id and actor_id from
// it for correlation.
fn envelope_record_manifest<'a>(tenant_id: &'a str, actor_id: &'a str) -> HarnessRunManifest<'a> {
    HarnessRunManifest {
        run_id: "autonomy_envelope_record",
        tenant_id,
        actor_id,
        origin_surface: "autonomy_envelope_route",
        mode: HarnessRunMode::PlanOnly,
        goal_summary: "record a human-owned autonomy envelope",
        definition_of_done: "envelope recorded on the ledger",
        input_artifacts: &["docs/adr/0488-governed-autonomous-delivery-envelope.md"],
        allowed_write_scope: &["crates/mdx-core/src/"],
        blocked_paths: &[".git/", ".env", ".mdx-local/"],
        allowed_tools: &[],
        blocked_tools: &["shell", "git", "patch", "browser", "mcp", "network"],
        default_parallelism: 1,
        allowed_model_profiles: &["deterministic_stub"],
        provider_fail_closed: true,
        allowed_context_sources: &["manifest", "source_map", "verification_manifest"],
        context_redaction_required: true,
        permission_policy: HarnessPermissionPolicy::plan_only_local(),
        budget_policy: HarnessBudgetPolicy {
            max_turns: 1,
            max_tool_calls: 0,
            max_runtime_ms: 250,
            max_context_tokens: 4096,
            max_output_tokens: 1024,
            max_event_bytes: 8192,
            max_transcript_bytes: 16384,
            max_cost_cents: 0,
        },
        quality_gates: &["cargo test -p mdx-core"],
        approval_mode: HarnessApprovalMode::PlanHash,
        approval_receipt_required: true,
        required_receipt_kinds: &[
            "harness.run.admitted",
            "harness.plan.emitted",
            "harness.write.refused",
            "harness.provider.profile.selected",
        ],
        telemetry_redaction_required: true,
        exit_criteria: &["evidence_ready_for_human", "human_decision_recorded"],
        trust_boundary: HarnessTrustBoundary {
            workspace_trusted: true,
            project_local_execution_allowed: false,
            mcp_startup_allowed: false,
            network_allowed: false,
            secrets_available: false,
            approval_receipt_id: Some("human_recorded_envelope"),
        },
        policy_profile: Some("core"),
        enterprise_pack: Some("none"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_a_self_open_pr_envelope_and_reports_it() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let body = r#"{"envelope_id":"env_live_test","owner_id":"human:owner","self_delivery_scope":"self_open_pr","allowed_path_scopes":["docs/"],"expires_at":"2030-01-01T00:00:00Z"}"#;
        let response = route_response("POST", "/autonomy/envelopes.json", body, &kernel)
            .expect("route matched")
            .expect("handler ran");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains("ENVELOPE_RECORDED"));
        assert!(
            response
                .body_text()
                .contains("\"self_delivery_scope\":\"self_open_pr\"")
        );
        // The envelope is now an ACTIVE authority (the blocker clears) carrying
        // its self-delivery scope on the receipt.
        let guard = kernel.read().expect("kernel");
        assert_eq!(
            guard.autonomy_envelope_authority_blocker("env_live_test"),
            None,
            "a freshly recorded envelope is active"
        );
        let scope_recorded = guard
            .ledger()
            .query()
            .by_kind("autonomy.envelope.recorded")
            .iter()
            .any(|receipt| {
                receipt.payload.get("envelope_id").map(String::as_str) == Some("env_live_test")
                    && receipt
                        .payload
                        .get("self_delivery_scope")
                        .map(String::as_str)
                        == Some("self_open_pr")
            });
        assert!(scope_recorded, "the scope is on the envelope receipt");
    }

    #[test]
    fn refuses_without_an_envelope_id_or_path_scope() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let missing_id = route_response("POST", "/autonomy/envelopes.json", "{}", &kernel)
            .unwrap()
            .unwrap();
        assert!(missing_id.body_text().contains("name the envelope_id"));
        let missing_scope = route_response(
            "POST",
            "/autonomy/envelopes.json",
            r#"{"envelope_id":"e1"}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        assert!(missing_scope.body_text().contains("allowed_path_scope"));
    }

    #[test]
    fn revokes_recorded_envelope_and_blocks_it() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let record_body = r#"{"envelope_id":"env_to_revoke","owner_id":"human:owner","self_delivery_scope":"self_open_pr","allowed_path_scopes":["docs/"],"expires_at":"2030-01-01T00:00:00Z"}"#;
        let _ = route_response("POST", "/autonomy/envelopes.json", record_body, &kernel)
            .unwrap()
            .unwrap();
        let revoke_body = r#"{"envelope_id":"env_to_revoke","reason":"test revoke"}"#;
        let resp = route_response(
            "POST",
            "/autonomy/envelopes/revocations.json",
            revoke_body,
            &kernel,
        )
        .expect("route matched")
        .expect("handler ran");
        assert!(resp.body_text().contains("ENVELOPE_REVOKED"));
        let guard = kernel.read().expect("kernel");
        assert!(
            guard
                .autonomy_envelope_authority_blocker("env_to_revoke")
                .is_some()
        );
    }

    #[test]
    fn refuses_unknown_envelope_id_on_revoke() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let body = r#"{"envelope_id":"no_such_env"}"#;
        let resp = route_response(
            "POST",
            "/autonomy/envelopes/revocations.json",
            body,
            &kernel,
        )
        .unwrap()
        .unwrap();
        assert!(resp.body_text().contains("REFUSED"));
    }

    #[test]
    fn refuses_empty_envelope_id_on_revoke() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let body = r#"{"envelope_id":""}"#;
        let resp = route_response(
            "POST",
            "/autonomy/envelopes/revocations.json",
            body,
            &kernel,
        )
        .unwrap()
        .unwrap();
        assert!(resp.body_text().contains("name the envelope_id"));
    }
}
