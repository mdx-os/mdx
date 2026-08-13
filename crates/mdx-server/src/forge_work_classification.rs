use crate::RouteResponse;
use mdx_core::{
    ForgeWorkClassificationRequest, MdxKernel, classify_forge_work,
    forge_work_classification_packets_json, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/forge/work-classifications.json" {
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
            render_work_classification_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/work-classifications/projection.json" {
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
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_work_classification_projection_json(&kernel),
        )));
    }
    None
}

fn render_work_classification_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let classification_id = json_string_field(body, "classification_id")
        .unwrap_or_else(|| "forge_work_classification_local_001".to_string());
    let ask = json_string_field(body, "ask")
        .unwrap_or_else(|| "Shape a long-horizon Forge mission for a meaty build".to_string());
    let repo = json_string_field(body, "repo").unwrap_or_else(|| "MDx".to_string());
    let declared_checks = json_string_field(body, "declared_checks").unwrap_or_default();
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:codex",
        "operator",
    );
    let report = kernel
        .classify_forge_work_local_with_identity(
            ForgeWorkClassificationRequest {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                classification_id: &classification_id,
                ask: &ask,
                repo: &repo,
                declared_checks: &declared_checks,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    let strategy = crate::forge_run_strategy::resolve_strategy(
        &classify_forge_work(&ask, &declared_checks),
        body,
        Some(report.recommended_width),
    );
    let strategy_execution_readiness = if strategy.harness_id == "mdx_native" {
        r#"{"status":"READY","ready":true,"reason":"MDx Native is available through the governed local loop.","setup_required":false,"clearance_route":"","approval_route":"","cost_enforcement":"priced_token_meter"}"#.to_string()
    } else {
        let runtime = if let Some(reason) = external_geometry_blocker(
            &strategy.harness_id,
            strategy.width,
            strategy.execution_shape,
        ) {
            Err(reason)
        } else {
            crate::forge_external_harness_runner::readiness(
                &strategy.harness_id,
                &resolved.tenant_id,
            )
        };
        let requested_builder_slot = json_string_field(body, "builder_slot").unwrap_or_default();
        let pairing = runtime.as_ref().map_err(Clone::clone).and_then(|runtime| {
            if requested_builder_slot.trim().is_empty() {
                return Ok(());
            }
            let endpoint = crate::forge_model_provider_routing::resolve_builder_endpoint(
                kernel,
                &resolved.tenant_id,
                &requested_builder_slot,
            )
            .ok_or_else(|| {
                format!(
                    "Model slot {} is not connected for this tenant.",
                    requested_builder_slot.trim()
                )
            })?;
            if endpoint.provider_family != runtime.provider_family {
                return Err(format!(
                    "{} currently supports {} models, but slot {} resolves to {}. Choose a compatible model or MDx Native for cross-provider execution.",
                    runtime.harness_id,
                    runtime.provider_family,
                    requested_builder_slot.trim(),
                    endpoint.provider_family
                ));
            }
            Ok(())
        });
        let authorization = pairing.and_then(|_| {
            crate::forge_fleet_eval_scoreboard::authorize_forge_external_run(
                body,
                kernel,
                &resolved.tenant_id,
                &strategy.harness_id,
                strategy.max_cost_cents,
                strategy.width,
            )
        });
        match authorization {
            Ok(_) => format!(
                r#"{{"status":"READY","ready":true,"reason":"The selected harness has a binary, credential, execution clearance, bounded live-run approval, and server environment gate.","setup_required":false,"clearance_route":"/forge/machine-league/closed-runner-clearances.json","approval_route":"/forge/fleet-eval-live-run-approvals.json","cost_enforcement":{}}}"#,
                json_string_literal(external_cost_enforcement(&strategy.harness_id)),
            ),
            Err(reason) => format!(
                r#"{{"status":"SETUP_REQUIRED","ready":false,"reason":{},"setup_required":true,"clearance_route":"/forge/machine-league/closed-runner-clearances.json","approval_route":"/forge/fleet-eval-live-run-approvals.json","cost_enforcement":{}}}"#,
                json_string_literal(&reason),
                json_string_literal(external_cost_enforcement(&strategy.harness_id)),
            ),
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-forge-work-classification-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/work-classifications.json","projection_route":"/forge/work-classifications/projection.json","run_route":{},"mission_route":{},"scoreboard_route":{},"dashboard_route":{},"classification_id":{},"classification_receipt_id":{},"policy_decision_id":{},"ask":{},"repo":{},"recommended_shape":{},"recommended_width":{},"recommendation_label":{},"recommendation_reason":{},"next_route":{},"next_gate":{},"adjustable_fields":{},"new_project_supported":{},"task_class":{},"complexity_tier":{},"confidence_pct":{},"rationale":{},"suggested_checks":{},"allowed_write_scope":{},"blocked_paths":{},"mission_recommended":{},"strategy":{},"strategy_execution_readiness":{},"classification_grants_execution_authority":{},"live_provider_calls_allowed":{},"adapter_execution_allowed":{},"production_write_allowed":{},"blocked_reason":{},"live_substrate_required":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.run_route),
        json_string_literal(&report.mission_route),
        json_string_literal(&report.scoreboard_route),
        json_string_literal(&report.dashboard_route),
        json_string_literal(&report.classification_id),
        json_string_literal(&report.classification_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.ask),
        json_string_literal(&report.repo),
        json_string_literal(&report.recommended_shape),
        report.recommended_width,
        json_string_literal(&report.recommendation_label),
        json_string_literal(&report.recommendation_reason),
        json_string_literal(&report.next_route),
        json_string_literal(&report.next_gate),
        json_string_literal(&report.adjustable_fields),
        report.new_project_supported,
        json_string_literal(&report.task_class),
        json_string_literal(&report.complexity_tier),
        report.confidence_pct,
        json_string_literal(&report.rationale),
        json_string_literal(&report.suggested_checks),
        json_string_literal(&report.allowed_write_scope),
        json_string_literal(&report.blocked_paths),
        report.mission_recommended,
        crate::forge_run_strategy::strategy_json(&strategy),
        strategy_execution_readiness,
        report.classification_grants_execution_authority,
        report.live_provider_calls_allowed,
        report.adapter_execution_allowed,
        report.production_write_allowed,
        json_string_literal(report.blocked_reason),
    ))
}

fn external_cost_enforcement(harness_id: &str) -> &'static str {
    if harness_id == "grok_build" {
        "approval_ceiling_plus_turn_and_runtime_bounds_usage_metering_unavailable"
    } else {
        "approval_ceiling_plus_runtime_and_scope_bounds_usage_and_turn_metering_unavailable"
    }
}

