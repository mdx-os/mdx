use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_turn_loop_001";
const DEFAULT_RUN_ID: &str = "dxr_run_turn_loop_001";
const DEFAULT_WORKER_RUN_ID: &str = "dxr_worker_run_turn_loop_001";
const DEFAULT_TURN_COUNT: usize = 3;
const DEFAULT_MODEL_TURN_COUNT: usize = 2;
const DEFAULT_TOOL_DENIED_COUNT: usize = 3;
const DEFAULT_TOOL_ESCALATED_COUNT: usize = 1;
const DEFAULT_HARNESS_VERDICT_COUNT: usize = 5;
const DEFAULT_EVIDENCE_RECORD_COUNT: usize = 12;
const DEFAULT_MAX_TURNS: usize = 12;
const DEFAULT_MAX_RUNTIME_MS: usize = 300_000;
const DEFAULT_OBSERVED_RUNTIME_MS: usize = 126_000;
const DEFAULT_TOKENS_IN: usize = 6_400;
const DEFAULT_TOKENS_OUT: usize = 2_900;
const DEFAULT_LATENCY_P95_MS: usize = 3_400;
const DEFAULT_MAX_LATENCY_P95_MS: usize = 5_000;

#[derive(Default)]
pub struct DxrWorkerTurnLoopRuntime {
    runs: Vec<DxrWorkerTurnLoopRun>,
    next_run: usize,
}

pub struct DxrWorkerTurnLoopResult {
    pub body: String,
    pub events: Vec<DxrWorkerTurnLoopRuntimeEvent>,
}

pub struct DxrWorkerTurnLoopRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrWorkerTurnLoopRun {
    sequence: usize,
    turn_loop_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    ctx_context_input_id: String,
    ctx_assembly_receipt_id: String,
    execution_supervision_id: String,
    worker_execution_proof_id: String,
    tool_policy_receipt_id: String,
    multi_judge_proof_id: String,
    provider_failover_proof_id: String,
    turn_count: usize,
    model_turn_count: usize,
    tool_denied_count: usize,
    tool_escalated_count: usize,
    harness_verdict_count: usize,
    evidence_record_count: usize,
    max_turns: usize,
    max_runtime_ms: usize,
    observed_runtime_ms: usize,
    tokens_in: usize,
    tokens_out: usize,
    latency_p95_ms: usize,
    max_latency_p95_ms: usize,
    ctx_context_observed: bool,
    execution_supervision_observed: bool,
    worker_execution_envelope_observed: bool,
    model_streaming_observed: bool,
    provider_failover_observed: bool,
    tool_denial_observed: bool,
    tool_escalation_observed: bool,
    harness_verdicts_observed: bool,
    evidence_chain_observed: bool,
    event_relay_observed: bool,
    reviewer_separation_observed: bool,
    dynamic_workflow_checkpoint_observed: bool,
    prompt_hash: String,
    response_hash: String,
    evidence_head_hash: String,
    status: String,
    terminal_state: String,
    loop_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct WorkerTurnLoopRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    ctx_context_input_id: String,
    ctx_assembly_receipt_id: String,
    execution_supervision_id: String,
    worker_execution_proof_id: String,
    tool_policy_receipt_id: String,
    multi_judge_proof_id: String,
    provider_failover_proof_id: String,
    turn_count: usize,
    model_turn_count: usize,
    tool_denied_count: usize,
    tool_escalated_count: usize,
    harness_verdict_count: usize,
    evidence_record_count: usize,
    max_turns: usize,
    max_runtime_ms: usize,
    observed_runtime_ms: usize,
    tokens_in: usize,
    tokens_out: usize,
    latency_p95_ms: usize,
    max_latency_p95_ms: usize,
    ctx_context_observed: bool,
    execution_supervision_observed: bool,
    worker_execution_envelope_observed: bool,
    model_streaming_observed: bool,
    provider_failover_observed: bool,
    tool_denial_observed: bool,
    tool_escalation_observed: bool,
    harness_verdicts_observed: bool,
    evidence_chain_observed: bool,
    event_relay_observed: bool,
    reviewer_separation_observed: bool,
    dynamic_workflow_checkpoint_observed: bool,
    worker_process_start_requested: bool,
    live_worker_execution_allowed: bool,
    provider_call_requested: bool,
    tool_execution_requested: bool,
    shell_execution_requested: bool,
    patch_application_requested: bool,
    git_execution_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    ci_claim_requested: bool,
    deployment_requested: bool,
    production_write_requested: bool,
}

impl DxrWorkerTurnLoopRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrWorkerTurnLoopResult, String> {
        let request = parse_worker_turn_loop_request(body)?;
        self.next_run += 1;

        let authority_requested = request.worker_process_start_requested
            || request.live_worker_execution_allowed
            || request.provider_call_requested
            || request.tool_execution_requested
            || request.shell_execution_requested
            || request.patch_application_requested
            || request.git_execution_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.ci_claim_requested
            || request.deployment_requested
            || request.production_write_requested;
        let missing_evidence = request.idempotency_key.trim().is_empty()
            || request.ctx_context_input_id.trim().is_empty()
            || request.ctx_assembly_receipt_id.trim().is_empty()
            || request.execution_supervision_id.trim().is_empty()
            || request.worker_execution_proof_id.trim().is_empty()
            || request.tool_policy_receipt_id.trim().is_empty()
            || request.multi_judge_proof_id.trim().is_empty()
            || request.provider_failover_proof_id.trim().is_empty()
            || request.turn_count == 0
            || request.turn_count > request.max_turns
            || request.model_turn_count == 0
            || request.tool_denied_count == 0
            || request.harness_verdict_count < 5
            || request.evidence_record_count < 10
            || request.max_runtime_ms == 0
            || request.observed_runtime_ms == 0
            || request.observed_runtime_ms > request.max_runtime_ms
            || request.latency_p95_ms == 0
            || request.latency_p95_ms > request.max_latency_p95_ms
            || !request.ctx_context_observed
            || !request.execution_supervision_observed
            || !request.worker_execution_envelope_observed
            || !request.model_streaming_observed
            || !request.provider_failover_observed
            || !request.tool_denial_observed
            || !request.tool_escalation_observed
            || !request.harness_verdicts_observed
            || !request.evidence_chain_observed
            || !request.event_relay_observed
            || !request.reviewer_separation_observed
            || !request.dynamic_workflow_checkpoint_observed;
        let (status, terminal_state, loop_decision, rejected, rejection_reason) =
            if authority_requested {
                (
                    "DXR_WORKER_TURN_LOOP_REJECTED_SECURITY_BOUNDARY",
                    "DXR_WORKER_TURN_LOOP_REJECTED_SECURITY_BOUNDARY",
                    "rejected_execution_authority_requested",
                    true,
                    "turn_loop_cannot_start_workers_providers_tools_shell_patch_git_network_secrets_filesystem_ci_deployments_or_production_writes",
                )
            } else if missing_evidence {
                (
                    "DXR_WORKER_TURN_LOOP_REJECTED_MISSING_EVIDENCE",
                    "DXR_WORKER_TURN_LOOP_REJECTED_MISSING_EVIDENCE",
                    "rejected_missing_turn_loop_evidence",
                    true,
                    "ctx_supervision_worker_model_tool_harness_evidence_relay_reviewer_or_checkpoint_gate_missing",
                )
            } else {
                (
                    "LIVE-LOCAL-DXR-WORKER-TURN-LOOP-FLOOR",
                    "DXR_WORKER_TURN_LOOP_RECORDED_LOCAL_AUTHORITY_BLOCKED",
                    "turn_loop_recorded_with_evidence_chain_and_authority_blocked",
                    false,
                    "none",
                )
            };

        let prompt_hash = stable_hash(&[
            &request.tenant_id,
            &request.job_id,
            &request.run_id,
            &request.ctx_context_input_id,
            &request.turn_count.to_string(),
            "prompt",
        ]);
        let response_hash = stable_hash(&[
            &request.tenant_id,
            &request.job_id,
            &request.run_id,
            &request.provider_failover_proof_id,
            &request.tokens_out.to_string(),
            "response",
        ]);
        let evidence_head_hash = stable_hash(&[
            &request.tenant_id,
            &request.job_id,
            &request.run_id,
            &request.worker_execution_proof_id,
            &request.tool_policy_receipt_id,
            &request.harness_verdict_count.to_string(),
            &request.evidence_record_count.to_string(),
            terminal_state,
        ]);

