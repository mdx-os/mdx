use mdx_core::{
    ActorId, AdaptivePolicyEvidence, CorrelationIds, LoopId, MdxKernel, ModelPolicyContext,
    ModelRouteCandidate, ModelRouteRequest, RoutePolicy, TenantId, TraceId, WorkflowId,
    resolve_model_route,
};
use std::collections::{BTreeMap, BTreeSet};

const MIN_EVALUATED_OUTCOMES: usize = 20;

pub(crate) struct AdaptiveEvaluationReport {
    pub evidence: AdaptivePolicyEvidence,
    pub next_state: String,
    pub recommended_transition: String,
    pub receipt_id: String,
}

pub(crate) struct ShadowComparisonContext<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub baseline_receipt_id: &'a str,
    pub baseline_deployment_id: &'a str,
    pub workload_id: &'a str,
    pub app_id: &'a str,
    pub environment: &'a str,
    pub session_id: Option<&'a str>,
}

pub(crate) fn evaluate(
    kernel: &mut MdxKernel,
    tenant_id: &str,
    actor_id: &str,
    policy_id: &str,
    policy_version: &str,
) -> Result<AdaptiveEvaluationReport, String> {
    let registry = crate::model_fabric_registry::global();
    let policy = registry.policy_version_for_tenant(tenant_id, policy_id, policy_version)?;
    if policy.lifecycle != "adaptive" {
        return Err(
            "only adaptive policies enter replay, shadow, and canary evaluation".to_string(),
        );
    }
    record_missing_replays(kernel, tenant_id, actor_id, &policy)?;
    let replay_cases = count_policy_receipts(
        kernel,
        tenant_id,
        mdx_core::MODEL_ADAPTIVE_REPLAY_RECEIPT_KIND,
        policy_id,
        policy_version,
    );
    let shadow_decisions = count_policy_receipts(
        kernel,
        tenant_id,
        mdx_core::MODEL_ADAPTIVE_SHADOW_RECEIPT_KIND,
        policy_id,
        policy_version,
    );
    let canary_decisions = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && receipt.kind == mdx_core::MODEL_ROUTE_SELECTED_RECEIPT_KIND
                && payload(receipt, "policy_id") == policy_id
                && payload(receipt, "policy_version") == policy_version
        })
        .count() as u64;
    let guardrails = derive_guardrails(kernel, tenant_id, &policy);
    let evidence = AdaptivePolicyEvidence {
        policy_id: policy_id.to_string(),
        policy_version: policy_version.to_string(),
        state: policy.state.clone(),
        replay_cases,
        shadow_decisions,
        canary_decisions,
        guardrail_evidence_sufficient: guardrails.sufficient,
        quality_guardrail_passed: guardrails.quality,
        safety_guardrail_passed: guardrails.safety,
        latency_guardrail_passed: guardrails.latency,
        cost_guardrail_passed: guardrails.cost,
    };
    let correlation = correlation(kernel, tenant_id, actor_id);
    let receipt = kernel
        .record_adaptive_policy_evaluation(&correlation, &evidence)
        .map_err(str::to_string)?;
    let next_state = payload(&receipt, "state").to_string();
    let recommended_transition = payload(&receipt, "recommended_transition").to_string();
    if next_state != evidence.state {
        registry.transition_policy(tenant_id, policy_id, policy_version, &next_state)?;
    }
    Ok(AdaptiveEvaluationReport {
        evidence,
        next_state,
        recommended_transition,
        receipt_id: receipt.receipt_id,
    })
}

pub(crate) fn approve_promotion(
    kernel: &mut MdxKernel,
    tenant_id: &str,
    actor_id: &str,
    policy_id: &str,
    policy_version: &str,
) -> Result<AdaptiveEvaluationReport, String> {
    let report = evaluate(kernel, tenant_id, actor_id, policy_id, policy_version)?;
    if report.evidence.state != "canary" || report.recommended_transition != "promoted" {
        return Err("adaptive policy has not satisfied canary promotion evidence".to_string());
    }
    let correlation = correlation(kernel, tenant_id, actor_id);
    let receipt = kernel
        .record_model_adaptive_policy_promoted(
            &correlation,
            policy_id,
            policy_version,
            &report.receipt_id,
        )
        .map_err(str::to_string)?;
    crate::model_fabric_registry::global().transition_policy(
        tenant_id,
        policy_id,
        policy_version,
        "promoted",
    )?;
    Ok(AdaptiveEvaluationReport {
        evidence: report.evidence,
        next_state: "promoted".to_string(),
        recommended_transition: "promoted".to_string(),
        receipt_id: receipt.receipt_id,
    })
}

