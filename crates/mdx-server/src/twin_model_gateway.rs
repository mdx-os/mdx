use crate::{RouteResponse, reject_unless_method};
use mdx_core::{
    MdxKernel, StorageProvider, TwinModelGatewayProviderObservation,
    TwinModelGatewayProviderObservationReport, json_string_literal,
    twin_model_gateway_provider_slots,
};
use std::sync::{Arc, RwLock};

#[path = "twin_runtime_control.rs"]
mod twin_runtime_control;

pub(crate) fn route_request(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if let Some(response) = twin_runtime_control::route_request(method, path, body, kernel) {
        return Some(response);
    }
    Some(match path {
        "/twin/model-gateway-runtime.json" => (|| {
            if let Some(response) = reject_unless_method(method, "GET") {
                Ok(response)
            } else {
                let kernel = kernel
                    .read()
                    .map_err(|_| "kernel lock poisoned".to_string())?;
                Ok(RouteResponse::json(
                    "200 OK",
                    render_twin_model_gateway_runtime_json(&kernel)?,
                ))
            }
        })(),
        "/twin/model-gateway-provider-observations.json" => (|| {
            if let Some(response) = reject_unless_method(method, "POST") {
                Ok(response)
            } else {
                let mut kernel = kernel
                    .write()
                    .map_err(|_| "kernel lock poisoned".to_string())?;
                Ok(RouteResponse::json(
                    "200 OK",
                    render_provider_observation_post_json(body, &mut kernel)?,
                ))
            }
        })(),
        "/twin/model-gateway-provider-observations/projection.json" => (|| {
            if let Some(response) = reject_unless_method(method, "GET") {
                Ok(response)
            } else {
                let kernel = kernel
                    .read()
                    .map_err(|_| "kernel lock poisoned".to_string())?;
                Ok(RouteResponse::json(
                    "200 OK",
                    render_provider_observation_projection_json(&kernel)?,
                ))
            }
        })(),
        _ => return None,
    })
}