        let run = DxrWorkerTurnLoopRun {
            sequence: self.next_run,
            turn_loop_id: format!("dxr_worker_turn_loop_{:06}", self.next_run),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            idempotency_key: request.idempotency_key,
            ctx_context_input_id: request.ctx_context_input_id,
            ctx_assembly_receipt_id: request.ctx_assembly_receipt_id,
            execution_supervision_id: request.execution_supervision_id,
            worker_execution_proof_id: request.worker_execution_proof_id,
            tool_policy_receipt_id: request.tool_policy_receipt_id,
            multi_judge_proof_id: request.multi_judge_proof_id,
            provider_failover_proof_id: request.provider_failover_proof_id,
            turn_count: request.turn_count,
            model_turn_count: request.model_turn_count,
            tool_denied_count: request.tool_denied_count,
            tool_escalated_count: request.tool_escalated_count,
            harness_verdict_count: request.harness_verdict_count,
            evidence_record_count: request.evidence_record_count,
            max_turns: request.max_turns,
            max_runtime_ms: request.max_runtime_ms,
            observed_runtime_ms: request.observed_runtime_ms,
            tokens_in: request.tokens_in,
            tokens_out: request.tokens_out,
            latency_p95_ms: request.latency_p95_ms,
            max_latency_p95_ms: request.max_latency_p95_ms,
            ctx_context_observed: request.ctx_context_observed,
            execution_supervision_observed: request.execution_supervision_observed,
            worker_execution_envelope_observed: request.worker_execution_envelope_observed,
            model_streaming_observed: request.model_streaming_observed,
            provider_failover_observed: request.provider_failover_observed,
            tool_denial_observed: request.tool_denial_observed,
            tool_escalation_observed: request.tool_escalation_observed,
            harness_verdicts_observed: request.harness_verdicts_observed,
            evidence_chain_observed: request.evidence_chain_observed,
            event_relay_observed: request.event_relay_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            dynamic_workflow_checkpoint_observed: request.dynamic_workflow_checkpoint_observed,
            prompt_hash,
            response_hash,
            evidence_head_hash,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            loop_decision: loop_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = worker_turn_loop_events(&run);
        let body = render_worker_turn_loop_response_json(&run);
        self.runs.push(run);
        Ok(DxrWorkerTurnLoopResult { body, events })
    }