pub(crate) fn monitor_evaluated_outcome(
    kernel: &mut MdxKernel,
    tenant_id: &str,
    actor_id: &str,
    outcome_receipt_id: &str,
    outcome: &mdx_core::ModelOutcome,
) -> Result<Option<String>, String> {
    let Some(decision) = kernel.ledger().entries().iter().find(|receipt| {
        receipt.tenant_id.as_str() == tenant_id
            && receipt.receipt_id == outcome.decision_id
            && receipt.kind == mdx_core::MODEL_ROUTE_SELECTED_RECEIPT_KIND
    }) else {
        return Ok(None);
    };
    let policy_id = payload(decision, "policy_id").to_string();
    let policy_version = payload(decision, "policy_version").to_string();
    if policy_id.is_empty() || policy_id == "core" || policy_version.is_empty() {
        return Ok(None);
    }
    let registry = crate::model_fabric_registry::global();
    let policy = registry.policy_version_for_tenant(tenant_id, &policy_id, &policy_version)?;
    if policy.state != "canary" {
        return Ok(None);
    }
    if outcome.safety_status == "failed" {
        let correlation = correlation(kernel, tenant_id, actor_id);
        let receipt = kernel
            .record_model_adaptive_policy_auto_rollback(
                &correlation,
                &policy_id,
                &policy_version,
                outcome_receipt_id,
                "evaluated canary outcome failed the safety guardrail",
            )
            .map_err(str::to_string)?;
        registry.transition_policy(tenant_id, &policy_id, &policy_version, "rollback_required")?;
        return Ok(Some(receipt.receipt_id));
    }
    let evaluated_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && receipt.kind == mdx_core::MODEL_OUTCOME_RECORDED_RECEIPT_KIND
                && payload(receipt, "deployment_id") == outcome.deployment_id
                && (payload(receipt, "quality_score").parse::<u64>().is_ok()
                    || payload(receipt, "safety_status") != "not_evaluated"
                    || payload(receipt, "task_status") != "not_evaluated")
        })
        .count();
    if evaluated_count < MIN_EVALUATED_OUTCOMES {
        return Ok(None);
    }
    let report = evaluate(kernel, tenant_id, actor_id, &policy_id, &policy_version)?;
    Ok((report.next_state == "rollback_required").then_some(report.receipt_id))
}

fn record_missing_replays(
    kernel: &mut MdxKernel,
    tenant_id: &str,
    actor_id: &str,
    policy: &RoutePolicy,
) -> Result<(), String> {
    let prior = kernel.ledger().entries().to_vec();
    let already = prior
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && receipt.kind == mdx_core::MODEL_ADAPTIVE_REPLAY_RECEIPT_KIND
                && payload(receipt, "policy_id") == policy.policy_id
                && payload(receipt, "policy_version") == policy.policy_version
        })
        .map(|receipt| payload(receipt, "baseline_decision_id").to_string())
        .collect::<BTreeSet<_>>();
    for baseline in prior.iter().filter(|receipt| {
        receipt.tenant_id.as_str() == tenant_id
            && receipt.kind == mdx_core::MODEL_ROUTE_SELECTED_RECEIPT_KIND
            && !already.contains(&receipt.receipt_id)
    }) {
        let workload_id = payload(baseline, "workload_id");
        if !policy.workload_ids.is_empty()
            && !policy.workload_ids.iter().any(|id| id == workload_id)
        {
            continue;
        }
        let decision = resolve_candidate(
            tenant_id,
            policy,
            workload_id,
            payload(baseline, "app_id"),
            payload(baseline, "environment"),
            nonempty(payload(baseline, "session_id")),
        )?;
        let correlation = correlation(kernel, tenant_id, actor_id);
        kernel
            .record_model_adaptive_comparison(
                &correlation,
                mdx_core::MODEL_ADAPTIVE_REPLAY_RECEIPT_KIND,
                policy,
                &baseline.receipt_id,
                payload(baseline, "selected_deployment_id"),
                &decision,
            )
            .map_err(str::to_string)?;
    }
    Ok(())
}