fn external_geometry_blocker(
    harness_id: &str,
    width: u32,
    execution_shape: &str,
) -> Option<String> {
    (width > mdx_core::DIRECT_RUN_MAX_WORKERS || execution_shape == "mission").then(|| {
        format!(
            "{} is currently admitted for solo and direct candidate runs up to {} workers. Choose MDx Native for governed fleet or mission execution; Forge will not silently replace a locked harness.",
            if harness_id == "grok_build" {
                "Grok Build"
            } else {
                "Codex CLI"
            },
            mdx_core::DIRECT_RUN_MAX_WORKERS
        )
    })
}

fn render_work_classification_projection_json(kernel: &MdxKernel) -> String {
    let projection = kernel.project_forge_work_classifications();
    format!(
        r#"{{"name":"mdx-forge-work-classification-local-projection","status":{},"route":"/forge/work-classifications/projection.json","writes_route":"/forge/work-classifications.json","run_route":"/forge/runs.json","mission_route":"/forge/long-horizon-missions.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","dashboard_route":"/forge/long-horizon-mission-dashboard.json","classification_count":{},"mission_recommended_count":{},"run_recommended_count":{},"live_provider_calls_allowed":{},"adapter_execution_allowed":{},"production_write_allowed":{},"class_vocabulary":["bug_fix","feature","refactor","security","ci_repair","docs_code","multi_file","architecture","api_compat","migration","performance","concurrency","observability","product_ux","long_horizon"],"complexity_tiers":["small","medium","large","xl","extreme"],"classifications":[{}]}}"#,
        json_string_literal(projection.status),
        projection.classification_count,
        projection.mission_recommended_count,
        projection.run_recommended_count,
        projection.live_provider_calls_allowed,
        projection.adapter_execution_allowed,
        projection.production_write_allowed,
        forge_work_classification_packets_json(&projection),
    )
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get(key)?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_classification_returns_strategy_and_native_readiness() {
        let mut kernel = MdxKernel::boot_local();
        let body = render_work_classification_json(
            r#"{"ask":"Fix a small Forge label","repo":"MDx","declared_checks":"cargo test -p mdx-server","strategy_mode":"auto","actor_id":"human:test"}"#,
            &mut kernel,
        )
        .expect("classification");
        let packet: serde_json::Value = serde_json::from_str(&body).expect("response json");

        assert_eq!(packet["strategy"]["version"], "forge_run_strategy_v1");
        assert_eq!(packet["strategy"]["harness_id"], "mdx_native");
        assert_eq!(packet["strategy"]["grants_execution_authority"], false);
        assert_eq!(packet["strategy_execution_readiness"]["status"], "READY");
        assert_eq!(packet["strategy_execution_readiness"]["ready"], true);
    }

    #[test]
    fn classification_decodes_multiline_checks_and_quoted_intent() {
        let mut kernel = MdxKernel::boot_local();
        let body = render_work_classification_json(
            r#"{"ask":"Cover the \"empty string\" case","repo":"classnames","declared_checks":"npm ci\nnpm test","strategy_mode":"auto","actor_id":"human:test"}"#,
            &mut kernel,
        )
        .expect("classification");
        let packet: serde_json::Value = serde_json::from_str(&body).expect("response json");

        assert_eq!(packet["ask"], "Cover the \"empty string\" case");
        assert_eq!(packet["suggested_checks"], "npm ci\nnpm test");
    }

    #[test]
    fn external_cost_disclosure_distinguishes_grok_turn_enforcement() {
        assert!(external_cost_enforcement("grok_build").contains("turn_and_runtime_bounds"));
        assert!(
            external_cost_enforcement("codex_cli").contains("usage_and_turn_metering_unavailable")
        );
    }

    #[test]
    fn locked_external_harness_never_silently_falls_back_for_wide_work() {
        let reason = external_geometry_blocker("grok_build", 8, "fleet")
            .expect("wide external strategy is blocked honestly");
        assert!(reason.contains("Choose MDx Native"));
        assert!(reason.contains("will not silently replace"));
        assert!(external_geometry_blocker("grok_build", 4, "candidates").is_none());
    }
}