    pub fn turn_loops_json(&self) -> String {
        let accepted_count = self.runs.iter().filter(|run| !run.rejected).count();
        let rejected_count = self.runs.len().saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-worker-turn-loops","status":"LIVE-LOCAL-DXR-WORKER-TURN-LOOP-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/worker-turn-loops.json","submit_route":"/v1/dxr/worker-turn-loops","turn_loop_count":{},"accepted_count":{},"rejected_count":{},"v1_stage":"turn_loop","required_event_types":["run_status_changed","model_called","turn_executed","tool_executed","check_passed","evidence_appended"],"required_gates":["ctx_context","execution_supervision","worker_execution_envelope","model_streaming","provider_failover","tool_denial","tool_escalation","harness_verdicts","evidence_chain","event_relay","reviewer_separation","dynamic_workflow_checkpoint"],"max_turns":{},"max_runtime_ms":{},"max_latency_p95_ms":{},"provider_turn_on_required":true,"live_authority_required":true,"worker_process_started":false,"live_worker_execution_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"turn_loops":[{}]}}"#,
            self.runs.len(),
            accepted_count,
            rejected_count,
            self.runs
                .last()
                .map(|run| run.max_turns)
                .unwrap_or(DEFAULT_MAX_TURNS),
            self.runs
                .last()
                .map(|run| run.max_runtime_ms)
                .unwrap_or(DEFAULT_MAX_RUNTIME_MS),
            self.runs
                .last()
                .map(|run| run.max_latency_p95_ms)
                .unwrap_or(DEFAULT_MAX_LATENCY_P95_MS),
            self.runs
                .iter()
                .map(render_worker_turn_loop_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_worker_turn_loop_request(body: &str) -> Result<WorkerTurnLoopRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR worker turn loop json: {error}"))?;
    Ok(WorkerTurnLoopRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        worker_run_id: string_value(&value, "worker_run_id", DEFAULT_WORKER_RUN_ID),
        idempotency_key: string_value(&value, "idempotency_key", ""),
        ctx_context_input_id: string_value(&value, "ctx_context_input_id", ""),
        ctx_assembly_receipt_id: string_value(&value, "ctx_assembly_receipt_id", ""),
        execution_supervision_id: string_value(&value, "execution_supervision_id", ""),
        worker_execution_proof_id: string_value(&value, "worker_execution_proof_id", ""),
        tool_policy_receipt_id: string_value(&value, "tool_policy_receipt_id", ""),
        multi_judge_proof_id: string_value(&value, "multi_judge_proof_id", ""),
        provider_failover_proof_id: string_value(&value, "provider_failover_proof_id", ""),
        turn_count: usize_value(&value, "turn_count", DEFAULT_TURN_COUNT),
        model_turn_count: usize_value(&value, "model_turn_count", DEFAULT_MODEL_TURN_COUNT),
        tool_denied_count: usize_value(&value, "tool_denied_count", DEFAULT_TOOL_DENIED_COUNT),
        tool_escalated_count: usize_value(
            &value,
            "tool_escalated_count",
            DEFAULT_TOOL_ESCALATED_COUNT,
        ),
        harness_verdict_count: usize_value(
            &value,
            "harness_verdict_count",
            DEFAULT_HARNESS_VERDICT_COUNT,
        ),
        evidence_record_count: usize_value(
            &value,
            "evidence_record_count",
            DEFAULT_EVIDENCE_RECORD_COUNT,
        ),
        max_turns: usize_value(&value, "max_turns", DEFAULT_MAX_TURNS),
        max_runtime_ms: usize_value(&value, "max_runtime_ms", DEFAULT_MAX_RUNTIME_MS),
        observed_runtime_ms: usize_value(
            &value,
            "observed_runtime_ms",
            DEFAULT_OBSERVED_RUNTIME_MS,
        ),
        tokens_in: usize_value(&value, "tokens_in", DEFAULT_TOKENS_IN),
        tokens_out: usize_value(&value, "tokens_out", DEFAULT_TOKENS_OUT),
        latency_p95_ms: usize_value(&value, "latency_p95_ms", DEFAULT_LATENCY_P95_MS),
        max_latency_p95_ms: usize_value(&value, "max_latency_p95_ms", DEFAULT_MAX_LATENCY_P95_MS),
        ctx_context_observed: bool_value(&value, "ctx_context_observed", false),
        execution_supervision_observed: bool_value(&value, "execution_supervision_observed", false),
        worker_execution_envelope_observed: bool_value(
            &value,
            "worker_execution_envelope_observed",
            false,
        ),
        model_streaming_observed: bool_value(&value, "model_streaming_observed", false),
        provider_failover_observed: bool_value(&value, "provider_failover_observed", false),
        tool_denial_observed: bool_value(&value, "tool_denial_observed", false),
        tool_escalation_observed: bool_value(&value, "tool_escalation_observed", false),
        harness_verdicts_observed: bool_value(&value, "harness_verdicts_observed", false),
        evidence_chain_observed: bool_value(&value, "evidence_chain_observed", false),
        event_relay_observed: bool_value(&value, "event_relay_observed", false),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", false),
        dynamic_workflow_checkpoint_observed: bool_value(
            &value,
            "dynamic_workflow_checkpoint_observed",
            false,
        ),
        worker_process_start_requested: bool_value(&value, "worker_process_start_requested", false),
        live_worker_execution_allowed: bool_value(&value, "live_worker_execution_allowed", false),
        provider_call_requested: bool_value(&value, "provider_call_requested", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        shell_execution_requested: bool_value(&value, "shell_execution_requested", false),
        patch_application_requested: bool_value(&value, "patch_application_requested", false),
        git_execution_requested: bool_value(&value, "git_execution_requested", false),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        ci_claim_requested: bool_value(&value, "ci_claim_requested", false),
        deployment_requested: bool_value(&value, "deployment_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn worker_turn_loop_events(run: &DxrWorkerTurnLoopRun) -> Vec<DxrWorkerTurnLoopRuntimeEvent> {
    let mut events = vec![worker_turn_loop_event(run, "worker_turn_loop_recorded")];
    if run.rejected {
        events.push(worker_turn_loop_event(run, "worker_turn_loop_rejected"));
    } else {
        events.extend([
            worker_turn_loop_event(run, "run_status_changed"),
            worker_turn_loop_event(run, "model_called"),
            worker_turn_loop_event(run, "turn_executed"),
            worker_turn_loop_event(run, "tool_executed"),
            worker_turn_loop_event(run, "check_passed"),
            worker_turn_loop_event(run, "evidence_appended"),
            worker_turn_loop_event(run, "worker_turn_loop_ctx_bound"),
            worker_turn_loop_event(run, "worker_turn_loop_harness_bound"),
        ]);
    }
    events.push(worker_turn_loop_event(
        run,
        "worker_turn_loop_authority_blocked",
    ));
    events
}

fn worker_turn_loop_event(
    run: &DxrWorkerTurnLoopRun,
    event_type: &str,
) -> DxrWorkerTurnLoopRuntimeEvent {
    DxrWorkerTurnLoopRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: run.tenant_id.clone(),
        job_id: run.job_id.clone(),
        run_id: run.run_id.clone(),
        actor_id: run.actor_id.clone(),
    }
}

fn render_worker_turn_loop_response_json(run: &DxrWorkerTurnLoopRun) -> String {
    format!(
        r#"{{"name":"mdx-dxr-worker-turn-loop","status":{},"runtime":"mdx-dxr-engine","turn_loop":{},"turn_loop_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"terminal_state":{},"loop_decision":{},"idempotency_key":{},"v1_stage":"turn_loop","prompt_hash":{},"response_hash":{},"evidence_head_hash":{},"turn_count":{},"model_turn_count":{},"tool_denied_count":{},"tool_escalated_count":{},"harness_verdict_count":{},"evidence_record_count":{},"max_turns":{},"max_runtime_ms":{},"observed_runtime_ms":{},"tokens_in":{},"tokens_out":{},"latency_p95_ms":{},"max_latency_p95_ms":{},"ctx_context_required":true,"execution_supervision_required":true,"worker_execution_envelope_required":true,"model_streaming_required":true,"provider_failover_required":true,"tool_denial_required":true,"tool_escalation_required":true,"harness_verdicts_required":true,"evidence_chain_required":true,"event_relay_required":true,"reviewer_separation_required":true,"dynamic_workflow_checkpoint_required":true,"rejected":{},"rejection_reason":{},"worker_process_started":false,"live_worker_execution_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&run.status),
        render_worker_turn_loop_json(run),
        json_string_literal(&run.turn_loop_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.run_id),
        json_string_literal(&run.worker_run_id),
        json_string_literal(&run.terminal_state),
        json_string_literal(&run.loop_decision),
        json_string_literal(&run.idempotency_key),
        json_string_literal(&run.prompt_hash),
        json_string_literal(&run.response_hash),
        json_string_literal(&run.evidence_head_hash),
        run.turn_count,
        run.model_turn_count,
        run.tool_denied_count,
        run.tool_escalated_count,
        run.harness_verdict_count,
        run.evidence_record_count,
        run.max_turns,
        run.max_runtime_ms,
        run.observed_runtime_ms,
        run.tokens_in,
        run.tokens_out,
        run.latency_p95_ms,
        run.max_latency_p95_ms,
        run.rejected,
        json_string_literal(&run.rejection_reason)
    )
}

fn render_worker_turn_loop_json(run: &DxrWorkerTurnLoopRun) -> String {
    format!(
        r#"{{"sequence":{},"turn_loop_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"idempotency_key":{},"ctx_context_input_id":{},"ctx_assembly_receipt_id":{},"execution_supervision_id":{},"worker_execution_proof_id":{},"tool_policy_receipt_id":{},"multi_judge_proof_id":{},"provider_failover_proof_id":{},"v1_stage":"turn_loop","prompt_hash":{},"response_hash":{},"evidence_head_hash":{},"turn_count":{},"model_turn_count":{},"tool_denied_count":{},"tool_escalated_count":{},"harness_verdict_count":{},"evidence_record_count":{},"max_turns":{},"max_runtime_ms":{},"observed_runtime_ms":{},"tokens_in":{},"tokens_out":{},"latency_p95_ms":{},"max_latency_p95_ms":{},"ctx_context_observed":{},"execution_supervision_observed":{},"worker_execution_envelope_observed":{},"model_streaming_observed":{},"provider_failover_observed":{},"tool_denial_observed":{},"tool_escalation_observed":{},"harness_verdicts_observed":{},"evidence_chain_observed":{},"event_relay_observed":{},"reviewer_separation_observed":{},"dynamic_workflow_checkpoint_observed":{},"status":{},"terminal_state":{},"loop_decision":{},"rejected":{},"rejection_reason":{},"worker_process_started":false,"live_worker_execution_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        run.sequence,
        json_string_literal(&run.turn_loop_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.run_id),
        json_string_literal(&run.worker_run_id),
        json_string_literal(&run.idempotency_key),
        json_string_literal(&run.ctx_context_input_id),
        json_string_literal(&run.ctx_assembly_receipt_id),
        json_string_literal(&run.execution_supervision_id),
        json_string_literal(&run.worker_execution_proof_id),
        json_string_literal(&run.tool_policy_receipt_id),
        json_string_literal(&run.multi_judge_proof_id),
        json_string_literal(&run.provider_failover_proof_id),
        json_string_literal(&run.prompt_hash),
        json_string_literal(&run.response_hash),
        json_string_literal(&run.evidence_head_hash),
        run.turn_count,
        run.model_turn_count,
        run.tool_denied_count,
        run.tool_escalated_count,
        run.harness_verdict_count,
        run.evidence_record_count,
        run.max_turns,
        run.max_runtime_ms,
        run.observed_runtime_ms,
        run.tokens_in,
        run.tokens_out,
        run.latency_p95_ms,
        run.max_latency_p95_ms,
        run.ctx_context_observed,
        run.execution_supervision_observed,
        run.worker_execution_envelope_observed,
        run.model_streaming_observed,
        run.provider_failover_observed,
        run.tool_denial_observed,
        run.tool_escalation_observed,
        run.harness_verdicts_observed,
        run.evidence_chain_observed,
        run.event_relay_observed,
        run.reviewer_separation_observed,
        run.dynamic_workflow_checkpoint_observed,
        json_string_literal(&run.status),
        json_string_literal(&run.terminal_state),
        json_string_literal(&run.loop_decision),
        run.rejected,
        json_string_literal(&run.rejection_reason)
    )
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn bool_value(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_value(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn stable_hash(parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_turn_loop_records_v1_stage_and_rejects_authority() {
        let mut runtime = DxrWorkerTurnLoopRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"idempotency_key":"turn-loop-001","ctx_context_input_id":"ctx_input_001","ctx_assembly_receipt_id":"ctx_receipt_001","execution_supervision_id":"dxr_execution_supervision_000001","worker_execution_proof_id":"dxr_worker_execution_proof_000001","tool_policy_receipt_id":"dxr_tool_execution_000001","multi_judge_proof_id":"dxr_multi_judge_proof_000001","provider_failover_proof_id":"dxr_provider_failover_proof_000001","ctx_context_observed":true,"execution_supervision_observed":true,"worker_execution_envelope_observed":true,"model_streaming_observed":true,"provider_failover_observed":true,"tool_denial_observed":true,"tool_escalation_observed":true,"harness_verdicts_observed":true,"evidence_chain_observed":true,"event_relay_observed":true,"reviewer_separation_observed":true,"dynamic_workflow_checkpoint_observed":true}"#,
            )
            .expect("accepted turn loop");
        assert!(
            accepted
                .body
                .contains("DXR_WORKER_TURN_LOOP_RECORDED_LOCAL_AUTHORITY_BLOCKED")
        );
        assert!(accepted.body.contains("\"v1_stage\":\"turn_loop\""));
        assert!(accepted.body.contains("\"turn_count\":3"));
        assert!(accepted.body.contains("\"harness_verdict_count\":5"));
        assert!(accepted.body.contains("\"evidence_record_count\":12"));
        assert!(accepted.body.contains("\"worker_process_started\":false"));
        assert!(accepted.body.contains("\"tool_execution_allowed\":false"));
        assert!(
            accepted
                .events
                .iter()
                .any(|event| event.event_type == "turn_executed")
        );

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"turn-loop-002","ctx_context_input_id":"ctx_input_001","ctx_assembly_receipt_id":"ctx_receipt_001","execution_supervision_id":"dxr_execution_supervision_000001","worker_execution_proof_id":"dxr_worker_execution_proof_000001","tool_policy_receipt_id":"dxr_tool_execution_000001","multi_judge_proof_id":"dxr_multi_judge_proof_000001","provider_failover_proof_id":"dxr_provider_failover_proof_000001","ctx_context_observed":true,"execution_supervision_observed":true,"worker_execution_envelope_observed":true,"model_streaming_observed":true,"provider_failover_observed":true,"tool_denial_observed":true,"tool_escalation_observed":true,"harness_verdicts_observed":true,"evidence_chain_observed":true,"event_relay_observed":true,"reviewer_separation_observed":true,"dynamic_workflow_checkpoint_observed":true,"tool_execution_requested":true}"#,
            )
            .expect("rejected turn loop");
        assert!(
            rejected
                .body
                .contains("DXR_WORKER_TURN_LOOP_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(rejected.body.contains("\"provider_calls_allowed\":false"));

        let projection = runtime.turn_loops_json();
        assert!(projection.contains("LIVE-LOCAL-DXR-WORKER-TURN-LOOP-FLOOR"));
        assert!(projection.contains("\"turn_loop_count\":2"));
        assert!(projection.contains("\"accepted_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(projection.contains("\"v1_stage\":\"turn_loop\""));
        assert!(projection.contains("\"production_writes_allowed\":false"));
    }
}