pub(crate) fn record_shadow_comparisons(
    kernel: &mut MdxKernel,
    context: &ShadowComparisonContext<'_>,
) -> Result<(), String> {
    let policies = crate::model_fabric_registry::global()
        .snapshot_for_tenant(context.tenant_id)?
        .policies;
    for policy in policies.iter().filter(|policy| {
        policy.lifecycle == "adaptive"
            && policy.state == "shadow"
            && (policy.workload_ids.is_empty()
                || policy
                    .workload_ids
                    .iter()
                    .any(|id| id == context.workload_id))
    }) {
        let decision = resolve_candidate(
            context.tenant_id,
            policy,
            context.workload_id,
            context.app_id,
            context.environment,
            context.session_id,
        )?;
        let correlation = correlation(kernel, context.tenant_id, context.actor_id);
        kernel
            .record_model_adaptive_comparison(
                &correlation,
                mdx_core::MODEL_ADAPTIVE_SHADOW_RECEIPT_KIND,
                policy,
                context.baseline_receipt_id,
                context.baseline_deployment_id,
                &decision,
            )
            .map_err(str::to_string)?;
    }
    Ok(())
}

fn resolve_candidate(
    tenant_id: &str,
    policy: &RoutePolicy,
    workload_id: &str,
    app_id: &str,
    environment: &str,
    session_id: Option<&str>,
) -> Result<mdx_core::ModelRouteDecision, String> {
    let owned = crate::model_fabric_registry::global().candidates_for_tenant(tenant_id)?;
    let rows = owned
        .iter()
        .map(|row| ModelRouteCandidate {
            deployment_id: &row.deployment_id,
            connection_id: &row.connection_id,
            tenant_id: &row.tenant_id,
            provider_id: &row.provider_id,
            model_id: &row.model_id,
            privacy_class: &row.privacy_class,
            region: &row.region,
            residency: &row.residency,
            data_retention: &row.data_retention,
            training_policy: &row.training_policy,
            connected: row.connected,
            healthy: row.healthy,
            live_call_allowed: row.live_call_allowed,
            tools: row.tools,
            structured_output: row.structured_output,
            vision: row.vision,
            context_tokens: row.context_tokens,
            input_cost_microusd_per_million: row.input_cost_microusd_per_million,
            output_cost_microusd_per_million: row.output_cost_microusd_per_million,
            price_known: row.price_known,
            p95_latency_ms: row.p95_latency_ms,
            latency_known: row.latency_known,
            quality_score: row.quality_score,
            quality_known: row.quality_known,
        })
        .collect::<Vec<_>>();
    Ok(resolve_model_route(&ModelRouteRequest {
        workload_id,
        preset: "",
        policy,
        expected_policy_version: Some(&policy.policy_version),
        context: ModelPolicyContext {
            tenant_id,
            app_id,
            environment,
            required_region: None,
            required_residency: None,
            session_id,
            prior_deployment_id: None,
        },
        required_context_tokens: 0,
        max_input_cost_microusd_per_million: 0,
        max_output_cost_microusd_per_million: 0,
        pinned_deployment_id: None,
        candidates: &rows,
    }))
}

#[derive(Default)]
struct Guardrails {
    sufficient: bool,
    quality: bool,
    safety: bool,
    latency: bool,
    cost: bool,
}

#[derive(Default)]
struct Metrics {
    latency: Vec<u64>,
    cost: Vec<u64>,
    quality: Vec<u64>,
    safety: Vec<bool>,
    task: Vec<bool>,
}

