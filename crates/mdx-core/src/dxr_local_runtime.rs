use super::*;
use std::collections::BTreeMap;

pub const LOCAL_DXR_RUNTIME: &str = "mdx-dxr-engine";
pub const LOCAL_DXR_DRIVER: &str = "local_dxr_runtime";
pub const LOCAL_DXR_STATUS: &str = "LIVE-LOCAL-DXR-RUNTIME";
pub const LOCAL_DXR_MODEL_DRIVER: &str = "DeterministicModelGateway";
pub const LOCAL_DXR_EVENT_BRIDGE: &str = "local_bounded_event_bridge";
pub const LOCAL_DXR_LEDGER_BRIDGE: &str = "local_ledger_attestation_queue";
const DXR_GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrLocalRuntimeRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub intent: &'a str,
    pub workspace: &'a str,
    pub branch: &'a str,
    pub idempotency_key: &'a str,
    pub max_turns: usize,
    pub max_cost_cents: usize,
    pub requested_tools: &'a [&'a str],
    pub quality_gates: &'a [&'a str],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrJobState {
    pub job_id: String,
    pub status: String,
    pub priority: i32,
    pub spec_hash: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrRunState {
    pub run_id: String,
    pub job_id: String,
    pub status: String,
    pub agent_id: String,
    pub turn_count: usize,
    pub parent_run_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrRunClaimState {
    pub claim_id: String,
    pub run_id: String,
    pub worker_id: String,
    pub status: String,
    pub lease_expires_at: String,
    pub heartbeat_status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrRunDependencyState {
    pub dependency_id: String,
    pub parent_run_id: String,
    pub child_run_id: String,
    pub status: String,
    pub wait_condition: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrTurnLogState {
    pub turn_id: String,
    pub run_id: String,
    pub turn_number: usize,
    pub model_id: String,
    pub prompt_hash: String,
    pub response_hash: String,
    pub tokens_in: usize,
    pub tokens_out: usize,
    pub latency_ms: usize,
    pub cost_cents: usize,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrToolExecutionState {
    pub execution_id: String,
    pub run_id: String,
    pub tool_name: String,
    pub status: String,
    pub outcome: String,
    pub duration_ms: usize,
    pub policy_decision: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrHarnessVerdictState {
    pub verdict_id: String,
    pub run_id: String,
    pub check_name: String,
    pub passed: bool,
    pub severity: String,
    pub evidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrEvidenceRecord {
    pub evidence_id: String,
    pub sequence_number: u64,
    pub previous_hash: String,
    pub record_hash: String,
    pub event_type: String,
    pub job_id: Option<String>,
    pub run_id: Option<String>,
    pub agent_id: Option<String>,
    pub payload: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrCtxIntakeReport {
    pub ctx_source_count: usize,
    pub memory_tier_count: usize,
    pub local_memory_input_count: usize,
    pub consolidation_status: String,
    pub handoff_status: String,
    pub signal_status: String,
    pub event_stream_status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrLocalRuntimePacket {
    pub name: String,
    pub status: String,
    pub runtime: String,
    pub route: String,
    pub submit_route: String,
    pub health_route: String,
    pub driver: String,
    pub model_driver: String,
    pub ctx_driver: String,
    pub ctx_runtime: String,
    pub ctx_assembly_receipt_id: String,
    pub ctx_source_count: usize,
    pub ctx_intake: DxrCtxIntakeReport,
    pub event_bridge_driver: String,
    pub ledger_bridge_driver: String,
    pub queue_driver: String,
    pub workspace_driver: String,
    pub harness_driver: String,
    pub provider_streaming_status: String,
    pub websocket_streaming_status: String,
    pub postgres_status: String,
    pub valkey_status: String,
    pub temporal_status: String,
    pub provider_calls_allowed: bool,
    pub tool_execution_allowed: bool,
    pub shell_execution_allowed: bool,
    pub git_execution_allowed: bool,
    pub worker_execution_allowed: bool,
    pub production_writes_allowed: bool,
    pub max_turns: usize,
    pub max_cost_cents: usize,
    pub hot_path_budget_ms: usize,
    pub estimated_queue_p95_ms: usize,
    pub job: DxrJobState,
    pub run: DxrRunState,
    pub claim: DxrRunClaimState,
    pub dependencies: Vec<DxrRunDependencyState>,
    pub turns: Vec<DxrTurnLogState>,
    pub tool_executions: Vec<DxrToolExecutionState>,
    pub harness_verdicts: Vec<DxrHarnessVerdictState>,
    pub evidence_records: Vec<DxrEvidenceRecord>,
    pub chain_valid: bool,
    pub head_hash: String,
    pub blocked_authorities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DxrLocalRuntimeError {
    MissingField(&'static str),
    EmptyList(&'static str),
    InvalidBudget(&'static str),
    ContextAssembly(String),
    EvidenceChain(String),
}

impl DxrLocalRuntimeError {
    pub fn message(&self) -> String {
        match self {
            Self::MissingField(field) => format!("DXR local runtime missing {field}"),
            Self::EmptyList(field) => format!("DXR local runtime list {field} is empty"),
            Self::InvalidBudget(field) => format!("DXR local runtime invalid budget {field}"),
            Self::ContextAssembly(message) => {
                format!("DXR local runtime CTX assembly failed: {message}")
            }
            Self::EvidenceChain(message) => {
                format!("DXR local runtime evidence chain failed: {message}")
            }
        }
    }
}

pub fn assemble_local_dxr_runtime_packet(
    request: DxrLocalRuntimeRequest<'_>,
) -> Result<DxrLocalRuntimePacket, DxrLocalRuntimeError> {
    validate_request(&request)?;

    let mut kernel = MdxKernel::boot_local();
    kernel
        .run_evals_runner_agent()
        .map_err(DxrLocalRuntimeError::ContextAssembly)?;
    let dxr_ctx_memories = [LocalContextMemoryInput {
        memory_id: "dxr_ctx_memory:execution_requires_grounding",
        tier: "procedural",
        content: "DXR execution must consume a CTX assembly receipt before recording workflow, tool, worker, or model turns.",
        source_receipt_id: "dxr_ctx_memory_receipt:execution_requires_grounding",
        importance: 96,
    }];
    let ctx_packet = kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            query: request.intent,
            token_budget: 180,
            source_preferences: &[],
            query_entities: &["forge", "dxr", "workflow"],
            local_memories: &dxr_ctx_memories,
        })
        .map_err(|error| DxrLocalRuntimeError::ContextAssembly(error.message()))?;
    let ctx_intake = DxrCtxIntakeReport {
        ctx_source_count: ctx_packet.included_source_count,
        memory_tier_count: ctx_packet.memory_tiers.len(),
        local_memory_input_count: ctx_packet.local_memory_input_count,
        consolidation_status: ctx_packet.consolidation_report.status.to_string(),
        handoff_status: ctx_packet.handoff_report.status.to_string(),
        signal_status: ctx_packet.signal_report.status.to_string(),
        event_stream_status: ctx_packet.event_stream_report.status.to_string(),
    };

    let job_id = format!(
        "dxr_job_{}",
        stable_token(&[
            request.tenant_id,
            request.actor_id,
            request.intent,
            request.idempotency_key,
        ])
    );
    let run_id = format!("dxr_run_{}", stable_token(&[&job_id, request.branch]));
    let claim_id = format!("dxr_claim_{}", stable_token(&[&run_id, "local-dxr-worker"]));
    let ctx_child_run_id = format!("dxr_run_{}", stable_token(&[&run_id, "ctx"]));
    let harness_child_run_id = format!("dxr_run_{}", stable_token(&[&run_id, "harness"]));
    let spec_hash = stable_hash(&[
        request.intent,
        request.workspace,
        request.branch,
        request.idempotency_key,
    ]);
    let prompt_hash = stable_hash(&[
        request.intent,
        &ctx_packet.assembly_receipt_id,
        &join_static_values(request.requested_tools),
    ]);
    let response_hash = stable_hash(&[
        "local_dxr_runtime_fail_closed",
        request.intent,
        request.actor_id,
    ]);

    let job = DxrJobState {
        job_id: job_id.clone(),
        status: "pending".to_string(),
        priority: 10,
        spec_hash,
        idempotency_key: request.idempotency_key.trim().to_string(),
    };
    let run = DxrRunState {
        run_id: run_id.clone(),
        job_id: job_id.clone(),
        status: "abandoned_before_live_execution".to_string(),
        agent_id: request.actor_id.trim().to_string(),
        turn_count: 1,
        parent_run_id: None,
    };
    let claim = DxrRunClaimState {
        claim_id,
        run_id: run_id.clone(),
        worker_id: "local-dxr-worker".to_string(),
        status: "released".to_string(),
        lease_expires_at: "local-proof-no-live-lease".to_string(),
        heartbeat_status: "local-heartbeat-recorded".to_string(),
    };
    let dependencies = vec![
        DxrRunDependencyState {
            dependency_id: format!("dxr_dep_{}", stable_token(&[&run_id, "ctx"])),
            parent_run_id: run_id.clone(),
            child_run_id: ctx_child_run_id,
            status: "resolved".to_string(),
            wait_condition: "ctx_context_packet_available".to_string(),
        },
        DxrRunDependencyState {
            dependency_id: format!("dxr_dep_{}", stable_token(&[&run_id, "harness"])),
            parent_run_id: run_id.clone(),
            child_run_id: harness_child_run_id,
            status: "resolved".to_string(),
            wait_condition: "harness_verdicts_recorded".to_string(),
        },
    ];
    let turns = vec![DxrTurnLogState {
        turn_id: format!("dxr_turn_{}", stable_token(&[&run_id, "1"])),
        run_id: run_id.clone(),
        turn_number: 1,
        model_id: LOCAL_DXR_MODEL_DRIVER.to_string(),
        prompt_hash,
        response_hash,
        tokens_in: 768,
        tokens_out: 96,
        latency_ms: 7,
        cost_cents: 0,
        status: "provider_denied_local_deterministic_response".to_string(),
    }];
    let tool_executions = request
        .requested_tools
        .iter()
        .enumerate()
        .map(|(index, tool)| DxrToolExecutionState {
            execution_id: format!(
                "dxr_tool_{}",
                stable_token(&[&run_id, tool, &index.to_string()])
            ),
            run_id: run_id.clone(),
            tool_name: tool.trim().to_string(),
            status: "blocked".to_string(),
            outcome: "denied_by_policy".to_string(),
            duration_ms: 1,
            policy_decision: "local_runtime_no_tool_authority".to_string(),
        })
        .collect::<Vec<_>>();
    let harness_verdicts = harness_verdicts(&run_id, &request, &ctx_packet);
    let evidence_records = evidence_chain(
        &request,
        &job_id,
        &run_id,
        &ctx_packet,
        &harness_verdicts,
        &tool_executions,
    );
    verify_dxr_evidence_chain(&evidence_records).map_err(DxrLocalRuntimeError::EvidenceChain)?;
    let head_hash = evidence_records
        .last()
        .map(|record| record.record_hash.clone())
        .unwrap_or_else(|| DXR_GENESIS_HASH.to_string());

    Ok(DxrLocalRuntimePacket {
        name: "mdx-dxr-local-runtime-packet".to_string(),
        status: LOCAL_DXR_STATUS.to_string(),
        runtime: LOCAL_DXR_RUNTIME.to_string(),
        route: "/dxr/local-runtime-packet.json".to_string(),
        submit_route: "/v1/dxr/jobs".to_string(),
        health_route: "/dxr/health".to_string(),
        driver: LOCAL_DXR_DRIVER.to_string(),
        model_driver: LOCAL_DXR_MODEL_DRIVER.to_string(),
        ctx_driver: ctx_packet.ctx_driver.to_string(),
        ctx_runtime: "mdx-ctx-engine".to_string(),
        ctx_assembly_receipt_id: ctx_packet.assembly_receipt_id,
        ctx_source_count: ctx_intake.ctx_source_count,
        ctx_intake,
        event_bridge_driver: LOCAL_DXR_EVENT_BRIDGE.to_string(),
        ledger_bridge_driver: LOCAL_DXR_LEDGER_BRIDGE.to_string(),
        queue_driver: "local_durable_queue_projection".to_string(),
        workspace_driver: "local_workspace_quarantine".to_string(),
        harness_driver: "local_dxr_harness_verdicts".to_string(),
        provider_streaming_status: "PENDING-LIVE-RUN".to_string(),
        websocket_streaming_status: "LOCAL-EVENT-BRIDGE-ONLY".to_string(),
        postgres_status: "LIVE-LOCAL-BOUNDARY".to_string(),
        valkey_status: "LIVE-LOCAL-BOUNDARY".to_string(),
        temporal_status: "PENDING-LIVE-RUN".to_string(),
        provider_calls_allowed: false,
        tool_execution_allowed: false,
        shell_execution_allowed: false,
        git_execution_allowed: false,
        worker_execution_allowed: false,
        production_writes_allowed: false,
        max_turns: request.max_turns,
        max_cost_cents: request.max_cost_cents,
        hot_path_budget_ms: 20,
        estimated_queue_p95_ms: 8,
        job,
        run,
        claim,
        dependencies,
        turns,
        tool_executions,
        harness_verdicts,
        evidence_records,
        chain_valid: true,
        head_hash,
        blocked_authorities: vec![
            "provider_streaming".to_string(),
            "shell_execution".to_string(),
            "git_execution".to_string(),
            "patch_application".to_string(),
            "worker_execution".to_string(),
            "production_writes".to_string(),
        ],
    })
}

pub fn default_local_dxr_runtime_request() -> DxrLocalRuntimeRequest<'static> {
    DxrLocalRuntimeRequest {
        tenant_id: "tenant_local",
        actor_id: "local_forge_operator",
        intent: "forge governed build with context and harness proof",
        workspace: "/workspace/mdx-native",
        branch: "main",
        idempotency_key: "local-dxr-runtime-proof",
        max_turns: 50,
        max_cost_cents: 0,
        requested_tools: &["read_file", "write_patch", "run_tests"],
        quality_gates: &[
            "context_packet_available",
            "tool_authority_denied",
            "evidence_chain_valid",
        ],
    }
}

pub fn render_local_dxr_runtime_packet_json(packet: &DxrLocalRuntimePacket) -> String {
    format!(
        r#"{{"name":{},"status":{},"runtime":{},"route":{},"submit_route":{},"health_route":{},"driver":{},"model_driver":{},"ctx_driver":{},"ctx_runtime":{},"ctx_assembly_receipt_id":{},"ctx_source_count":{},"ctx_intake":{},"event_bridge_driver":{},"ledger_bridge_driver":{},"queue_driver":{},"workspace_driver":{},"harness_driver":{},"provider_streaming_status":{},"websocket_streaming_status":{},"postgres_status":{},"valkey_status":{},"temporal_status":{},"provider_calls_allowed":{},"tool_execution_allowed":{},"shell_execution_allowed":{},"git_execution_allowed":{},"worker_execution_allowed":{},"production_writes_allowed":{},"max_turns":{},"max_cost_cents":{},"hot_path_budget_ms":{},"estimated_queue_p95_ms":{},"job":{},"run":{},"claim":{},"dependencies":[{}],"turns":[{}],"tool_executions":[{}],"harness_verdicts":[{}],"evidence_records":[{}],"evidence_record_count":{},"chain_valid":{},"head_hash":{},"blocked_authorities":[{}]}}"#,
        json_string_literal(&packet.name),
        json_string_literal(&packet.status),
        json_string_literal(&packet.runtime),
        json_string_literal(&packet.route),
        json_string_literal(&packet.submit_route),
        json_string_literal(&packet.health_route),
        json_string_literal(&packet.driver),
        json_string_literal(&packet.model_driver),
        json_string_literal(&packet.ctx_driver),
        json_string_literal(&packet.ctx_runtime),
        json_string_literal(&packet.ctx_assembly_receipt_id),
        packet.ctx_source_count,
        render_ctx_intake_json(&packet.ctx_intake),
        json_string_literal(&packet.event_bridge_driver),
        json_string_literal(&packet.ledger_bridge_driver),
        json_string_literal(&packet.queue_driver),
        json_string_literal(&packet.workspace_driver),
        json_string_literal(&packet.harness_driver),
        json_string_literal(&packet.provider_streaming_status),
        json_string_literal(&packet.websocket_streaming_status),
        json_string_literal(&packet.postgres_status),
        json_string_literal(&packet.valkey_status),
        json_string_literal(&packet.temporal_status),
        packet.provider_calls_allowed,
        packet.tool_execution_allowed,
        packet.shell_execution_allowed,
        packet.git_execution_allowed,
        packet.worker_execution_allowed,
        packet.production_writes_allowed,
        packet.max_turns,
        packet.max_cost_cents,
        packet.hot_path_budget_ms,
        packet.estimated_queue_p95_ms,
        render_job_json(&packet.job),
        render_run_json(&packet.run),
        render_claim_json(&packet.claim),
        packet
            .dependencies
            .iter()
            .map(render_dependency_json)
            .collect::<Vec<_>>()
            .join(","),
        packet
            .turns
            .iter()
            .map(render_turn_json)
            .collect::<Vec<_>>()
            .join(","),
        packet
            .tool_executions
            .iter()
            .map(render_tool_json)
            .collect::<Vec<_>>()
            .join(","),
        packet
            .harness_verdicts
            .iter()
            .map(render_verdict_json)
            .collect::<Vec<_>>()
            .join(","),
        packet
            .evidence_records
            .iter()
            .map(render_evidence_json)
            .collect::<Vec<_>>()
            .join(","),
        packet.evidence_records.len(),
        packet.chain_valid,
        json_string_literal(&packet.head_hash),
        packet
            .blocked_authorities
            .iter()
            .map(|authority| json_string_literal(authority))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn render_ctx_intake_json(report: &DxrCtxIntakeReport) -> String {
    format!(
        r#"{{"ctx_source_count":{},"memory_tier_count":{},"local_memory_input_count":{},"consolidation_status":{},"handoff_status":{},"signal_status":{},"event_stream_status":{}}}"#,
        report.ctx_source_count,
        report.memory_tier_count,
        report.local_memory_input_count,
        json_string_literal(&report.consolidation_status),
        json_string_literal(&report.handoff_status),
        json_string_literal(&report.signal_status),
        json_string_literal(&report.event_stream_status)
    )
}

pub fn verify_dxr_evidence_chain(records: &[DxrEvidenceRecord]) -> Result<(), String> {
    let mut previous_hash = DXR_GENESIS_HASH.to_string();
    for record in records {
        if record.previous_hash != previous_hash {
            return Err(format!("chain break at seq {}", record.sequence_number));
        }
        let expected = compute_dxr_evidence_hash(
            &record.previous_hash,
            record.sequence_number,
            &record.event_type,
            record.job_id.as_deref(),
            record.run_id.as_deref(),
            record.agent_id.as_deref(),
            &record.payload,
        );
        if record.record_hash != expected {
            return Err(format!("hash mismatch at seq {}", record.sequence_number));
        }
        previous_hash = record.record_hash.clone();
    }
    Ok(())
}

fn validate_request(request: &DxrLocalRuntimeRequest<'_>) -> Result<(), DxrLocalRuntimeError> {
    for (field, value) in [
        ("tenant_id", request.tenant_id),
        ("actor_id", request.actor_id),
        ("intent", request.intent),
        ("workspace", request.workspace),
        ("branch", request.branch),
        ("idempotency_key", request.idempotency_key),
    ] {
        if value.trim().is_empty() {
            return Err(DxrLocalRuntimeError::MissingField(field));
        }
    }
    if request.max_turns == 0 {
        return Err(DxrLocalRuntimeError::InvalidBudget("max_turns"));
    }
    if request.requested_tools.is_empty() {
        return Err(DxrLocalRuntimeError::EmptyList("requested_tools"));
    }
    if request.quality_gates.is_empty() {
        return Err(DxrLocalRuntimeError::EmptyList("quality_gates"));
    }
    Ok(())
}

fn harness_verdicts(
    run_id: &str,
    request: &DxrLocalRuntimeRequest<'_>,
    ctx_packet: &LocalContextPacket,
) -> Vec<DxrHarnessVerdictState> {
    vec![
        DxrHarnessVerdictState {
            verdict_id: format!("dxr_verdict_{}", stable_token(&[run_id, "policy_profile"])),
            run_id: run_id.to_string(),
            check_name: "policy_profile_fail_closed".to_string(),
            passed: true,
            severity: "error".to_string(),
            evidence:
                "provider, shell, git, patch, worker, and production write authority remain closed"
                    .to_string(),
        },
        DxrHarnessVerdictState {
            verdict_id: format!("dxr_verdict_{}", stable_token(&[run_id, "context_packet"])),
            run_id: run_id.to_string(),
            check_name: "ctx_packet_consumed".to_string(),
            passed: ctx_packet.included_source_count > 0,
            severity: "error".to_string(),
            evidence: ctx_packet.assembly_receipt_id.clone(),
        },
        DxrHarnessVerdictState {
            verdict_id: format!(
                "dxr_verdict_{}",
                stable_token(&[run_id, "ctx_memory_tiers"])
            ),
            run_id: run_id.to_string(),
            check_name: "ctx_memory_tiers_available".to_string(),
            passed: ctx_packet.memory_tiers.len() == 4 && ctx_packet.local_memory_input_count > 0,
            severity: "error".to_string(),
            evidence: format!(
                "tiers={} local_memory_inputs={}",
                ctx_packet.memory_tiers.len(),
                ctx_packet.local_memory_input_count
            ),
        },
        DxrHarnessVerdictState {
            verdict_id: format!("dxr_verdict_{}", stable_token(&[run_id, "tool_denial"])),
            run_id: run_id.to_string(),
            check_name: "tool_authority_denied".to_string(),
            passed: true,
            severity: "error".to_string(),
            evidence: join_static_values(request.requested_tools),
        },
        DxrHarnessVerdictState {
            verdict_id: format!("dxr_verdict_{}", stable_token(&[run_id, "evidence_chain"])),
            run_id: run_id.to_string(),
            check_name: "evidence_chain_valid".to_string(),
            passed: true,
            severity: "error".to_string(),
            evidence: "DXR hash-chain format compatible with v1 dxr-evidence".to_string(),
        },
        DxrHarnessVerdictState {
            verdict_id: format!("dxr_verdict_{}", stable_token(&[run_id, "quality_gates"])),
            run_id: run_id.to_string(),
            check_name: "quality_gates_recorded".to_string(),
            passed: true,
            severity: "warning".to_string(),
            evidence: join_static_values(request.quality_gates),
        },
    ]
}

fn evidence_chain(
    request: &DxrLocalRuntimeRequest<'_>,
    job_id: &str,
    run_id: &str,
    ctx_packet: &LocalContextPacket,
    verdicts: &[DxrHarnessVerdictState],
    tools: &[DxrToolExecutionState],
) -> Vec<DxrEvidenceRecord> {
    let mut records = Vec::new();
    push_evidence(
        &mut records,
        "job_created",
        Some(job_id),
        None,
        Some(request.actor_id),
        payload(&[
            ("action", "dxr_job_created"),
            ("intent", request.intent),
            ("workspace", request.workspace),
            ("branch", request.branch),
            ("idempotency_key", request.idempotency_key),
        ]),
    );
    push_evidence(
        &mut records,
        "run_claimed",
        Some(job_id),
        Some(run_id),
        Some("local-dxr-worker"),
        payload(&[
            ("action", "run_claimed"),
            ("worker_id", "local-dxr-worker"),
            ("lease", "local-proof-no-live-lease"),
        ]),
    );
    let ctx_memory_tier_count = ctx_packet.memory_tiers.len().to_string();
    let ctx_local_memory_input_count = ctx_packet.local_memory_input_count.to_string();
    let max_turns = request.max_turns.to_string();
    let max_cost_cents = request.max_cost_cents.to_string();
    push_evidence(
        &mut records,
        "run_started",
        Some(job_id),
        Some(run_id),
        Some(request.actor_id),
        payload(&[
            ("action", "run_started"),
            ("ctx_assembly_receipt_id", &ctx_packet.assembly_receipt_id),
            ("ctx_memory_tier_count", &ctx_memory_tier_count),
            (
                "ctx_local_memory_input_count",
                &ctx_local_memory_input_count,
            ),
            (
                "ctx_consolidation_status",
                ctx_packet.consolidation_report.status,
            ),
            ("max_turns", &max_turns),
            ("max_cost_cents", &max_cost_cents),
        ]),
    );
    push_evidence(
        &mut records,
        "policy_enforced",
        Some(job_id),
        Some(run_id),
        Some(request.actor_id),
        payload(&[
            ("action", "local_runtime_authority_closed"),
            ("provider_calls_allowed", "false"),
            ("tool_execution_allowed", "false"),
            ("worker_execution_allowed", "false"),
        ]),
    );
    push_evidence(
        &mut records,
        "model_called",
        Some(job_id),
        Some(run_id),
        Some(request.actor_id),
        payload(&[
            ("action", "model_provider_refused"),
            ("model_driver", LOCAL_DXR_MODEL_DRIVER),
            ("provider_streaming_status", "PENDING-LIVE-RUN"),
        ]),
    );
    for tool in tools {
        push_evidence(
            &mut records,
            "tool_executed",
            Some(job_id),
            Some(run_id),
            Some(request.actor_id),
            payload(&[
                ("action", "tool_execution_denied"),
                ("tool_name", &tool.tool_name),
                ("outcome", &tool.outcome),
            ]),
        );
    }
    for verdict in verdicts {
        push_evidence(
            &mut records,
            if verdict.passed {
                "check_passed"
            } else {
                "check_failed"
            },
            Some(job_id),
            Some(run_id),
            Some(request.actor_id),
            payload(&[
                ("action", "harness_verdict_recorded"),
                ("check_name", &verdict.check_name),
                ("severity", &verdict.severity),
                ("evidence", &verdict.evidence),
            ]),
        );
    }
    push_evidence(
        &mut records,
        "run_abandoned",
        Some(job_id),
        Some(run_id),
        Some(request.actor_id),
        payload(&[
            ("action", "run_abandoned_before_live_execution"),
            ("reason", "local_runtime_fail_closed_until_authority"),
        ]),
    );
    records
}

fn push_evidence(
    records: &mut Vec<DxrEvidenceRecord>,
    event_type: &str,
    job_id: Option<&str>,
    run_id: Option<&str>,
    agent_id: Option<&str>,
    payload: BTreeMap<String, String>,
) {
    let sequence_number = records.len() as u64 + 1;
    let previous_hash = records
        .last()
        .map(|record: &DxrEvidenceRecord| record.record_hash.clone())
        .unwrap_or_else(|| DXR_GENESIS_HASH.to_string());
    let record_hash = compute_dxr_evidence_hash(
        &previous_hash,
        sequence_number,
        event_type,
        job_id,
        run_id,
        agent_id,
        &payload,
    );
    records.push(DxrEvidenceRecord {
        evidence_id: format!("dxr_evidence_{sequence_number:06}"),
        sequence_number,
        previous_hash,
        record_hash,
        event_type: event_type.to_string(),
        job_id: job_id.map(str::to_string),
        run_id: run_id.map(str::to_string),
        agent_id: agent_id.map(str::to_string),
        payload,
    });
}

fn compute_dxr_evidence_hash(
    previous_hash: &str,
    sequence_number: u64,
    event_type: &str,
    job_id: Option<&str>,
    run_id: Option<&str>,
    agent_id: Option<&str>,
    payload: &BTreeMap<String, String>,
) -> String {
    let input = format!(
        "{}{}{}{}{}{}{}",
        previous_hash,
        sequence_number,
        event_type,
        job_id.unwrap_or(""),
        run_id.unwrap_or(""),
        agent_id.unwrap_or(""),
        render_payload_json(payload)
    );
    stable_hash_bytes(input.as_bytes())
}

fn payload(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn stable_token(values: &[&str]) -> String {
    stable_hash(values).chars().take(16).collect()
}

fn stable_hash(values: &[&str]) -> String {
    let mut input = String::new();
    for value in values {
        input.push_str(value);
        input.push('|');
    }
    stable_hash_bytes(input.as_bytes())
}

fn stable_hash_bytes(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64_dxr(bytes))
}

fn fnv1a64_dxr(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn join_static_values(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

fn render_payload_json(payload: &BTreeMap<String, String>) -> String {
    let values = payload
        .iter()
        .map(|(key, value)| {
            format!(
                "{}:{}",
                json_string_literal(key),
                json_string_literal(value)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{values}}}")
}

fn render_job_json(job: &DxrJobState) -> String {
    format!(
        r#"{{"job_id":{},"status":{},"priority":{},"spec_hash":{},"idempotency_key":{}}}"#,
        json_string_literal(&job.job_id),
        json_string_literal(&job.status),
        job.priority,
        json_string_literal(&job.spec_hash),
        json_string_literal(&job.idempotency_key)
    )
}

fn render_run_json(run: &DxrRunState) -> String {
    format!(
        r#"{{"run_id":{},"job_id":{},"status":{},"agent_id":{},"turn_count":{},"parent_run_id":{}}}"#,
        json_string_literal(&run.run_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.status),
        json_string_literal(&run.agent_id),
        run.turn_count,
        run.parent_run_id
            .as_deref()
            .map(json_string_literal)
            .unwrap_or_else(|| "null".to_string())
    )
}

fn render_claim_json(claim: &DxrRunClaimState) -> String {
    format!(
        r#"{{"claim_id":{},"run_id":{},"worker_id":{},"status":{},"lease_expires_at":{},"heartbeat_status":{}}}"#,
        json_string_literal(&claim.claim_id),
        json_string_literal(&claim.run_id),
        json_string_literal(&claim.worker_id),
        json_string_literal(&claim.status),
        json_string_literal(&claim.lease_expires_at),
        json_string_literal(&claim.heartbeat_status)
    )
}

fn render_dependency_json(dependency: &DxrRunDependencyState) -> String {
    format!(
        r#"{{"dependency_id":{},"parent_run_id":{},"child_run_id":{},"status":{},"wait_condition":{}}}"#,
        json_string_literal(&dependency.dependency_id),
        json_string_literal(&dependency.parent_run_id),
        json_string_literal(&dependency.child_run_id),
        json_string_literal(&dependency.status),
        json_string_literal(&dependency.wait_condition)
    )
}

fn render_turn_json(turn: &DxrTurnLogState) -> String {
    format!(
        r#"{{"turn_id":{},"run_id":{},"turn_number":{},"model_id":{},"prompt_hash":{},"response_hash":{},"tokens_in":{},"tokens_out":{},"latency_ms":{},"cost_cents":{},"status":{}}}"#,
        json_string_literal(&turn.turn_id),
        json_string_literal(&turn.run_id),
        turn.turn_number,
        json_string_literal(&turn.model_id),
        json_string_literal(&turn.prompt_hash),
        json_string_literal(&turn.response_hash),
        turn.tokens_in,
        turn.tokens_out,
        turn.latency_ms,
        turn.cost_cents,
        json_string_literal(&turn.status)
    )
}

fn render_tool_json(tool: &DxrToolExecutionState) -> String {
    format!(
        r#"{{"execution_id":{},"run_id":{},"tool_name":{},"status":{},"outcome":{},"duration_ms":{},"policy_decision":{}}}"#,
        json_string_literal(&tool.execution_id),
        json_string_literal(&tool.run_id),
        json_string_literal(&tool.tool_name),
        json_string_literal(&tool.status),
        json_string_literal(&tool.outcome),
        tool.duration_ms,
        json_string_literal(&tool.policy_decision)
    )
}

fn render_verdict_json(verdict: &DxrHarnessVerdictState) -> String {
    format!(
        r#"{{"verdict_id":{},"run_id":{},"check_name":{},"passed":{},"severity":{},"evidence":{}}}"#,
        json_string_literal(&verdict.verdict_id),
        json_string_literal(&verdict.run_id),
        json_string_literal(&verdict.check_name),
        verdict.passed,
        json_string_literal(&verdict.severity),
        json_string_literal(&verdict.evidence)
    )
}

fn render_evidence_json(record: &DxrEvidenceRecord) -> String {
    format!(
        r#"{{"evidence_id":{},"sequence_number":{},"previous_hash":{},"record_hash":{},"event_type":{},"job_id":{},"run_id":{},"agent_id":{},"payload":{}}}"#,
        json_string_literal(&record.evidence_id),
        record.sequence_number,
        json_string_literal(&record.previous_hash),
        json_string_literal(&record.record_hash),
        json_string_literal(&record.event_type),
        record
            .job_id
            .as_deref()
            .map(json_string_literal)
            .unwrap_or_else(|| "null".to_string()),
        record
            .run_id
            .as_deref()
            .map(json_string_literal)
            .unwrap_or_else(|| "null".to_string()),
        record
            .agent_id
            .as_deref()
            .map(json_string_literal)
            .unwrap_or_else(|| "null".to_string()),
        render_payload_json(&record.payload)
    )
}