pub(crate) fn render_twin_model_gateway_runtime_json<S: StorageProvider>(
    kernel: &MdxKernel<S>,
) -> Result<String, String> {
    let provider = &deterministic_provider_report()?;
    let accepted = accepted_provider_observations(kernel);
    let provider_connected_local_allowed = !accepted.is_empty();
    let selected_live_driver = accepted
        .first()
        .map(|receipt| payload_value(receipt, "adapter"))
        .unwrap_or("none");
    let selected_live_model_id = accepted
        .first()
        .map(|receipt| payload_value(receipt, "model_id"))
        .unwrap_or("none");
    Ok(format!(
        r#"{{"name":"mdx-twin-model-gateway-runtime","mode":"deterministic-local-read-only","status":"OK","route":"/twin/model-gateway-runtime.json","read_only":true,"writes_allowed":false,"execution_allowed":false,"provider_call_allowed":{},"network_allowed":{},"secrets_available":false,"live_substrate_required":{},"production_write_allowed":false,"selected_driver":{{"model_gateway_driver":"local_model_gateway","model_gateway_provider":"DeterministicModelGateway","model_gateway_model_id":{},"model_gateway_routing":"single_deterministic_stub","model_gateway_inference_id":"local_model_gateway_runtime_projection","model_gateway_stream_contract":"normalized_model_stream_v1","model_gateway_stream_event_count":3,"model_gateway_terminal_event":"model_stream_completed","model_gateway_fallback_strategy":"local_first_fail_closed","model_gateway_fallback_provider":"none_local_single_provider","model_gateway_first_byte_latency_ms":1,"model_gateway_failover_slo_ms":3000,"provider_call_allowed":false}},"provider_connected_runtime":{{"status":{},"provider_connected_local_allowed":{},"provider_call_allowed_now":{},"live_network_allowed_now":{},"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false,"accepted_observation_count":{},"selected_live_driver":{},"selected_live_model_id":{},"observation_route":"/twin/model-gateway-provider-observations.json","projection_route":"/twin/model-gateway-provider-observations/projection.json","turn_on_check":"make v2-openai-provider-turn-on","provider_turn_on_drill_route":"/v2/provider-turn-on-drill.json"}},"provider_profile":{{"status":{},"requested_profile_id":{},"requested_mode":{},"selected_profile_id":{},"provider_kind":{},"policy_profile":{},"enterprise_pack":{},"receipt_id":{},"policy_decision_id":{},"live_provider_call_allowed":false}},"vendor_slots":[{}],"turn_on_boundary":{{"status":"PROVIDER_TURN_ON_REQUIRED","provider_call_allowed":false,"network_allowed":false,"secrets_available":false,"live_provider_call_allowed":false,"required_receipt_kinds":["ollama.chat.observed","openai.response.observed","anthropic.message.observed","xai.chat.observed","gemini.generate_content.observed","mistral.chat.observed","tensorzero.inference.observed"],"proof_command":"make live-turn-on-check"}},"authority_boundaries":{{"status":"FAIL_CLOSED","blocked_authorities":["provider_secrets","tool_execution","worker_spawn","production_write"],"action_authority":{{"call_provider_allowed":{},"open_network_allowed":{},"read_provider_secret_allowed":false,"spawn_worker_allowed":false,"write_production_memory_allowed":false}}}},"ui_handoff":{{"target_owner":"claude","target_surface":"apps/mdx Twin module","safe_to_render":true,"should_render_vendor_slots":true,"must_not_claim_live_provider":{},"copy_boundary":{}}},"evidence":{{"status":"AVAILABLE_READ_ONLY","receipt_ids":[{}],"policy_decision_ids":[{}],"source_paths":["docs/HARNESS-MODEL-GATEWAY.md","docs/HARNESS-PROVIDER-GATEWAY.md","docs/HARNESS-POLICY-PROFILES.md","generated/architecture/harness-model-gateway-driver.json","generated/response-schemas/twin-model-gateway-runtime-response.schema.json","/harness/provider-gateway.json","/twin/session-drafts.json","/twin/model-gateway-provider-observations.json","/v2/provider-turn-on-drill.json"]}}}}"#,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        json_optional_string(&provider.model_id),
        json_string_literal(if provider_connected_local_allowed {
            "PROVIDER_CONNECTED_LOCAL_READY"
        } else {
            "LOCAL_DETERMINISTIC_PROVIDER_TURN_ON_REQUIRED"
        }),
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        accepted.len(),
        json_string_literal(selected_live_driver),
        json_string_literal(selected_live_model_id),
        json_string_literal(&provider.status),
        json_string_literal(&provider.requested_profile_id),
        json_string_literal(&provider.requested_mode),
        json_optional_string(&provider.selected_profile_id),
        json_optional_string(&provider.provider_kind),
        json_optional_string(&provider.policy_profile),
        json_optional_string(&provider.enterprise_pack),
        json_string_literal(&provider.receipt_id),
        json_string_literal(&provider.policy_decision_id),
        vendor_slots_json(),
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        !provider_connected_local_allowed,
        json_string_literal(if provider_connected_local_allowed {
            "Twin may use provider-connected local mode because an observed provider receipt is retained; secret values and output text are still not recorded."
        } else {
            "Twin is using the local deterministic model gateway until a provider turn-on receipt is observed."
        }),
        json_string_literal(&provider.receipt_id),
        json_string_literal(&provider.policy_decision_id)
    ))
}