fn derive_guardrails(kernel: &MdxKernel, tenant_id: &str, policy: &RoutePolicy) -> Guardrails {
    let comparisons = kernel.ledger().entries().iter().filter(|receipt| {
        receipt.tenant_id.as_str() == tenant_id
            && matches!(
                receipt.kind.as_str(),
                mdx_core::MODEL_ADAPTIVE_REPLAY_RECEIPT_KIND
                    | mdx_core::MODEL_ADAPTIVE_SHADOW_RECEIPT_KIND
            )
            && payload(receipt, "policy_id") == policy.policy_id.as_str()
            && payload(receipt, "policy_version") == policy.policy_version.as_str()
            && payload(receipt, "candidate_status") == "ROUTE_SELECTED"
    });
    let mut baseline_ids = BTreeSet::new();
    let mut candidate_ids = BTreeSet::new();
    for comparison in comparisons {
        baseline_ids.insert(payload(comparison, "baseline_deployment_id").to_string());
        candidate_ids.insert(payload(comparison, "candidate_deployment_id").to_string());
    }
    let mut by_deployment: BTreeMap<String, Metrics> = BTreeMap::new();
    for receipt in kernel.ledger().entries().iter().filter(|receipt| {
        receipt.tenant_id.as_str() == tenant_id
            && receipt.kind == mdx_core::MODEL_OUTCOME_RECORDED_RECEIPT_KIND
            && (policy.workload_ids.is_empty()
                || policy
                    .workload_ids
                    .iter()
                    .any(|workload_id| workload_id == payload(receipt, "workload_id")))
    }) {
        let metrics = by_deployment
            .entry(payload(receipt, "deployment_id").to_string())
            .or_default();
        let quality = payload(receipt, "quality_score").parse::<u64>().ok();
        let evaluated = quality.is_some()
            || payload(receipt, "safety_status") != "not_evaluated"
            || payload(receipt, "task_status") != "not_evaluated";
        if !evaluated {
            continue;
        }
        metrics
            .latency
            .push(parse_u64(payload(receipt, "latency_ms")));
        if let Ok(cost) = payload(receipt, "cost_microusd").parse::<u64>() {
            metrics.cost.push(cost);
        }
        if let Some(score) = quality {
            metrics.quality.push(score);
        }
        if payload(receipt, "safety_status") != "not_evaluated" {
            metrics
                .safety
                .push(payload(receipt, "safety_status") == "passed");
        }
        if payload(receipt, "task_status") != "not_evaluated" {
            metrics
                .task
                .push(payload(receipt, "task_status") == "succeeded");
        }
    }
    let baseline = combine(&by_deployment, &baseline_ids);
    let candidate = combine(&by_deployment, &candidate_ids);
    let sufficient = candidate.quality.len() >= MIN_EVALUATED_OUTCOMES
        && candidate.safety.len() >= MIN_EVALUATED_OUTCOMES
        && candidate.task.len() >= MIN_EVALUATED_OUTCOMES
        && candidate.latency.len() >= MIN_EVALUATED_OUTCOMES
        && candidate.cost.len() >= MIN_EVALUATED_OUTCOMES
        && !baseline.quality.is_empty()
        && !baseline.latency.is_empty()
        && !baseline.cost.is_empty();
    if !sufficient {
        return Guardrails::default();
    }
    Guardrails {
        sufficient: true,
        quality: average(&candidate.quality) * 100 >= average(&baseline.quality) * 98,
        safety: candidate.safety.iter().all(|passed| *passed)
            && pass_rate(&candidate.task) >= pass_rate(&baseline.task),
        latency: average(&candidate.latency) * 100 <= average(&baseline.latency) * 120,
        cost: average(&candidate.cost) * 100 <= average(&baseline.cost) * 110,
    }
}

fn combine(by_deployment: &BTreeMap<String, Metrics>, ids: &BTreeSet<String>) -> Metrics {
    let mut combined = Metrics::default();
    for metrics in ids.iter().filter_map(|id| by_deployment.get(id)) {
        combined.latency.extend(&metrics.latency);
        combined.cost.extend(&metrics.cost);
        combined.quality.extend(&metrics.quality);
        combined.safety.extend(&metrics.safety);
        combined.task.extend(&metrics.task);
    }
    combined
}

fn average(values: &[u64]) -> u128 {
    values.iter().map(|value| u128::from(*value)).sum::<u128>() / values.len().max(1) as u128
}

fn pass_rate(values: &[bool]) -> u128 {
    values.iter().filter(|value| **value).count() as u128 * 10_000 / values.len().max(1) as u128
}

fn count_policy_receipts(
    kernel: &MdxKernel,
    tenant_id: &str,
    kind: &str,
    policy_id: &str,
    policy_version: &str,
) -> u64 {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && receipt.kind == kind
                && payload(receipt, "policy_id") == policy_id
                && payload(receipt, "policy_version") == policy_version
        })
        .count() as u64
}

fn correlation(kernel: &mut MdxKernel, tenant_id: &str, actor_id: &str) -> CorrelationIds {
    CorrelationIds {
        tenant_id: TenantId::new(tenant_id),
        trace_id: TraceId::new(kernel.mint_id("trace")),
        actor_id: ActorId::new(actor_id),
        loop_id: LoopId::new("model_fabric"),
        workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
    }
}

