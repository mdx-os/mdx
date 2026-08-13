use super::*;

#[test]
fn dxr_local_runtime_records_v1_shaped_execution_boundary() {
    let packet = assemble_local_dxr_runtime_packet(default_local_dxr_runtime_request())
        .expect("DXR local runtime packet");

    assert_eq!(packet.name, "mdx-dxr-local-runtime-packet");
    assert_eq!(packet.status, LOCAL_DXR_STATUS);
    assert_eq!(packet.runtime, LOCAL_DXR_RUNTIME);
    assert_eq!(packet.driver, LOCAL_DXR_DRIVER);
    assert_eq!(packet.model_driver, LOCAL_DXR_MODEL_DRIVER);
    assert_eq!(packet.ctx_runtime, "mdx-ctx-engine");
    assert!(!packet.ctx_assembly_receipt_id.is_empty());
    assert!(packet.ctx_source_count > 0);
    assert_eq!(packet.ctx_intake.memory_tier_count, 4);
    assert_eq!(packet.ctx_intake.local_memory_input_count, 1);
    assert_eq!(
        packet.ctx_intake.consolidation_status,
        "LIVE-LOCAL-CONSOLIDATION-PROPOSAL-FLOOR"
    );
    assert_eq!(
        packet.ctx_intake.handoff_status,
        "LIVE-LOCAL-HANDOFF-SUMMARY-FLOOR"
    );
    assert_eq!(packet.event_bridge_driver, LOCAL_DXR_EVENT_BRIDGE);
    assert_eq!(packet.ledger_bridge_driver, LOCAL_DXR_LEDGER_BRIDGE);
    assert!(!packet.provider_calls_allowed);
    assert!(!packet.tool_execution_allowed);
    assert!(!packet.shell_execution_allowed);
    assert!(!packet.git_execution_allowed);
    assert!(!packet.worker_execution_allowed);
    assert!(!packet.production_writes_allowed);
    assert!(packet.estimated_queue_p95_ms <= packet.hot_path_budget_ms);
    assert!(!packet.job.job_id.is_empty());
    assert_eq!(packet.job.status, "pending");
    assert_eq!(packet.run.status, "abandoned_before_live_execution");
    assert_eq!(packet.claim.status, "released");
    assert_eq!(packet.dependencies.len(), 2);
    assert_eq!(packet.turns.len(), 1);
    assert_eq!(packet.tool_executions.len(), 3);
    assert!(
        packet
            .tool_executions
            .iter()
            .all(|tool| tool.outcome == "denied_by_policy")
    );
    assert!(packet.harness_verdicts.len() >= 5);
    assert!(
        packet
            .harness_verdicts
            .iter()
            .any(|verdict| verdict.check_name == "ctx_packet_consumed")
    );
    assert!(
        packet
            .harness_verdicts
            .iter()
            .any(|verdict| verdict.check_name == "ctx_memory_tiers_available")
    );
    assert!(packet.evidence_records.len() >= 10);
    assert!(packet.chain_valid);
    assert!(verify_dxr_evidence_chain(&packet.evidence_records).is_ok());
    assert!(
        packet
            .evidence_records
            .iter()
            .any(|record| record.event_type == "tool_executed")
    );
    assert!(
        packet
            .evidence_records
            .iter()
            .any(|record| record.event_type == "model_called")
    );
    assert!(packet.evidence_records.iter().any(|record| {
        record.event_type == "run_started"
            && record.payload["ctx_memory_tier_count"] == "4"
            && record.payload["ctx_local_memory_input_count"] == "1"
    }));
}

#[test]
fn dxr_local_runtime_rejects_empty_authority_lists() {
    let request = DxrLocalRuntimeRequest {
        tenant_id: "tenant_local",
        actor_id: "actor",
        intent: "intent",
        workspace: "/workspace",
        branch: "main",
        idempotency_key: "key",
        max_turns: 1,
        max_cost_cents: 0,
        requested_tools: &[],
        quality_gates: &["evidence_chain_valid"],
    };
    let error = assemble_local_dxr_runtime_packet(request).expect_err("empty tools must fail");
    assert_eq!(
        error.message(),
        "DXR local runtime list requested_tools is empty"
    );
}