pub(crate) fn render_provider_observation_post_json<S: StorageProvider>(
    body: &str,
    kernel: &mut MdxKernel<S>,
) -> Result<String, String> {
    let tenant_id = string_field(body, "tenant_id").unwrap_or_else(|| "tenant_local".to_string());
    let actor_id = string_field(body, "actor_id").unwrap_or_else(|| "local_user".to_string());
    let provider_id = string_field(body, "provider_id").unwrap_or_else(|| "openai".to_string());
    let adapter =
        string_field(body, "adapter").unwrap_or_else(|| "OpenAIResponsesModelGateway".to_string());
    let receipt_kind = string_field(body, "receipt_kind")
        .unwrap_or_else(|| "openai.response.observed".to_string());
    let approval_receipt_id = string_field(body, "approval_receipt_id")
        .unwrap_or_else(|| "local_provider_turn_on_approval_receipt_pending".to_string());
    let evidence_file = string_field(body, "evidence_file")
        .unwrap_or_else(|| ".mdx-local/provider-turn-on/openai-response-observed.json".to_string());
    let model_id = string_field(body, "model_id")
        .unwrap_or_else(|| "provider_observation_pending".to_string());
    let response_id = string_field(body, "response_id")
        .unwrap_or_else(|| "provider_response_pending".to_string());
    let response_status =
        string_field(body, "response_status").unwrap_or_else(|| "pending".to_string());
    let report = kernel
        .save_twin_model_gateway_provider_observation_local(TwinModelGatewayProviderObservation {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            provider_id: &provider_id,
            adapter: &adapter,
            receipt_kind: &receipt_kind,
            approval_receipt_id: &approval_receipt_id,
            evidence_file: &evidence_file,
            model_id: &model_id,
            response_id: &response_id,
            response_status: &response_status,
            observed: bool_field(body, "observed"),
            provider_call_attempted: bool_field(body, "provider_call_attempted"),
            network_call_attempted: bool_field(body, "network_call_attempted"),
            credential_presence_only: bool_field_default(body, "credential_presence_only", true),
            credential_values_recorded: bool_field(body, "credential_values_recorded"),
            provider_secret_values_recorded: bool_field(body, "provider_secret_values_recorded"),
            requested_secret_values_recorded: bool_field(body, "requested_secret_values_recorded"),
            output_text_recorded: bool_field(body, "output_text_recorded"),
            production_write_allowed: bool_field(body, "production_write_allowed"),
            total_tokens: usize_field(body, "total_tokens"),
        })
        .map_err(|error| error.message())?;
    Ok(render_observation_report_json(&report))
}