fn payload<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn nonempty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn parse_u64(value: &str) -> u64 {
    value.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_policy_holds_without_derived_evaluator_evidence() {
        let tenant_id = "adaptive-hold-tenant";
        let registry = crate::model_fabric_registry::global();
        registry.clear_tenant(tenant_id);
        registry
            .upsert_policy(RoutePolicy {
                policy_id: "forge-default".to_string(),
                policy_version: "v2".to_string(),
                lifecycle: "adaptive".to_string(),
                state: "draft".to_string(),
                canary_percent: 10,
                tenant_id: tenant_id.to_string(),
                workload_ids: vec!["mdx/forge/builder".to_string()],
                allowed_app_ids: vec!["forge".to_string()],
                allowed_environments: Vec::new(),
                allowed_provider_ids: Vec::new(),
                preferred_deployment_ids: Vec::new(),
                denied_provider_ids: Vec::new(),
                denied_deployment_ids: Vec::new(),
                allowed_regions: Vec::new(),
                required_residency: None,
                allow_data_retention: true,
                allow_training: false,
                max_input_cost_microusd_per_million: 0,
                max_output_cost_microusd_per_million: 0,
            })
            .expect("adaptive policy");
        let mut kernel = MdxKernel::boot_local();
        let report = evaluate(&mut kernel, tenant_id, "human:test", "forge-default", "v2")
            .expect("evaluation");
        assert_eq!(report.next_state, "draft");
        assert!(!report.evidence.guardrail_evidence_sufficient);
        assert_eq!(report.evidence.replay_cases, 0);
        registry.clear_tenant(tenant_id);
    }

    #[test]
    fn evaluated_canary_safety_failure_rolls_back_immediately() {
        let tenant_id = "adaptive-safety-tenant";
        let registry = crate::model_fabric_registry::global();
        registry.clear_tenant(tenant_id);
        registry
            .upsert_policy(RoutePolicy {
                policy_id: "twin-canary".to_string(),
                policy_version: "v2".to_string(),
                lifecycle: "adaptive".to_string(),
                state: "canary".to_string(),
                canary_percent: 10,
                tenant_id: tenant_id.to_string(),
                workload_ids: vec!["mdx/twin/conversation".to_string()],
                allowed_app_ids: vec!["twin".to_string()],
                allowed_environments: Vec::new(),
                allowed_provider_ids: Vec::new(),
                preferred_deployment_ids: Vec::new(),
                denied_provider_ids: Vec::new(),
                denied_deployment_ids: Vec::new(),
                allowed_regions: Vec::new(),
                required_residency: None,
                allow_data_retention: true,
                allow_training: false,
                max_input_cost_microusd_per_million: 0,
                max_output_cost_microusd_per_million: 0,
            })
            .expect("canary policy");
        let mut kernel = MdxKernel::boot_local();
        let correlation = correlation(&mut kernel, tenant_id, "human:evaluator");
        let decision = mdx_core::ModelRouteDecision {
            status: "ROUTE_SELECTED",
            workload_id: "mdx/twin/conversation".to_string(),
            app_id: "twin".to_string(),
            environment: "production".to_string(),
            session_id: "session-1".to_string(),
            preset: "balanced".to_string(),
            policy_id: "twin-canary".to_string(),
            policy_version: "v2".to_string(),
            selected_deployment_id: Some("openai/canary".to_string()),
            selected_provider_id: Some("openai".to_string()),
            selected_model_id: Some("openai/test".to_string()),
            selection_reason: "policy_objective",
            session_sticky_applied: false,
            provider_failover_deployment_ids: Vec::new(),
            model_fallback_deployment_ids: Vec::new(),
            fallback_deployment_ids: Vec::new(),
            exclusions: Vec::new(),
            grants_execution_authority: false,
        };
        let route_receipt = kernel.record_model_route_decision(&correlation, &decision);
        let outcome = mdx_core::ModelOutcome {
            decision_id: route_receipt.receipt_id,
            workload_id: decision.workload_id.clone(),
            app_id: decision.app_id.clone(),
            environment: decision.environment.clone(),
            session_id: decision.session_id.clone(),
            deployment_id: "openai/canary".to_string(),
            latency_ms: 100,
            cost_microusd: Some(10),
            quality_score: Some(90),
            safety_status: "failed".to_string(),
            task_status: "succeeded".to_string(),
            correction_status: "not_observed".to_string(),
            provenance: "operator_evaluation:test".to_string(),
        };
        let outcome_receipt = kernel
            .record_model_outcome(&correlation, &outcome)
            .expect("outcome");
        let rollback = monitor_evaluated_outcome(
            &mut kernel,
            tenant_id,
            "human:evaluator",
            &outcome_receipt.receipt_id,
            &outcome,
        )
        .expect("monitor")
        .expect("rollback receipt");
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.receipt_id == rollback
                && receipt.kind == mdx_core::MODEL_ADAPTIVE_POLICY_AUTO_ROLLBACK_RECEIPT_KIND
        }));
        assert_eq!(
            registry
                .policy_version_for_tenant(tenant_id, "twin-canary", "v2")
                .expect("rolled back policy")
                .state,
            "rollback_required"
        );
        registry.clear_tenant(tenant_id);
    }
}