pub(crate) fn render_provider_observation_projection_json<S: StorageProvider>(
    kernel: &MdxKernel<S>,
) -> Result<String, String> {
    let observations = observation_receipts(kernel);
    let accepted_count = observations
        .iter()
        .filter(|receipt| payload_value(receipt, "provider_connected_local_allowed") == "true")
        .count();
    let providers = twin_model_gateway_provider_slots()
        .iter()
        .map(|slot| {
            let accepted = observations.iter().find(|receipt| {
                payload_value(receipt, "provider_id") == slot.provider_id
                    && payload_value(receipt, "provider_connected_local_allowed") == "true"
            });
            format!(
                r#"{{"provider_id":{},"adapter":{},"required_receipt_kind":{},"env_key":{},"stream_contract":{},"turn_on_signal":{},"status":{},"env_present":{},"provider_connected_local_allowed":{},"observation_receipt_id":{},"model_id":{},"response_id":{},"credential_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false}}"#,
                json_string_literal(slot.provider_id),
                json_string_literal(slot.adapter),
                json_string_literal(slot.required_receipt_kind),
                json_string_literal(slot.env_key),
                json_string_literal(slot.stream_contract),
                json_string_literal(slot.turn_on_signal),
                json_string_literal(if accepted.is_some() { "OBSERVED" } else { "PENDING_TURN_ON" }),
                env_present(slot.env_key),
                accepted.is_some(),
                json_string_literal(accepted.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(accepted.map(|receipt| payload_value(receipt, "model_id")).unwrap_or("")),
                json_string_literal(accepted.map(|receipt| payload_value(receipt, "response_id")).unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let observation_json = observations
        .iter()
        .map(|receipt| {
            format!(
                r#"{{"observation_receipt_id":{},"provider_id":{},"adapter":{},"receipt_kind":{},"approval_receipt_id":{},"evidence_file":{},"model_id":{},"response_id":{},"response_status":{},"observed":{},"provider_connected_local_allowed":{},"provider_call_allowed_now":{},"live_network_allowed_now":{},"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"denial_reason":{},"terminal_state":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                json_string_literal(payload_value(receipt, "provider_id")),
                json_string_literal(payload_value(receipt, "adapter")),
                json_string_literal(payload_value(receipt, "receipt_kind")),
                json_string_literal(payload_value(receipt, "approval_receipt_id")),
                json_string_literal(payload_value(receipt, "evidence_file")),
                json_string_literal(payload_value(receipt, "model_id")),
                json_string_literal(payload_value(receipt, "response_id")),
                json_string_literal(payload_value(receipt, "response_status")),
                payload_bool(receipt, "observed"),
                payload_bool(receipt, "provider_connected_local_allowed"),
                payload_bool(receipt, "provider_call_allowed_now"),
                payload_bool(receipt, "live_network_allowed_now"),
                json_string_literal(payload_value(receipt, "denial_reason")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        r#"{{"name":"mdx-twin-model-gateway-provider-observation-projection","status":"OK","route":"/twin/model-gateway-provider-observations/projection.json","writes_route":"/twin/model-gateway-provider-observations.json","read_only":true,"provider_slot_count":{},"observation_count":{},"accepted_observation_count":{},"provider_connected_local_allowed":{},"provider_call_allowed_now":{},"live_network_allowed_now":{},"credential_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"providers":[{}],"observations":[{}]}}"#,
        twin_model_gateway_provider_slots().len(),
        observations.len(),
        accepted_count,
        accepted_count > 0,
        accepted_count > 0,
        accepted_count > 0,
        providers,
        observation_json
    ))
}

fn vendor_slots_json() -> String {
    twin_model_gateway_provider_slots()
    .iter()
    .map(|slot| {
        format!(
            r#"{{"driver_id":{},"provider":{},"status":"PENDING-LIVE-RUN","stream_contract":{},"vendor_swappable":true,"live_substrate_required":true,"provider_call_allowed":false,"network_allowed":false,"secrets_available":false,"secret_env_key":{},"env_present":{},"required_turn_on_receipt_kind":{},"turn_on_signal":{},"blocked_by":"provider_turn_on_evidence_missing"}}"#,
            json_string_literal(slot.provider_id),
            json_string_literal(slot.adapter),
            json_string_literal(slot.stream_contract),
            json_string_literal(slot.env_key),
            env_present(slot.env_key),
            json_string_literal(slot.required_receipt_kind),
            json_string_literal(slot.turn_on_signal)
        )
    })
    .collect::<Vec<_>>()
    .join(",")
}

fn json_optional_string(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| json_string_literal(value))
        .unwrap_or_else(|| "null".to_string())
}

fn render_observation_report_json(report: &TwinModelGatewayProviderObservationReport) -> String {
    format!(
        r#"{{"name":"mdx-twin-model-gateway-provider-observation-local-post","status":{},"route":"/twin/model-gateway-provider-observations.json","projection_route":"/twin/model-gateway-provider-observations/projection.json","provider_id":{},"adapter":{},"receipt_kind":{},"approval_receipt_id":{},"evidence_file":{},"model_id":{},"response_id":{},"response_status":{},"observed":{},"accepted":{},"provider_connected_local_allowed":{},"provider_call_allowed_now":{},"live_network_allowed_now":{},"credential_presence_only":{},"credential_values_recorded":{},"provider_secret_values_recorded":{},"requested_secret_values_recorded":{},"output_text_recorded":{},"production_write_allowed":{},"total_tokens":{},"denial_reason":{},"observation_receipt_id":{},"policy_decision_id":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(&report.provider_id),
        json_string_literal(&report.adapter),
        json_string_literal(&report.receipt_kind),
        json_string_literal(&report.approval_receipt_id),
        json_string_literal(&report.evidence_file),
        json_string_literal(&report.model_id),
        json_string_literal(&report.response_id),
        json_string_literal(&report.response_status),
        report.observed,
        report.accepted,
        report.provider_connected_local_allowed,
        report.provider_call_allowed_now,
        report.live_network_allowed_now,
        report.credential_presence_only,
        report.credential_values_recorded,
        report.provider_secret_values_recorded,
        report.requested_secret_values_recorded,
        report.output_text_recorded,
        report.production_write_allowed,
        report.total_tokens,
        json_string_literal(report.denial_reason),
        json_string_literal(&report.observation_receipt_id),
        json_string_literal(&report.policy_decision_id)
    )
}

pub(crate) fn accepted_provider_observations<S: StorageProvider>(
    kernel: &MdxKernel<S>,
) -> Vec<&mdx_core::Receipt> {
    observation_receipts(kernel)
        .into_iter()
        .filter(|receipt| payload_value(receipt, "provider_connected_local_allowed") == "true")
        .collect()
}

pub(crate) fn accepted_provider_observations_for_tenant<'a, S: StorageProvider>(
    kernel: &'a MdxKernel<S>,
    tenant_id: &str,
) -> Vec<&'a mdx_core::Receipt> {
    accepted_provider_observations(kernel)
        .into_iter()
        .filter(|receipt| receipt.tenant_id.as_str() == tenant_id)
        .collect()
}

fn observation_receipts<S: StorageProvider>(kernel: &MdxKernel<S>) -> Vec<&mdx_core::Receipt> {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.model_gateway.provider.observed")
        .collect()
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn payload_bool(receipt: &mdx_core::Receipt, key: &str) -> bool {
    payload_value(receipt, key) == "true"
}

fn env_present(key: &str) -> bool {
    std::env::var(key)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn string_field(body: &str, key: &str) -> Option<String> {
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

fn bool_field(body: &str, key: &str) -> bool {
    bool_field_default(body, key, false)
}

fn bool_field_default(body: &str, key: &str, default: bool) -> bool {
    let Some(after_key) = body.split(&format!("\"{key}\"")).nth(1) else {
        return default;
    };
    let Some(after_colon) = after_key.split_once(':').map(|pair| pair.1.trim_start()) else {
        return default;
    };
    after_colon.starts_with("true")
}

fn usize_field(body: &str, key: &str) -> usize {
    let Some(after_key) = body.split(&format!("\"{key}\"")).nth(1) else {
        return 0;
    };
    let Some(after_colon) = after_key.split_once(':').map(|pair| pair.1.trim_start()) else {
        return 0;
    };
    after_colon
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<usize>()
        .unwrap_or(0)
}

/// The deterministic provider-gateway posture, produced on a scratch
/// kernel. This used to ride the deleted ceremonial harness inspection
/// chain; the Twin gateway only ever consumed the provider stage, so the
/// provider stage is all that runs now.
fn deterministic_provider_report() -> Result<mdx_core::HarnessProviderGatewayReport, String> {
    use mdx_core::{
        HarnessApprovalMode, HarnessBudgetPolicy, HarnessEnterprisePack, HarnessPermissionPolicy,
        HarnessProviderGatewayRequest, HarnessProviderKind, HarnessProviderProfile,
        HarnessRunManifest, HarnessRunMode, HarnessTrustBoundary, LocalHarnessProviderGateway,
    };
    let manifest = HarnessRunManifest {
        run_id: "twin_model_gateway_provider_inspect",
        tenant_id: "tenant_local",
        actor_id: "mdx_server",
        origin_surface: "mdx_server_cli",
        mode: HarnessRunMode::PlanOnly,
        goal_summary: "inspect the deterministic provider gateway posture",
        definition_of_done: "provider profile selection renders without any live provider call",
        input_artifacts: &["docs/HARNESS-PROVIDER-GATEWAY.md"],
        allowed_write_scope: &[],
        blocked_paths: &[".git/", ".env", ".mdx-local/"],
        allowed_tools: &[],
        blocked_tools: &["shell", "git", "patch", "browser", "mcp", "network"],
        default_parallelism: 1,
        allowed_model_profiles: &["deterministic_stub"],
        provider_fail_closed: true,
        allowed_context_sources: &["manifest", "plan", "transcript", "receipts"],
        context_redaction_required: true,
        permission_policy: HarnessPermissionPolicy::plan_only_local(),
        budget_policy: HarnessBudgetPolicy {
            max_turns: 1,
            max_tool_calls: 0,
            max_runtime_ms: 250,
            max_context_tokens: 4096,
            max_output_tokens: 2048,
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
        exit_criteria: &["provider_profile_selected"],
        trust_boundary: HarnessTrustBoundary {
            workspace_trusted: true,
            project_local_execution_allowed: false,
            mcp_startup_allowed: false,
            network_allowed: false,
            secrets_available: false,
            approval_receipt_id: Some("human_approved_plan_hash_inspect_001"),
        },
        policy_profile: Some("core"),
        enterprise_pack: Some("none"),
    };
    let profiles = [HarnessProviderProfile {
        profile_id: "deterministic_stub",
        provider_kind: HarnessProviderKind::DeterministicStub,
        model_id: "deterministic_local_v1",
        allowed_modes: &["plan_only"],
        max_context_tokens: 4096,
        max_output_tokens: 1024,
        max_cost_cents: 0,
        data_retention: "local_ephemeral",
        context_redaction_required: true,
        telemetry_redaction_required: true,
        provider_fail_closed: true,
        policy_profile: "core",
        enterprise_pack: "none",
        enabled: true,
    }];
    let packs = [HarnessEnterprisePack {
        enterprise_pack_id: "none",
        allowed_policy_profiles: &["core"],
        provider_profile_allowlist: &["deterministic_stub"],
        max_context_tokens: 4096,
        max_output_tokens: 1024,
        max_cost_cents: 0,
        data_retention: "local_ephemeral",
        context_redaction_required: true,
        telemetry_redaction_required: true,
        approval_receipt_required: false,
        audit_export_allowed: false,
    }];
    let mut kernel = mdx_core::MdxKernel::boot_local();
    LocalHarnessProviderGateway
        .select_profile(
            &mut kernel,
            &manifest,
            &HarnessProviderGatewayRequest {
                requested_profile_id: "deterministic_stub",
                requested_mode: "plan_only",
                profiles: &profiles,
                enterprise_packs: &packs,
            },
        )
        .map_err(|error| error.message())
}

#[cfg(test)]
mod tests {
    use super::render_twin_model_gateway_runtime_json;
    use crate::{JSON_CONTENT_TYPE, route_request, route_request_with_body};
    use mdx_core::MdxKernel;
    use std::sync::{Arc, RwLock};

    #[test]
    fn twin_model_gateway_runtime_route_exposes_local_driver_and_vendor_boundaries() {
        let kernel = MdxKernel::boot_local();
        let body =
            render_twin_model_gateway_runtime_json(&kernel).expect("twin model gateway runtime");
        for expected in [
            "\"name\":\"mdx-twin-model-gateway-runtime\"",
            "\"route\":\"/twin/model-gateway-runtime.json\"",
            "\"provider_connected_local_allowed\":false",
            "\"model_gateway_driver\":\"local_model_gateway\"",
            "\"model_gateway_provider\":\"DeterministicModelGateway\"",
            "\"model_gateway_model_id\":\"deterministic_local_v1\"",
            "\"model_gateway_stream_contract\":\"normalized_model_stream_v1\"",
            "\"model_gateway_failover_slo_ms\":3000",
            "\"provider_call_allowed\":false",
            "\"network_allowed\":false",
            "\"secrets_available\":false",
            "\"driver_id\":\"ollama\"",
            "\"driver_id\":\"openai\"",
            "\"driver_id\":\"anthropic\"",
            "\"driver_id\":\"xai\"",
            "\"driver_id\":\"gemini\"",
            "\"driver_id\":\"mistral\"",
            "\"driver_id\":\"tensorzero\"",
            "\"provider\":\"OpenAIResponsesModelGateway\"",
            "\"observation_route\":\"/twin/model-gateway-provider-observations.json\"",
            "\"source_paths\":[\"docs/HARNESS-MODEL-GATEWAY.md\"",
        ] {
            assert!(body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn twin_model_gateway_provider_observation_routes_record_rejected_and_accepted_states() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let rejected = route_request_with_body(
            "POST",
            "/twin/model-gateway-provider-observations.json",
            "{}",
            &kernel,
        )
        .expect("rejected observation route");
        assert_eq!(rejected.status, "200 OK");
        assert_eq!(rejected.content_type, JSON_CONTENT_TYPE);
        for expected in [
            "\"name\":\"mdx-twin-model-gateway-provider-observation-local-post\"",
            "\"status\":\"TWIN_MODEL_GATEWAY_PROVIDER_OBSERVATION_REJECTED\"",
            "\"provider_connected_local_allowed\":false",
            "\"credential_values_recorded\":false",
            "\"provider_secret_values_recorded\":false",
            "\"output_text_recorded\":false",
        ] {
            assert!(rejected.body.contains(expected), "{expected}");
        }

        let accepted = route_request_with_body(
            "POST",
            "/twin/model-gateway-provider-observations.json",
            r#"{"tenant_id":"tenant_local","actor_id":"local_operator","provider_id":"openai","adapter":"OpenAIResponsesModelGateway","receipt_kind":"openai.response.observed","approval_receipt_id":"human_approval_receipt_000001","evidence_file":".mdx-local/provider-turn-on/openai-response-observed.json","model_id":"gpt-5-mini","response_id":"resp_000001","response_status":"completed","observed":true,"provider_call_attempted":true,"network_call_attempted":true,"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"total_tokens":12}"#,
            &kernel,
        )
        .expect("accepted observation route");
        assert_eq!(accepted.status, "200 OK");
        for expected in [
            "\"status\":\"TWIN_MODEL_GATEWAY_PROVIDER_OBSERVED\"",
            "\"provider_connected_local_allowed\":true",
            "\"provider_call_allowed_now\":true",
            "\"live_network_allowed_now\":true",
            "\"credential_values_recorded\":false",
            "\"output_text_recorded\":false",
        ] {
            assert!(accepted.body.contains(expected), "{expected}");
        }

        let projection = route_request(
            "GET",
            "/twin/model-gateway-provider-observations/projection.json",
            &kernel,
        )
        .expect("observation projection");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-twin-model-gateway-provider-observation-projection\"",
            "\"observation_count\":2",
            "\"accepted_observation_count\":1",
            "\"provider_connected_local_allowed\":true",
            "\"provider_id\":\"openai\"",
            "\"adapter\":\"OpenAIResponsesModelGateway\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }

        let runtime = route_request("GET", "/twin/model-gateway-runtime.json", &kernel)
            .expect("runtime after observation");
        assert_eq!(runtime.status, "200 OK");
        for expected in [
            "\"status\":\"PROVIDER_CONNECTED_LOCAL_READY\"",
            "\"provider_connected_local_allowed\":true",
            "\"selected_live_driver\":\"OpenAIResponsesModelGateway\"",
            "\"selected_live_model_id\":\"gpt-5-mini\"",
            "\"must_not_claim_live_provider\":false",
        ] {
            assert!(runtime.body.contains(expected), "{expected}");
        }
    }
}
