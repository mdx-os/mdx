use super::*;
use std::collections::BTreeSet;
fn local_correlation() -> CorrelationIds {
    CorrelationIds {
        tenant_id: TenantId::new("tenant_test"),
        trace_id: TraceId::new("trace_test"),
        actor_id: ActorId::new("actor_test"),
        loop_id: LoopId::new("loop_test"),
        workflow_id: WorkflowId::new("workflow_test"),
    }
}
#[rustfmt::skip]
fn local_ctx_request() -> LocalContextRequest<'static> { LocalContextRequest { tenant_id: "local_tenant", actor_id: "local_user", query: "forge memory world model", token_budget: 120, source_preferences: &[], query_entities: &[], local_memories: &[] } }
fn valid_worker_spawn_admission() -> WorkerSpawnAdmission<'static> {
    WorkerSpawnAdmission {
        worker_template_id: "worker_bounded_task_template",
        parent_id: "forge_orchestrator_agent",
        sponsor_chain_authority_receipt_id: "receipt_sponsor",
        human_sponsor_chain: &["local_human_sponsor"],
        worker_lease_authority_receipt_id: "receipt_lease",
        scope: "forge_clean_build_entry",
        credential_scope: "forge_clean_build_entry",
        expires_at: "2030-01-01T00:00:00Z",
        now: "2026-01-01T00:00:00Z",
        budget_authority_receipt_id: "receipt_budget",
        budget: WorkerSpawnBudget {
            max_runtime_ms: 250,
            max_tool_calls: 1,
        },
        tool_allowlist_authority_receipt_id: "receipt_tools",
        tool_allowlist: &["local_read", "local_write_patch"],
        credential_check_receipt_id: "receipt_credential",
        credential_requested_receipt_kind: "worker.credential.checked",
        issuer_loop_id: "evals_runner_agent",
        requested_receipt_kind: "worker.spawn_requested",
    }
}
fn valid_worker_handoff_admission() -> WorkerHandoffAdmission<'static> {
    WorkerHandoffAdmission {
        parent_loop_id: "forge_orchestrator_agent",
        worker_template_id: "worker_bounded_task_template",
        worker_run_id: "worker_run_001",
        spawn_receipt_id: "receipt_spawn",
        credential_check_receipt_id: "receipt_credential",
        output_artifacts: &["artifact_patch", "artifact_test_log"],
        verification_evidence: &["ci.verification.observed"],
        summary: "worker completed bounded task",
        next_owner: "forge_orchestrator_agent",
        requested_receipt_kind: "worker.handoff.recorded",
    }
}
fn valid_worker_retirement_admission() -> WorkerRetirementAdmission<'static> {
    WorkerRetirementAdmission {
        parent_loop_id: "forge_orchestrator_agent",
        worker_template_id: "worker_bounded_task_template",
        worker_run_id: "worker_run_001",
        spawn_receipt_id: "receipt_spawn",
        handoff_receipt_id: "receipt_handoff",
        requested_receipt_kind: "worker.retired",
    }
}

fn valid_harness_run_manifest() -> HarnessRunManifest<'static> {
    HarnessRunManifest {
        run_id: "harness_manifest_001",
        tenant_id: "tenant_local",
        actor_id: "codex",
        origin_surface: "codex_thread",
        mode: HarnessRunMode::PlanOnly,
        goal_summary: "add deterministic local plan runner",
        definition_of_done: "plan emitted, receipts recorded, writes refused",
        input_artifacts: &[
            "docs/HARNESS-RUN-MANIFEST-CONTRACT.md",
            "generated/architecture/harness-run-manifest-contract.json",
        ],
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
        approval_mode: HarnessApprovalMode::None,
        approval_receipt_required: false,
        required_receipt_kinds: &[
            "harness.run.admitted",
            "harness.plan.emitted",
            "harness.write.refused",
        ],
        telemetry_redaction_required: true,
        exit_criteria: &["plan_only_completed", "write_refusal_recorded"],
        trust_boundary: HarnessTrustBoundary {
            workspace_trusted: true,
            project_local_execution_allowed: false,
            mcp_startup_allowed: false,
            network_allowed: false,
            secrets_available: false,
            approval_receipt_id: None,
        },
        policy_profile: Some("core"),
        enterprise_pack: Some("none"),
    }
}
fn valid_harness_patch_manifest() -> HarnessRunManifest<'static> {
    let mut manifest = valid_harness_run_manifest();
    manifest.mode = HarnessRunMode::AskBeforeAction;
    manifest.definition_of_done = "patch proposal mediated without application";
    manifest.allowed_write_scope = &["crates/mdx-core/src/"];
    manifest.allowed_tools = &["local_write_patch"];
    manifest.blocked_tools = &["shell", "git", "browser", "mcp", "network"];
    manifest.permission_policy = HarnessPermissionPolicy {
        file_read: HarnessPermissionLevel::ReadOnly,
        file_write: HarnessPermissionLevel::AllowedWithPolicy,
        shell: HarnessPermissionLevel::Forbidden,
        git: HarnessPermissionLevel::Forbidden,
        patch: HarnessPermissionLevel::AllowedWithPolicy,
        browser: HarnessPermissionLevel::Forbidden,
        mcp: HarnessPermissionLevel::Forbidden,
        network: HarnessPermissionLevel::Forbidden,
    };
    manifest.budget_policy.max_tool_calls = 2;
    manifest.required_receipt_kinds = &[
        "harness.run.admitted",
        "harness.plan.emitted",
        "harness.write.refused",
        "harness.tool.patch.allowed",
        "harness.tool.patch.denied",
    ];
    manifest.exit_criteria = &["patch_proposal_mediated", "dangerous_patch_denied"];
    manifest
}
fn deterministic_harness_provider_profile() -> HarnessProviderProfile<'static> {
    HarnessProviderProfile {
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
    }
}
fn deterministic_harness_enterprise_pack() -> HarnessEnterprisePack<'static> {
    HarnessEnterprisePack {
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
    }
}

fn live_harness_provider_profile() -> HarnessProviderProfile<'static> {
    HarnessProviderProfile {
        profile_id: "tensorzero_live",
        provider_kind: HarnessProviderKind::TensorZeroModelGateway,
        model_id: "tensorzero_gateway",
        allowed_modes: &["plan_only"],
        max_context_tokens: 4096,
        max_output_tokens: 1024,
        max_cost_cents: 0,
        data_retention: "provider_ephemeral",
        context_redaction_required: true,
        telemetry_redaction_required: true,
        provider_fail_closed: true,
        policy_profile: "core",
        enterprise_pack: "none",
        enabled: true,
    }
}
#[test]
fn evals_runner_agent_passes_disappear_test_locally() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_evals_runner_agent().expect("loop run");

    assert_eq!(report.status, "COMPLETED");
    assert_eq!(report.score, 100);
    assert_eq!(report.credential_status, "MINTED");
    assert_eq!(kernel.loop_transitions().len(), 5);
    assert!(kernel.ledger().verify().is_ok());
    assert!(
        kernel
            .concierge_answer("what happened")
            .contains("Source receipts")
    );
}
#[test]
fn kernel_can_boot_with_explicit_storage_provider() {
    let storage = InMemoryStorage::default();
    let mut kernel = MdxKernel::with_storage(storage);
    let report = kernel.run_evals_runner_agent().expect("loop run");

    assert_eq!(report.status, "COMPLETED");
    assert_eq!(kernel.ledger().entries().len(), 14);
    assert_eq!(kernel.eval_verdicts().len(), 1);
    assert_eq!(kernel.credentials().len(), 1);
}
#[test]
fn storage_provider_appends_receipts_through_boundary() {
    let mut storage = InMemoryStorage::default();
    let mut ids = IdFactory::default();
    let receipt = storage.append_receipt(
        &mut ids,
        &local_correlation(),
        "loop.boundary.checked",
        Some("policy_boundary".to_string()),
        payload(&[("boundary", "storage_provider")]),
    );

    assert_eq!(receipt.kind, "loop.boundary.checked");
    assert_eq!(storage.ledger().query().count(), 1);
    assert_eq!(
        storage
            .ledger()
            .query()
            .by_id(&receipt.receipt_id)
            .map(|entry| entry.policy_decision_id.as_deref()),
        Some(Some("policy_boundary"))
    );
    storage.ledger().verify().expect("valid receipt chain");
}
#[test]
fn ledger_verify_is_incremental_and_verify_full_re_checks_everything() {
    // The watermark: verify() after a verify() only checks new entries, so a
    // write path is no longer O(history). verify_full() ignores the
    // watermark, which is what restore boundaries use - a chain tampered
    // behind the watermark still fails the full check.
    let mut ledger = Ledger::default();
    let mut ids = IdFactory::default();
    let correlation = CorrelationIds {
        tenant_id: TenantId::new("tenant_local"),
        trace_id: TraceId::new("trace_incremental_verify"),
        actor_id: ActorId::new("human:verify_test"),
        loop_id: LoopId::new("verify_test_loop"),
        workflow_id: WorkflowId::new("wf_verify_test"),
    };
    for _ in 0..3 {
        ledger.append(
            &mut ids,
            &correlation,
            "verify.test.recorded",
            None,
            BTreeMap::new(),
        );
    }
    ledger.verify().expect("first verify walks the chain");
    ledger.verify().expect("second verify is a no-op");

    let mut entries = ledger.entries().to_vec();
    entries[1].kind = "tampered.kind".to_string();
    let mut tampered = Ledger::default();
    assert!(tampered.restore_entries(entries).is_err());

    ledger.verify_full().expect("full re-check still passes");
}
#[test]
fn new_receipts_bind_trusted_time_into_the_hash() {
    let mut ledger = Ledger::default();
    let mut ids = IdFactory::default();
    let first = ledger.append(
        &mut ids,
        &local_correlation(),
        "trusted.time.recorded",
        None,
        BTreeMap::new(),
    );
    let second = ledger.append(
        &mut ids,
        &local_correlation(),
        "trusted.time.recorded",
        None,
        BTreeMap::new(),
    );

    assert_eq!(first.hash_version, RECEIPT_HASH_VERSION_TRUSTED_TIME);
    assert_ne!(first.receipt_timestamp, trusted_receipt_timestamp(1));
    assert_ne!(second.receipt_timestamp, trusted_receipt_timestamp(2));
    assert!(first.receipt_timestamp <= second.receipt_timestamp);
    ledger.verify_full().expect("trusted-time chain verifies");

    let mut tampered = ledger.entries().to_vec();
    tampered[0].receipt_timestamp = trusted_receipt_timestamp(99);
    let mut restored = Ledger::default();
    assert!(
        restored.restore_entries(tampered).is_err(),
        "timestamp rewrites must change the receipt hash"
    );
}
#[test]
fn trusted_time_receipts_refuse_backdated_chain_order() {
    let mut ledger = Ledger::default();
    let mut ids = IdFactory::default();
    ledger.append(
        &mut ids,
        &local_correlation(),
        "trusted.time.recorded",
        None,
        BTreeMap::new(),
    );
    ledger.append(
        &mut ids,
        &local_correlation(),
        "trusted.time.recorded",
        None,
        BTreeMap::new(),
    );
    let mut entries = ledger.entries().to_vec();
    entries[1].receipt_timestamp = "2025-12-31T23:59:59.000000000Z".to_string();
    let correlation = CorrelationIds {
        tenant_id: entries[1].tenant_id.clone(),
        trace_id: entries[1].trace_id.clone(),
        actor_id: entries[1].actor_id.clone(),
        loop_id: entries[1].loop_id.clone(),
        workflow_id: entries[1].workflow_id.clone(),
    };
    entries[1].hash = receipt_hash_for_version(ReceiptHashParts {
        receipt_id: &entries[1].receipt_id,
        correlation: &correlation,
        kind: &entries[1].kind,
        policy_decision_id: entries[1].policy_decision_id.as_deref(),
        payload: &entries[1].payload,
        previous_hash: entries[1].previous_hash.as_deref(),
        receipt_timestamp: &entries[1].receipt_timestamp,
        hash_version: entries[1].hash_version,
    });

    let mut restored = Ledger::default();
    let error = restored
        .restore_entries(entries)
        .expect_err("backdated receipt should fail");
    assert!(error.contains("receipt_timestamp before"));
}
#[test]
fn legacy_timeless_receipts_still_restore_with_v1_hashes() {
    let mut ledger = Ledger::default();
    let mut ids = IdFactory::default();
    ledger.append(
        &mut ids,
        &local_correlation(),
        "legacy.time.recorded",
        None,
        payload(&[("legacy", "true")]),
    );
    let mut entries = ledger.entries().to_vec();
    entries[0].receipt_timestamp.clear();
    entries[0].hash_version = RECEIPT_HASH_VERSION_TIMELESS;
    let correlation = CorrelationIds {
        tenant_id: entries[0].tenant_id.clone(),
        trace_id: entries[0].trace_id.clone(),
        actor_id: entries[0].actor_id.clone(),
        loop_id: entries[0].loop_id.clone(),
        workflow_id: entries[0].workflow_id.clone(),
    };
    entries[0].hash = receipt_hash_for_version(ReceiptHashParts {
        receipt_id: &entries[0].receipt_id,
        correlation: &correlation,
        kind: &entries[0].kind,
        policy_decision_id: entries[0].policy_decision_id.as_deref(),
        payload: &entries[0].payload,
        previous_hash: entries[0].previous_hash.as_deref(),
        receipt_timestamp: &entries[0].receipt_timestamp,
        hash_version: entries[0].hash_version,
    });

    let mut restored = Ledger::default();
    assert_eq!(
        restored.restore_entries(entries).expect("legacy restore"),
        1
    );
}
#[test]
fn postgres_ledger_export_sql_is_rendered_from_core_contract() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_evals_runner_agent().expect("loop run");
    let final_receipt = kernel.ledger().entries().last().expect("final receipt");

    let sql = render_postgres_ledger_export_sql(kernel.ledger().entries(), &report.loop_id);

    assert!(sql.contains("INSERT INTO tenants"));
    assert!(sql.contains("INSERT INTO actors"));
    assert!(sql.contains("INSERT INTO ledger_entries"));
    assert!(sql.contains("INSERT INTO ledger_chain_heads"));
    assert!(sql.contains("ON CONFLICT (tenant_id) DO UPDATE SET"));
    assert!(sql.contains(&final_receipt.receipt_id));
    assert!(sql.contains(&final_receipt.hash));
    assert!(sql.contains("'evals_runner_agent'"));
    assert!(sql.contains("SELECT count(*) FROM ledger_entries"));
    assert_eq!(sql.matches("INSERT INTO ledger_entries").count(), 14);
    assert_eq!(sql.matches("INSERT INTO ledger_chain_heads").count(), 1);
}
#[test]
fn postgres_storage_requires_database_url() {
    let result = PostgresStorage::connect(None);
    assert_eq!(result, Err(StorageAdapterError::MissingDatabaseUrl));
}
#[test]
fn postgres_storage_fails_closed_until_live_migrations_are_observed() {
    let result = PostgresStorage::connect(Some("postgres://mdx:mdx@localhost/mdx"));
    assert_eq!(
        result,
        Err(StorageAdapterError::PendingLiveRun {
            adapter: "PostgresStorage",
            reason: "migrations have not been applied against a running Postgres server"
        })
    );
    assert_eq!(PostgresStorage::adapter_name(), "PostgresStorage");
}
#[test]
fn postgres_storage_accepts_observed_migration_evidence() {
    let report = migration_report();
    let storage = PostgresStorage::connect_after_observed_migrations(
        Some("postgres://mdx:mdx@localhost/mdx"),
        PostgresMigrationEvidence {
            migration_count: report.migration_count,
            tenant_owned_tables: report.tenant_owned_tables,
            rls_enabled_tables: report.rls_enabled_tables,
            observed_by: "unit_test".to_string(),
        },
    )
    .expect("observed migration evidence");

    assert_eq!(storage.database_url(), "postgres://mdx:mdx@localhost/mdx");
}

#[test]
fn postgres_storage_renders_receipt_write_sql_after_migration_evidence() {
    let report = migration_report();
    let storage = PostgresStorage::connect_after_observed_migrations(
        Some("postgres://mdx:mdx@localhost/mdx"),
        PostgresMigrationEvidence {
            migration_count: report.migration_count,
            tenant_owned_tables: report.tenant_owned_tables,
            rls_enabled_tables: report.rls_enabled_tables,
            observed_by: "unit_test".to_string(),
        },
    )
    .expect("observed migration evidence");
    let mut kernel = MdxKernel::boot_local();
    let loop_report = kernel.run_evals_runner_agent().expect("loop run");

    let sql = storage.render_receipt_write_sql(kernel.ledger().entries(), &loop_report.loop_id);

    assert!(sql.contains("INSERT INTO ledger_entries"));
    assert!(sql.contains("INSERT INTO ledger_chain_heads"));
    assert!(sql.contains("SELECT count(*) FROM ledger_entries"));
    assert_eq!(sql.matches("INSERT INTO ledger_entries").count(), 14);
}

#[test]
fn postgres_storage_rejects_migration_evidence_mismatch() {
    let result = PostgresStorage::connect_after_observed_migrations(
        Some("postgres://mdx:mdx@localhost/mdx"),
        PostgresMigrationEvidence {
            migration_count: 0,
            tenant_owned_tables: 0,
            rls_enabled_tables: 0,
            observed_by: "unit_test".to_string(),
        },
    );

    assert!(matches!(
        result,
        Err(StorageAdapterError::MigrationEvidenceMismatch { .. })
    ));
}

#[test]
fn postgres_receipt_writer_requires_database_url() {
    let result = PostgresReceiptWriter::connect(None);
    assert_eq!(result, Err(StorageAdapterError::MissingDatabaseUrl));
}

#[test]
fn postgres_receipt_writer_fails_closed_until_observed() {
    let result = PostgresReceiptWriter::connect(Some("postgres://mdx:mdx@localhost/mdx"));
    assert_eq!(
        result,
        Err(StorageAdapterError::PendingLiveRun {
            adapter: "PostgresReceiptWriter",
            reason: "durable receipt writes have not been observed through PostgresStorage"
        })
    );
}

#[test]
fn postgres_receipt_writer_accepts_loop_export_evidence() {
    let evidence = PostgresLoopExportEvidence::from_local_loop_budgets(
        migration_report().migration_count,
        "unit test",
    );
    let writer = PostgresReceiptWriter::connect_after_observed_loop_export(
        Some("postgres://mdx:mdx@localhost/mdx"),
        evidence,
    )
    .expect("writer evidence accepted");

    assert_eq!(writer.database_url(), "postgres://mdx:mdx@localhost/mdx");
    let contract = PostgresReceiptWriter::contract();
    assert_eq!(contract.adapter, "PostgresReceiptWriter");
    assert!(contract.database_url_required);
    assert!(contract.observed_loop_export_required);
    assert_eq!(contract.ledger_table, "ledger_entries");
    assert_eq!(contract.chain_head_table, "ledger_chain_heads");
    assert!(contract.receipt_count_by_loop_required);
}

#[test]
fn postgres_receipt_writer_rejects_mismatched_loop_export_evidence() {
    let mut evidence = PostgresLoopExportEvidence::from_local_loop_budgets(
        migration_report().migration_count,
        "unit test",
    );
    evidence.loop_receipt_counts[0].ledger_receipts = 0;

    assert_eq!(
        PostgresReceiptWriter::connect_after_observed_loop_export(
            Some("postgres://mdx:mdx@localhost/mdx"),
            evidence,
        ),
        Err(StorageAdapterError::PendingLiveRun {
            adapter: "PostgresReceiptWriter",
            reason: "local loop ledger export evidence does not match expected receipt counts"
        })
    );
}

#[test]
fn postgres_receipt_writer_rejects_missing_loop_export_evidence() {
    let mut evidence = PostgresLoopExportEvidence::from_local_loop_budgets(
        migration_report().migration_count,
        "unit test",
    );
    evidence.loop_receipt_counts.pop();

    assert_eq!(
        PostgresReceiptWriter::connect_after_observed_loop_export(
            Some("postgres://mdx:mdx@localhost/mdx"),
            evidence,
        ),
        Err(StorageAdapterError::PendingLiveRun {
            adapter: "PostgresReceiptWriter",
            reason: "local loop ledger export evidence does not match expected receipt counts"
        })
    );
}

#[test]
fn local_loop_runner_preserves_first_loop_behavior() {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalLoopRunner;
    let report = runner
        .run_evals_runner_agent(&mut kernel)
        .expect("loop run");

    assert_eq!(
        <LocalLoopRunner as LoopRunner<InMemoryStorage>>::runner_name(&runner),
        "LocalLoopRunner"
    );
    assert_eq!(report.status, "COMPLETED");
    assert_eq!(report.credential_status, "MINTED");
    assert_eq!(kernel.loop_transitions().len(), 5);
}
mod access_control_matrix;
mod actor_admission;
mod agent_delegation_runtime;
mod app_state_export;
mod ctx_local_engine;
mod deployment_profile;
mod dxr_local_runtime;
mod first_run_profile;
mod forge_ship_ratification;
mod harness;
mod harness_envelope_runtime;
mod harness_execute;
mod harness_fleet_eval;
mod harness_long_horizon_mission;
mod harness_pipeline;
mod harness_provider;
mod harness_runtime_metrics;
mod harness_sensors;
mod harness_ship_readiness;
mod harness_work_classification;
mod harness_worker_admission;
mod install_owner;
mod live_worker_execution;
mod memory_store;
mod message_action;
mod message_fanout_request;
mod message_presence_request;
mod message_thread_message;
mod model_turn_on;
mod pages_approval_request;
mod pages_edit_draft;
mod pages_publication;
mod production_auth_boundary;
mod setup_track;
mod studio_presence;
mod studio_run;
mod studio_steering;
mod trusted_session_verifier;
mod twin_model_gateway_observation;
mod twin_session_draft;
mod twin_session_trusted_context;
mod work_triage;
#[test]
fn aegis_scanner_agent_passes_disappear_test_locally() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_aegis_scanner_agent().expect("aegis loop run");

    assert_eq!(report.loop_id, "aegis_scanner_agent");
    assert_eq!(report.status, "COMPLETED");
    assert_eq!(report.score, 100);
    assert_eq!(report.credential_status, "NOT_APPLICABLE");
    assert_eq!(kernel.loop_transitions().len(), 5);
    assert_eq!(kernel.charter_records().len(), 1);
    assert!(kernel.ledger().verify().is_ok());
    assert!(report.concierge_answer.contains("aegis_local_finding_001"));
}

#[test]
fn local_loop_runner_runs_aegis_scanner_agent() {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalLoopRunner;
    let report = runner
        .run_aegis_scanner_agent(&mut kernel)
        .expect("aegis loop run");

    assert_eq!(report.loop_id, "aegis_scanner_agent");
    assert_eq!(report.status, "COMPLETED");
    assert_eq!(kernel.loop_runs().len(), 1);
}

#[test]
fn charter_attestation_agent_passes_disappear_test_locally() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_charter_attestation_agent()
        .expect("charter loop run");

    assert_eq!(report.loop_id, "charter_attestation_agent");
    assert_eq!(report.status, "COMPLETED");
    assert_eq!(report.score, 100);
    assert_eq!(report.credential_status, "NOT_APPLICABLE");
    assert_eq!(
        report.receipts.len(),
        local_loop_budget("charter_attestation_agent")
            .expect("charter attestation budget")
            .max_receipts
    );
    assert_eq!(kernel.policy_decisions().len(), 5);
    assert_eq!(kernel.loop_transitions().len(), 5);
    assert_eq!(kernel.charter_records().len(), 1);
    assert_eq!(kernel.outbox_events().len(), 1);
    assert!(kernel.ledger().verify().is_ok());
    assert!(
        report
            .concierge_answer
            .contains("charter_attestation_agent checked a declared obligation")
    );
}

#[test]
fn local_loop_runner_runs_charter_attestation_agent() {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalLoopRunner;
    let report = runner
        .run_charter_attestation_agent(&mut kernel)
        .expect("charter loop runner");

    assert_eq!(report.loop_id, "charter_attestation_agent");
    assert_eq!(report.status, "COMPLETED");
    assert_eq!(kernel.loop_runs().len(), 1);
}

#[test]
fn forge_orchestrator_agent_blocks_at_talent_authority_boundary() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_forge_orchestrator_agent()
        .expect("forge loop run");

    assert_eq!(report.loop_id, "forge_orchestrator_agent");
    assert_eq!(report.status, "BLOCKED_ON_TALENT_AUTHORITY");
    assert_eq!(report.score, 0);
    assert_eq!(report.credential_status, "NOT_APPLICABLE");
    assert_eq!(
        report.receipts.len(),
        local_loop_budget("forge_orchestrator_agent")
            .expect("forge orchestrator budget")
            .max_receipts
    );
    assert_eq!(kernel.policy_decisions().len(), 5);
    assert_eq!(kernel.loop_transitions().len(), 5);
    assert_eq!(kernel.outbox_events().len(), 1);
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.delegation.requested")[0]
            .payload
            .contains_key("talent_authorization_envelope_id")
    );
    assert!(
        kernel.ledger().query().by_kind("loop.adjustment.recorded")[0]
            .payload
            .contains_key("terminal_state")
    );
    assert!(kernel.ledger().verify().is_ok());
    assert!(
        report
            .concierge_answer
            .contains("blocked before worker spawn")
    );
}

#[test]
fn local_loop_runner_runs_forge_orchestrator_agent() {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalLoopRunner;
    let report = runner
        .run_forge_orchestrator_agent(&mut kernel)
        .expect("forge loop runner");

    assert_eq!(report.loop_id, "forge_orchestrator_agent");
    assert_eq!(report.status, "BLOCKED_ON_TALENT_AUTHORITY");
    assert_eq!(kernel.loop_runs().len(), 1);
}

#[test]
fn talent_autonomy_agent_records_authority_and_blocks_before_live_worker_execution() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_talent_autonomy_agent().expect("talent loop run");

    assert_eq!(report.loop_id, "talent_autonomy_agent");
    assert_eq!(report.status, "BLOCKED_ON_LIVE_WORKER_EXECUTION");
    assert_eq!(report.score, 0);
    assert_eq!(report.credential_status, "CHECKED_LOCAL_AUTHORITY");
    assert_eq!(
        report.receipts.len(),
        local_loop_budget("talent_autonomy_agent")
            .expect("talent autonomy budget")
            .max_receipts
    );
    assert_eq!(kernel.policy_decisions().len(), 9);
    assert_eq!(kernel.loop_transitions().len(), 9);
    assert_eq!(kernel.outbox_events().len(), 1);

    let query = kernel.ledger().query();
    let authorization = &query.by_kind("talent.authorization.recorded")[0];
    for field in [
        "sponsor_chain_authority_receipt_id",
        "worker_lease_authority_receipt_id",
        "budget_authority_receipt_id",
        "tool_allowlist_authority_receipt_id",
    ] {
        assert!(authorization.payload.contains_key(field), "{field}");
    }
    assert!(
        query.by_kind("worker.credential.checked")[0]
            .payload
            .contains_key("authorization_receipt_id")
    );
    assert!(
        query.by_kind("worker.spawn_requested")[0]
            .payload
            .contains_key("credential_check_receipt_id")
    );
    assert!(query.by_kind("worker.handoff.recorded").is_empty());
    assert!(query.by_kind("worker.retired").is_empty());
    assert!(kernel.ledger().verify().is_ok());
    assert!(
        report
            .concierge_answer
            .contains("blocked before live worker execution")
    );
}

#[test]
fn product_shaping_agent_shapes_bet_and_blocks_before_human_ratification() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_product_shaping_agent()
        .expect("product shaping loop run");

    assert_eq!(report.loop_id, "product_shaping_agent");
    assert_eq!(report.status, "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION");
    assert_eq!(report.score, 0);
    assert_eq!(report.credential_status, "NOT_APPLICABLE");
    assert_eq!(
        report.receipts.len(),
        local_loop_budget("product_shaping_agent")
            .expect("product shaping budget")
            .max_receipts
    );
    assert_eq!(kernel.policy_decisions().len(), 5);
    assert_eq!(kernel.loop_transitions().len(), 5);
    assert_eq!(kernel.outbox_events().len(), 1);

    let query = kernel.ledger().query();
    let shaped_bet = &query.by_kind("product.bet.shaped")[0];
    assert!(shaped_bet.payload.contains_key("source_signal_receipt_id"));
    assert_eq!(
        shaped_bet.payload.get("shape_status"),
        Some(&"DRAFT_REQUIRES_HUMAN_RATIFICATION".to_string())
    );
    let handoff = &query.by_kind("product.handoff.requested")[0];
    assert!(handoff.payload.contains_key("shaped_bet_receipt_id"));
    assert_eq!(
        handoff.payload.get("ratification_required"),
        Some(&"true".to_string())
    );
    assert!(kernel.ledger().verify().is_ok());
    assert!(report.concierge_answer.contains("human ratification"));
}

#[test]
fn local_worker_runtime_records_handoff_then_retirement() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_talent_autonomy_agent().expect("talent loop run");
    let query = kernel.ledger().query();
    let spawn_receipt_id = query.by_kind("worker.spawn_requested")[0]
        .receipt_id
        .clone();
    let credential_check_receipt_id = query.by_kind("worker.credential.checked")[0]
        .receipt_id
        .clone();

    let report = kernel
        .run_local_worker_runtime(WorkerRuntimeRequest {
            spawn_receipt_id: spawn_receipt_id.clone(),
            credential_check_receipt_id: credential_check_receipt_id.clone(),
            output_artifacts: vec![
                "artifact_patch".to_string(),
                "artifact_test_log".to_string(),
            ],
            verification_evidence: vec!["ci.verification.observed".to_string()],
            summary: "local worker completed bounded task".to_string(),
            next_owner: "forge_orchestrator_agent".to_string(),
        })
        .expect("local worker runtime");

    assert_eq!(report.status, "RETIRED_AFTER_HANDOFF");
    assert_eq!(
        report.source_receipts,
        vec![spawn_receipt_id, credential_check_receipt_id]
    );
    let query = kernel.ledger().query();
    let handoff = &query.by_kind("worker.handoff.recorded")[0];
    let retirement = &query.by_kind("worker.retired")[0];
    assert_eq!(report.handoff_receipt_id, handoff.receipt_id);
    assert_eq!(report.retirement_receipt_id, retirement.receipt_id);
    assert_eq!(
        retirement.payload.get("handoff_receipt_id"),
        Some(&handoff.receipt_id)
    );
    assert_eq!(kernel.outbox_events().len(), 3);
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn local_worker_runtime_requires_spawn_and_credential_receipts() {
    let mut kernel = MdxKernel::boot_local();

    let missing_spawn = kernel.run_local_worker_runtime(WorkerRuntimeRequest {
        spawn_receipt_id: "missing_spawn".to_string(),
        credential_check_receipt_id: "missing_credential".to_string(),
        output_artifacts: vec!["artifact_patch".to_string()],
        verification_evidence: vec!["ci.verification.observed".to_string()],
        summary: "local worker completed bounded task".to_string(),
        next_owner: "forge_orchestrator_agent".to_string(),
    });

    assert_eq!(
        missing_spawn,
        Err("worker spawn receipt is required".to_string())
    );
}

#[test]
fn local_loop_runner_runs_talent_autonomy_agent() {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalLoopRunner;
    let report = runner
        .run_talent_autonomy_agent(&mut kernel)
        .expect("talent loop runner");

    assert_eq!(report.loop_id, "talent_autonomy_agent");
    assert_eq!(report.status, "BLOCKED_ON_LIVE_WORKER_EXECUTION");
    assert_eq!(kernel.loop_runs().len(), 1);
}

#[test]
fn local_loop_runner_runs_product_shaping_agent() {
    let mut kernel = MdxKernel::boot_local();
    let runner = LocalLoopRunner;
    let report = runner
        .run_product_shaping_agent(&mut kernel)
        .expect("product shaping loop runner");

    assert_eq!(report.loop_id, "product_shaping_agent");
    assert_eq!(report.status, "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION");
    assert_eq!(kernel.loop_runs().len(), 1);
}

#[test]
fn worker_spawn_admission_allows_bounded_request() {
    let request = valid_worker_spawn_admission();

    assert_eq!(admit_worker_spawn(&request), Ok(()));
}

#[test]
fn worker_spawn_admission_rejects_missing_authority_receipt() {
    let mut request = valid_worker_spawn_admission();
    request.budget_authority_receipt_id = "";

    assert_eq!(
        admit_worker_spawn(&request),
        Err(WorkerSpawnRejection::MissingAuthorityReceipt(
            "budget_authority_receipt_id"
        ))
    );
}

#[test]
fn worker_spawn_admission_rejects_expired_request() {
    let mut request = valid_worker_spawn_admission();
    request.expires_at = "2026-01-01T00:00:00Z";

    assert_eq!(
        admit_worker_spawn(&request),
        Err(WorkerSpawnRejection::ExpiredLeaseOrCredential)
    );
}

#[test]
fn worker_spawn_admission_rejects_scope_mismatch() {
    let mut request = valid_worker_spawn_admission();
    request.credential_scope = "other_scope";

    assert_eq!(
        admit_worker_spawn(&request),
        Err(WorkerSpawnRejection::ScopeMismatch)
    );
}

#[test]
fn worker_spawn_admission_rejects_empty_budget_and_tools() {
    let mut request = valid_worker_spawn_admission();
    request.budget.max_tool_calls = 0;
    assert_eq!(
        admit_worker_spawn(&request),
        Err(WorkerSpawnRejection::EmptyBudget)
    );

    request = valid_worker_spawn_admission();
    request.tool_allowlist = &[];
    assert_eq!(
        admit_worker_spawn(&request),
        Err(WorkerSpawnRejection::EmptyToolAllowlist)
    );
}

#[test]
fn worker_spawn_admission_rejects_wrong_receipt_kinds() {
    let mut request = valid_worker_spawn_admission();
    request.credential_requested_receipt_kind = "credential.minted";
    assert_eq!(
        admit_worker_spawn(&request),
        Err(WorkerSpawnRejection::InvalidCredentialReceiptKind)
    );

    request = valid_worker_spawn_admission();
    request.requested_receipt_kind = "worker.handoff.recorded";
    assert_eq!(
        admit_worker_spawn(&request),
        Err(WorkerSpawnRejection::InvalidSpawnReceiptKind)
    );
}

#[test]
fn worker_handoff_admission_allows_evidenced_output() {
    let request = valid_worker_handoff_admission();

    assert_eq!(admit_worker_handoff(&request), Ok(()));
}

#[test]
fn worker_handoff_admission_rejects_missing_receipts_or_output() {
    let mut request = valid_worker_handoff_admission();
    request.spawn_receipt_id = "";
    assert_eq!(
        admit_worker_handoff(&request),
        Err(WorkerHandoffRejection::MissingField("spawn_receipt_id"))
    );

    request = valid_worker_handoff_admission();
    request.output_artifacts = &[];
    assert_eq!(
        admit_worker_handoff(&request),
        Err(WorkerHandoffRejection::MissingOutputArtifacts)
    );

    request = valid_worker_handoff_admission();
    request.verification_evidence = &[];
    assert_eq!(
        admit_worker_handoff(&request),
        Err(WorkerHandoffRejection::MissingVerificationEvidence)
    );
}

#[test]
fn worker_handoff_admission_rejects_wrong_receipt_kind() {
    let mut request = valid_worker_handoff_admission();
    request.requested_receipt_kind = "worker.retired";

    assert_eq!(
        admit_worker_handoff(&request),
        Err(WorkerHandoffRejection::InvalidHandoffReceiptKind)
    );
}

#[test]
fn worker_retirement_admission_allows_after_handoff() {
    let request = valid_worker_retirement_admission();

    assert_eq!(admit_worker_retirement(&request), Ok(()));
}

#[test]
fn worker_retirement_admission_rejects_missing_handoff() {
    let mut request = valid_worker_retirement_admission();
    request.handoff_receipt_id = "";

    assert_eq!(
        admit_worker_retirement(&request),
        Err(WorkerRetirementRejection::MissingHandoffReceipt)
    );
}

#[test]
fn worker_retirement_admission_rejects_wrong_receipt_kind() {
    let mut request = valid_worker_retirement_admission();
    request.requested_receipt_kind = "worker.handoff.recorded";

    assert_eq!(
        admit_worker_retirement(&request),
        Err(WorkerRetirementRejection::InvalidRetirementReceiptKind)
    );
}

#[test]
fn temporal_loop_runner_requires_address() {
    let result = TemporalLoopRunner::connect(None);
    assert_eq!(result, Err(LoopRunnerAdapterError::MissingTemporalAddress));
}

#[test]
fn temporal_loop_runner_fails_closed_until_observed() {
    let result = TemporalLoopRunner::connect(Some("127.0.0.1:7233"));
    assert_eq!(
        result,
        Err(LoopRunnerAdapterError::PendingLiveRun {
            adapter: "TemporalLoopRunner",
            reason: "durable Temporal workflow execution has not been observed"
        })
    );
    assert_eq!(TemporalLoopRunner::adapter_name(), "TemporalLoopRunner");
}

#[test]
fn temporal_loop_runner_declares_durable_workflow_contract() {
    let contract = TemporalLoopRunner::evals_runner_contract();

    assert_eq!(contract.workflow_type, "evals_runner_agent");
    assert_eq!(contract.task_queue, "mdx-local-loop-runners");
    assert!(contract.namespace_required);
    assert!(contract.retry_policy_required);
    assert_eq!(contract.activity_timeout_seconds, 30);
    assert_eq!(contract.receipt_kind, "loop.triggered");
}

#[test]
fn deterministic_model_gateway_runs_local_eval_suite() {
    let gateway = DeterministicModelGateway;
    let trace = gateway
        .run_eval_suite("local_credentialing_smoke")
        .expect("deterministic model gateway");

    assert_eq!(gateway.gateway_name(), "deterministic_stub");
    assert_eq!(trace.cases, 3);
    assert_eq!(trace.score, 100);
    assert_eq!(trace.variant, "deterministic_local_v1");
    assert_eq!(trace.routing_strategy, "single_deterministic_stub");
    assert_eq!(trace.stream_contract, "normalized_model_stream_v1");
    assert_eq!(trace.stream_event_count, 4);
    assert_eq!(trace.terminal_event, "model_stream_completed");
    assert_eq!(trace.fallback_strategy, "local_first_fail_closed");
    assert_eq!(trace.fallback_provider, "none_local_single_provider");
    assert_eq!(trace.first_byte_latency_ms, 1);
    assert_eq!(trace.failover_slo_ms, 3000);
    assert!(trace.inference_id.contains("local_credentialing_smoke"));
    assert!(trace.passed);
}

#[test]
fn tensorzero_model_gateway_requires_gateway_url() {
    let result = TensorZeroModelGateway::connect(None);
    assert_eq!(
        result,
        Err(ModelGatewayAdapterError::MissingTensorZeroGatewayUrl)
    );
}

#[test]
fn tensorzero_model_gateway_fails_closed_until_observed() {
    let result = TensorZeroModelGateway::connect(Some("http://127.0.0.1:3000"));
    assert_eq!(
        result,
        Err(ModelGatewayAdapterError::PendingLiveRun {
            adapter: "TensorZeroModelGateway",
            reason: "live model gateway calls have not been observed"
        })
    );
    assert_eq!(
        TensorZeroModelGateway::adapter_name(),
        "TensorZeroModelGateway"
    );
}
#[test]
fn tensorzero_gateway_contract_requires_observability_and_feedback() {
    let contract = TensorZeroModelGateway::contract();

    assert_eq!(contract.provider, "TensorZero");
    assert!(contract.gateway_url_required);
    assert!(contract.observability_required);
    assert!(contract.feedback_required);
    assert!(contract.fallback_required);
    assert_eq!(contract.receipt_kind, "eval.suite.ran");
}
#[test]
fn mem0_memory_provider_requires_api_key() {
    let result = Mem0MemoryProvider::connect(None);
    assert_eq!(result, Err(MemoryProviderAdapterError::MissingMem0ApiKey));
}
#[test]
fn mem0_memory_provider_fails_closed_until_observed() {
    let result = Mem0MemoryProvider::connect(Some("mem0_test_key"));
    assert_eq!(
        result,
        Err(MemoryProviderAdapterError::PendingLiveRun {
            adapter: "Mem0MemoryProvider",
            reason: "live memory writes have not been observed through the consolidation gate"
        })
    );
    assert_eq!(Mem0MemoryProvider::adapter_name(), "Mem0MemoryProvider");
}

#[test]
fn outbox_event_links_tenant_and_source_receipt() {
    let mut ids = IdFactory::default();
    let mut ledger = Ledger::default();
    let receipt = ledger.append(
        &mut ids,
        &local_correlation(),
        "loop.signal.recorded",
        None,
        payload(&[("signal", "credential_ready")]),
    );
    let event = OutboxEvent::from_receipt(
        &mut ids,
        &receipt,
        "observatory.credential_ready",
        payload(&[("credential_status", "MINTED")]),
    );

    assert_eq!(event.tenant_id, receipt.tenant_id);
    assert_eq!(event.source_receipt_id, receipt.receipt_id);
    assert_eq!(event.topic, "observatory.credential_ready");
    assert!(!event.delivered);
}

#[test]
fn local_outbox_enqueues_and_marks_delivered() {
    let mut ids = IdFactory::default();
    let mut ledger = Ledger::default();
    let receipt = ledger.append(
        &mut ids,
        &local_correlation(),
        "loop.signal.recorded",
        None,
        payload(&[("signal", "credential_ready")]),
    );
    let event = OutboxEvent::from_receipt(
        &mut ids,
        &receipt,
        "observatory.credential_ready",
        BTreeMap::new(),
    );
    let event_id = event.event_id.clone();
    let mut outbox = InMemoryOutbox::default();

    outbox.enqueue(event);
    outbox.mark_delivered(&event_id).expect("mark delivered");

    assert_eq!(outbox.events().len(), 1);
    assert!(outbox.events()[0].delivered);
    assert_eq!(
        outbox.mark_delivered("missing"),
        Err("outbox event missing not found".to_string())
    );
}

#[test]
fn receipt_query_finds_receipts_by_contract_fields() {
    let mut ids = IdFactory::default();
    let mut ledger = Ledger::default();
    let policy_receipt = ledger.append(
        &mut ids,
        &local_correlation(),
        POLICY_DECISION_RECEIPT_KIND,
        Some("policy_test".to_string()),
        payload(&[("decision", "ALLOW")]),
    );
    let signal_receipt = ledger.append(
        &mut ids,
        &local_correlation(),
        "loop.signal.recorded",
        Some("policy_test".to_string()),
        payload(&[("signal", "credential_ready")]),
    );

    let query = ledger.query();

    assert_eq!(query.count(), 2);
    assert_eq!(
        query
            .by_id(&policy_receipt.receipt_id)
            .map(|entry| &entry.kind),
        Some(&POLICY_DECISION_RECEIPT_KIND.to_string())
    );
    assert!(query.by_id("missing_receipt").is_none());
    assert_eq!(query.by_kind("loop.signal.recorded"), vec![&signal_receipt]);
    assert_eq!(
        query.by_policy_decision("policy_test"),
        vec![&policy_receipt, &signal_receipt]
    );
    assert_eq!(
        query.receipt_ids(),
        vec![
            policy_receipt.receipt_id.as_str(),
            signal_receipt.receipt_id.as_str()
        ]
    );

    let mut restored = Ledger::default();
    restored
        .restore_entries(ledger.entries().to_vec())
        .expect("restore");
    let restored_query = restored.query();
    assert!(restored_query.by_id(&signal_receipt.receipt_id).is_some());
}

#[test]
fn receipt_evidence_formats_source_receipts_from_query() {
    let mut ids = IdFactory::default();
    let mut ledger = Ledger::default();
    let first = ledger.append(
        &mut ids,
        &local_correlation(),
        POLICY_DECISION_RECEIPT_KIND,
        None,
        BTreeMap::new(),
    );
    let second = ledger.append(
        &mut ids,
        &local_correlation(),
        "loop.signal.recorded",
        None,
        BTreeMap::new(),
    );

    let evidence = ReceiptEvidence::from_query(ledger.query());

    assert_eq!(
        evidence.receipt_ids,
        vec![first.receipt_id.clone(), second.receipt_id.clone()]
    );
    assert_eq!(
        evidence.source_list(),
        format!("{}, {}", first.receipt_id, second.receipt_id)
    );
}
#[test]
fn trace_event_carries_required_correlation_ids() {
    let correlation = local_correlation();
    let event = TraceEvent::from_correlation(&correlation, "loop.started");

    assert_eq!(event.tenant_id, correlation.tenant_id);
    assert_eq!(event.trace_id, correlation.trace_id);
    assert_eq!(event.actor_id, correlation.actor_id);
    assert_eq!(event.loop_id, correlation.loop_id);
    assert_eq!(event.workflow_id, correlation.workflow_id);
    assert_eq!(event.name, "loop.started");
}
#[test]
fn local_trace_exporter_records_events_without_credentials() {
    let mut exporter = LocalTraceExporter::default();
    let event = TraceEvent::from_correlation(&local_correlation(), "eval.suite.ran");

    exporter.export(event.clone()).expect("local trace export");

    assert_eq!(exporter.exporter_name(), "LocalTraceExporter");
    assert_eq!(exporter.events(), &[event]);
}
#[test]
fn opentelemetry_exporter_requires_endpoint() {
    let result = OpenTelemetryExporter::connect(None);
    assert_eq!(result, Err(ObservabilityAdapterError::MissingOtelEndpoint));
}

#[test]
fn opentelemetry_exporter_fails_closed_until_observed() {
    let result = OpenTelemetryExporter::connect(Some("http://127.0.0.1:4317"));
    assert_eq!(
        result,
        Err(ObservabilityAdapterError::PendingLiveRun {
            adapter: "OpenTelemetryExporter",
            reason: "live trace export has not been observed"
        })
    );
    assert_eq!(
        OpenTelemetryExporter::adapter_name(),
        "OpenTelemetryExporter"
    );
}

#[test]
fn live_substrate_statuses_cover_turn_on_boundaries() {
    let substrates = live_substrate_statuses()
        .iter()
        .map(|status| status.substrate)
        .collect::<Vec<_>>();

    assert_eq!(
        substrates,
        vec![
            "postgres",
            "temporal",
            "tensorzero",
            "mem0",
            "opentelemetry",
            "render"
        ]
    );
}

#[test]
fn live_substrate_statuses_track_observed_local_activation() {
    for status in live_substrate_statuses() {
        assert!(!status.local_adapter.is_empty());
        assert!(!status.live_adapter.is_empty());
        assert!(!status.turn_on_signal.is_empty());
        assert!(!status.blocking_rails.is_empty());
        assert!(!status.first_local_proof.is_empty());
        if status.substrate == "postgres" {
            assert_eq!(status.status, "LIVE-LOCAL");
            assert!(status.blocking_rails.contains(&"production-database-url"));
        } else {
            assert_eq!(status.status, "PENDING-LIVE-RUN");
            assert!(status.blocking_rails.contains(&"provider-turn-on-evidence"));
        }
    }
}

#[test]
fn provider_turn_on_evidence_receipts_name_live_provider_proofs() {
    let receipts = provider_turn_on_evidence_receipts();

    assert_eq!(receipts.len(), 6);
    assert!(receipts.iter().any(|receipt| {
        receipt.substrate == "postgres"
            && receipt.adapter == "PostgresStorage"
            && receipt.receipt_kind == "postgres.receipt.write.observed"
    }));
    assert!(receipts.iter().any(|receipt| {
        receipt.substrate == "temporal"
            && receipt.adapter == "TemporalLoopRunner"
            && receipt.receipt_kind == "temporal.workflow.observed"
    }));
    assert!(receipts.iter().any(|receipt| {
        receipt.substrate == "tensorzero"
            && receipt.adapter == "TensorZeroModelGateway"
            && receipt.receipt_kind == "tensorzero.inference.observed"
    }));
    assert!(receipts.iter().any(|receipt| {
        receipt.substrate == "mem0"
            && receipt.adapter == "Mem0MemoryProvider"
            && receipt.receipt_kind == "mem0.memory.write.observed"
    }));
    assert!(receipts.iter().any(|receipt| {
        receipt.substrate == "opentelemetry"
            && receipt.adapter == "OpenTelemetryExporter"
            && receipt.receipt_kind == "opentelemetry.trace.exported"
    }));
    assert!(receipts.iter().any(|receipt| {
        receipt.substrate == "render"
            && receipt.adapter == "render.yaml"
            && receipt.receipt_kind == "render.deployment.observed"
    }));
}

#[test]
fn postgres_provider_turn_on_evidence_receipt_cites_observed_transport() {
    let receipt =
        postgres_provider_turn_on_evidence_receipt(6, 72, 1, Some("hash_previous".to_string()));

    assert_eq!(receipt.kind, "postgres.receipt.write.observed");
    assert_eq!(receipt.loop_id.as_str(), "postgres_live_transport");
    assert_eq!(receipt.previous_hash.as_deref(), Some("hash_previous"));
    assert_eq!(
        receipt
            .payload
            .get("observed_ledger_receipts")
            .map(String::as_str),
        Some("72")
    );
    assert_eq!(
        receipt
            .payload
            .get("observed_chain_heads")
            .map(String::as_str),
        Some("1")
    );
}

#[test]
fn local_status_json_reports_machine_readable_substrate_state() {
    let json = local_status_json(8);

    assert!(json.contains("\"name\": \"mdx-native local\""));
    assert!(json.contains("\"migrations\": 8"));
    assert!(json.contains("\"mode\": \"deterministic-local\""));
    assert!(json.contains("\"substrate\": \"opentelemetry\""));
    assert!(json.contains("\"live_adapter\": \"OpenTelemetryExporter\""));
    assert!(json.contains("\"blocking_rails\": [\"production-database-url\""));
    assert!(json.contains("\"first_local_proof\": \"make live-turn-on-check\""));
    assert_eq!(json.matches("\"status\": \"PENDING-LIVE-RUN\"").count(), 5);
    assert!(json.contains("\"status\": \"LIVE-LOCAL\""));
}
#[test]
fn every_loop_transition_has_policy_decision_and_receipt() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    for transition in kernel.loop_transitions() {
        assert!(!transition.policy_decision_id.is_empty());
        assert!(
            kernel
                .ledger()
                .entries()
                .iter()
                .any(|entry| entry.receipt_id == transition.receipt_id)
        );
    }
}
#[test]
fn policy_decisions_have_ledger_receipts() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    for decision in kernel.policy_decisions() {
        let receipt_id = decision.receipt_id.as_ref().expect("policy receipt");
        assert!(
            kernel
                .ledger()
                .entries()
                .iter()
                .any(|entry| &entry.receipt_id == receipt_id)
        );
    }
}

#[test]
fn consequential_receipts_have_policy_decision_ids() {
    let mut kernel = MdxKernel::boot_local();
    LocalHarnessPlanRunner
        .run_plan_only(&mut kernel, &valid_harness_run_manifest())
        .expect("harness plan-only run");
    let mut harness_manifest = valid_harness_run_manifest();
    harness_manifest.budget_policy.max_tool_calls = 2;
    LocalHarnessReadToolPlane
        .read_virtual_artifact(
            &mut kernel,
            &harness_manifest,
            &HarnessReadToolRequest {
                path: "docs/HARNESS-RUNWAY.md",
                contents: "runway",
                allowed_read_roots: &["docs/"],
                max_output_bytes: 1024,
            },
        )
        .expect("harness safe read");
    LocalHarnessReadToolPlane
        .read_virtual_artifact(
            &mut kernel,
            &harness_manifest,
            &HarnessReadToolRequest {
                path: ".env",
                contents: "SECRET=value",
                allowed_read_roots: &[".env"],
                max_output_bytes: 1024,
            },
        )
        .expect("harness denied read");
    LocalHarnessReadToolPlane
        .search_virtual_artifact(
            &mut kernel,
            &harness_manifest,
            &HarnessSearchToolRequest {
                path: "docs/HARNESS-SAFE-TOOL-PLANE.md",
                contents: "safe tool plane",
                query: "safe",
                allowed_read_roots: &["docs/"],
                max_matches: 1,
                max_output_bytes: 1024,
            },
        )
        .expect("harness safe search");
    LocalHarnessReadToolPlane
        .search_virtual_artifact(
            &mut kernel,
            &harness_manifest,
            &HarnessSearchToolRequest {
                path: ".env",
                contents: "SECRET=value",
                query: "SECRET",
                allowed_read_roots: &[".env"],
                max_matches: 1,
                max_output_bytes: 1024,
            },
        )
        .expect("harness denied search");
    LocalHarnessReadToolPlane
        .list_virtual_artifacts(
            &mut kernel,
            &harness_manifest,
            &HarnessListToolRequest {
                path: "docs/",
                entries: &["HARNESS-SAFE-TOOL-PLANE.md"],
                allowed_read_roots: &["docs/"],
                max_entries: 1,
                max_output_bytes: 1024,
            },
        )
        .expect("harness safe list");
    LocalHarnessReadToolPlane
        .list_virtual_artifacts(
            &mut kernel,
            &harness_manifest,
            &HarnessListToolRequest {
                path: ".mdx-local/",
                entries: &["evidence-index.json"],
                allowed_read_roots: &[".mdx-local/"],
                max_entries: 1,
                max_output_bytes: 1024,
            },
        )
        .expect("harness denied list");
    let patch_manifest = valid_harness_patch_manifest();
    LocalHarnessReadToolPlane
        .mediate_virtual_patch(
            &mut kernel,
            &patch_manifest,
            &HarnessPatchToolRequest {
                path: "crates/mdx-core/src/lib.rs",
                patch: "diff --git a/crates/mdx-core/src/lib.rs b/crates/mdx-core/src/lib.rs\n@@\n+safe patch proposal\n",
                max_patch_bytes: 512,
                max_output_bytes: 128,
            },
        )
        .expect("harness safe patch");
    LocalHarnessReadToolPlane
        .mediate_virtual_patch(
            &mut kernel,
            &patch_manifest,
            &HarnessPatchToolRequest {
                path: ".env",
                patch: "diff --git a/.env b/.env\n@@\n+SECRET=value\n",
                max_patch_bytes: 512,
                max_output_bytes: 128,
            },
        )
        .expect("harness denied patch");
    kernel.run_evals_runner_agent().expect("loop run");
    kernel
        .run_charter_attestation_agent()
        .expect("charter loop run");
    kernel
        .run_forge_orchestrator_agent()
        .expect("forge loop run");
    kernel
        .run_product_shaping_agent()
        .expect("product shaping loop run");
    assert!(kernel.assemble_local_context(local_ctx_request()).is_ok());
    kernel.run_talent_autonomy_agent().expect("talent loop run");
    let spawn_receipt_id = kernel.ledger().query().by_kind("worker.spawn_requested")[0]
        .receipt_id
        .clone();
    let credential_check_receipt_id = kernel.ledger().query().by_kind("worker.credential.checked")
        [0]
    .receipt_id
    .clone();
    kernel
        .run_local_worker_runtime(WorkerRuntimeRequest {
            spawn_receipt_id,
            credential_check_receipt_id,
            output_artifacts: vec!["artifact_patch".to_string()],
            verification_evidence: vec!["ci.verification.observed".to_string()],
            summary: "local worker completed bounded task".to_string(),
            next_owner: "forge_orchestrator_agent".to_string(),
        })
        .expect("local worker runtime");
    let treasury_correlation = CorrelationIds {
        tenant_id: TenantId::new("tenant_local"),
        trace_id: TraceId::new("trace_treasury_test"),
        actor_id: ActorId::new("evals_runner_agent"),
        loop_id: LoopId::new("evals_runner_agent"),
        workflow_id: WorkflowId::new("workflow_treasury_test"),
    };
    kernel
        .authorize_treasury(
            &treasury_correlation,
            1,
            "local_test_counterparty",
            "prove policy coverage",
        )
        .expect("treasury authorization");
    let external_correlation = local_correlation();
    let external_run_id = "harness_external_machine_policy_test";
    for (action, transition, kind) in [
        (
            ActionKind::AdmitHarnessExternalMachineRequest,
            "EXTERNAL_MACHINE_REQUEST_ADMITTED",
            "harness.external_machine.request.admitted",
        ),
        (
            ActionKind::DenyHarnessExternalMachineAdapter,
            "EXTERNAL_MACHINE_ADAPTER_DENIED",
            "harness.external_machine.adapter.denied",
        ),
        (
            ActionKind::ImportHarnessExternalMachineEvidence,
            "EXTERNAL_MACHINE_EVIDENCE_IMPORTED",
            "harness.external_machine.evidence.imported",
        ),
        (
            ActionKind::RecordHarnessExternalMachineVerdict,
            "EXTERNAL_MACHINE_VERDICT_RECORDED",
            "harness.external_machine.verdict.recorded",
        ),
    ] {
        let decision = kernel.decide_with_receipt(&external_correlation, action);
        kernel.transition_receipt(
            external_run_id,
            transition,
            &external_correlation,
            &decision,
            kind,
            payload(&[("policy_coverage", "true")]),
        );
    }
    kernel
        .classify_forge_work_local(ForgeWorkClassificationRequest {
            tenant_id: "local_tenant",
            actor_id: "local_user",
            actor_role: "owner",
            classification_id: "policy_coverage_classification",
            ask: "Refactor Forge into a long-horizon mission with checkpoints.",
            repo: "MDx",
            declared_checks: "make verify-budget",
        })
        .expect("forge work classification policy coverage");
    kernel
        .admit_forge_long_horizon_mission_local(ForgeLongHorizonMissionAdmission {
            tenant_id: "local_tenant",
            actor_id: "local_user",
            actor_role: "owner",
            mission_id: "policy_coverage_mission",
            goal: "Prove long-horizon mission policy coverage",
            non_goals: "no live providers",
            constraints: "local proof only",
            done_when: "policy coverage receipts exist",
            allowed_write_scope: "crates/mdx-core/src/",
            blocked_paths: ".env,.git/",
            validation_commands: "make verify-budget",
            model_policy: "local deterministic only",
            provider_allowlist: "none",
            fleet_width: 2,
            max_runtime_ms: 120_000,
            max_cost_cents: 1,
            checkpoint_cadence_minutes: 15,
        })
        .expect("forge long-horizon mission policy coverage");
    kernel
        .run_forge_fleet_eval_dry_run_local("policy_coverage_dry_run")
        .expect("forge fleet eval dry run policy coverage");
    kernel
        .approve_forge_fleet_eval_live_run_local(ForgeFleetEvalLiveRunApproval {
            tenant_id: "local_tenant",
            actor_id: "local_user",
            actor_role: "owner",
            approval_id: "policy_coverage_live_approval",
            provider_allowlist: "anthropic,gemini,xai,aws_bedrock",
            max_spend_cents: 1,
            max_tasks: 1,
            max_parallel_agents: 1,
            artifact_retention_policy: "local quarantine",
            redaction_policy: "presence only",
            stop_conditions: "any missing credential",
        })
        .expect("forge fleet eval live-run approval policy coverage");
    let identity = GovernedWriteIdentity::local_demo("local_user");
    let tenant = "local_tenant";
    let actor = "local_user";
    kernel
        .register_mobile_device(
            MobileDeviceRegistration {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                device_id: "policy_coverage_iphone",
                platform: "ios",
                display_name: "Policy Coverage iPhone",
                public_key: "policy-coverage-p256-public-key",
                public_key_thumbprint: "policy-coverage-device-thumbprint",
                registered_at_epoch: 1,
            },
            &identity,
        )
        .expect("mobile device policy coverage");
    kernel
        .register_mobile_host(
            MobileHostRegistration {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                host_id: "policy_coverage_mac",
                display_name: "Policy Coverage Mac",
                public_key_thumbprint: "policy-coverage-host-thumbprint",
                registered_at_epoch: 2,
            },
            &identity,
        )
        .expect("mobile host policy coverage");
    kernel
        .record_mobile_pairing(
            MobilePairingRegistration {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                device_id: "policy_coverage_iphone",
                host_id: "policy_coverage_mac",
                paired_at_epoch: 3,
            },
            &identity,
        )
        .expect("mobile pairing policy coverage");
    kernel
        .revoke_mobile_device(tenant, actor, "policy_coverage_iphone", 4, &identity)
        .expect("mobile device revocation policy coverage");
    kernel
        .revoke_mobile_host(tenant, actor, "policy_coverage_mac", 5, &identity)
        .expect("mobile host revocation policy coverage");
    kernel
        .approve_auth_user_admission(auth_user_admission_policy_coverage_request())
        .expect("auth user admission policy coverage");

    let expected_kinds = CONSEQUENTIAL_RECEIPT_KINDS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let observed_kinds = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|entry| expected_kinds.contains(entry.kind.as_str()))
        .map(|entry| entry.kind.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(observed_kinds, expected_kinds);

    for receipt in kernel
        .ledger()
        .entries()
        .iter()
        .filter(|entry| expected_kinds.contains(entry.kind.as_str()))
    {
        let policy_decision_id = receipt
            .policy_decision_id
            .as_ref()
            .expect("consequential receipt policy decision");
        assert!(
            kernel
                .policy_decisions()
                .iter()
                .any(|decision| &decision.policy_decision_id == policy_decision_id),
            "missing policy decision for receipt {}",
            receipt.receipt_id
        );
    }
}

#[test]
fn evals_runner_agent_stays_within_local_budget() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_evals_runner_agent().expect("loop run");
    let budget = local_loop_budget("evals_runner_agent").expect("evals budget");

    assert_eq!(report.loop_id, budget.loop_id);
    assert!(report.receipts.len() <= budget.max_receipts);
    assert!(kernel.policy_decisions().len() <= budget.max_policy_decisions);
    assert_eq!(kernel.loop_transitions().len(), budget.required_transitions);
    assert!(kernel.outbox_events().len() <= budget.max_outbox_events);
    assert!(kernel.credentials().len() <= budget.max_credentials);
    assert!(kernel.memory_records().len() <= budget.max_memory_records);
    assert!(kernel.charter_records().len() <= budget.max_charter_records);
}

#[test]
fn aegis_scanner_agent_stays_within_local_budget() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_aegis_scanner_agent().expect("aegis loop run");
    let budget = local_loop_budget("aegis_scanner_agent").expect("aegis budget");
    assert_eq!(report.loop_id, budget.loop_id);
    assert!(report.receipts.len() <= budget.max_receipts);
    assert!(kernel.policy_decisions().len() <= budget.max_policy_decisions);
    assert_eq!(kernel.loop_transitions().len(), budget.required_transitions);
    assert!(kernel.outbox_events().len() <= budget.max_outbox_events);
    assert!(kernel.credentials().len() <= budget.max_credentials);
    assert!(kernel.memory_records().len() <= budget.max_memory_records);
    assert!(kernel.charter_records().len() <= budget.max_charter_records);
}

#[test]
fn charter_attestation_agent_stays_within_local_budget() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_charter_attestation_agent()
        .expect("charter loop run");
    let budget = local_loop_budget("charter_attestation_agent").expect("charter budget");
    assert_eq!(report.loop_id, budget.loop_id);
    assert!(report.receipts.len() <= budget.max_receipts);
    assert!(kernel.policy_decisions().len() <= budget.max_policy_decisions);
    assert_eq!(kernel.loop_transitions().len(), budget.required_transitions);
    assert!(kernel.outbox_events().len() <= budget.max_outbox_events);
    assert!(kernel.credentials().len() <= budget.max_credentials);
    assert!(kernel.memory_records().len() <= budget.max_memory_records);
    assert!(kernel.charter_records().len() <= budget.max_charter_records);
}

#[test]
fn forge_orchestrator_agent_stays_within_local_budget() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_forge_orchestrator_agent()
        .expect("forge loop run");
    let budget = local_loop_budget("forge_orchestrator_agent").expect("forge budget");
    assert_eq!(report.loop_id, budget.loop_id);
    assert!(report.receipts.len() <= budget.max_receipts);
    assert!(kernel.policy_decisions().len() <= budget.max_policy_decisions);
    assert_eq!(kernel.loop_transitions().len(), budget.required_transitions);
    assert!(kernel.outbox_events().len() <= budget.max_outbox_events);
    assert!(kernel.credentials().len() <= budget.max_credentials);
    assert!(kernel.memory_records().len() <= budget.max_memory_records);
    assert!(kernel.charter_records().len() <= budget.max_charter_records);
}

#[test]
fn talent_autonomy_agent_stays_within_local_budget() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_talent_autonomy_agent().expect("talent loop run");
    let budget = local_loop_budget("talent_autonomy_agent").expect("talent budget");
    assert_eq!(report.loop_id, budget.loop_id);
    assert!(report.receipts.len() <= budget.max_receipts);
    assert!(kernel.policy_decisions().len() <= budget.max_policy_decisions);
    assert_eq!(kernel.loop_transitions().len(), budget.required_transitions);
    assert!(kernel.outbox_events().len() <= budget.max_outbox_events);
    assert!(kernel.credentials().len() <= budget.max_credentials);
    assert!(kernel.memory_records().len() <= budget.max_memory_records);
    assert!(kernel.charter_records().len() <= budget.max_charter_records);
}

#[test]
fn product_shaping_agent_stays_within_local_budget() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_product_shaping_agent()
        .expect("product shaping loop run");
    let budget = local_loop_budget("product_shaping_agent").expect("product shaping budget");
    assert_eq!(report.loop_id, budget.loop_id);
    assert!(report.receipts.len() <= budget.max_receipts);
    assert!(kernel.policy_decisions().len() <= budget.max_policy_decisions);
    assert_eq!(kernel.loop_transitions().len(), budget.required_transitions);
    assert!(kernel.outbox_events().len() <= budget.max_outbox_events);
    assert!(kernel.credentials().len() <= budget.max_credentials);
    assert!(kernel.memory_records().len() <= budget.max_memory_records);
    assert!(kernel.charter_records().len() <= budget.max_charter_records);
}

#[test]
fn aegis_finding_links_scan_and_charter_receipts() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_aegis_scanner_agent().expect("aegis loop run");
    let finding = kernel
        .ledger()
        .entries()
        .iter()
        .find(|entry| entry.kind == "aegis.finding.classified")
        .expect("finding receipt");
    let scan_receipt_id = finding
        .payload
        .get("source_receipt_id")
        .expect("source receipt id");
    assert!(kernel.ledger().query().by_id(scan_receipt_id).is_some());

    let charter = kernel.charter_records().last().expect("charter");
    assert_eq!(charter.source_receipt_id, finding.receipt_id);
    assert!(kernel.ledger().query().by_id(&charter.receipt_id).is_some());
}

#[test]
fn credential_mint_requires_eval_verdict() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let credential = kernel.credentials().last().expect("credential");
    assert!(
        kernel
            .eval_verdicts()
            .iter()
            .any(|verdict| verdict.eval_verdict_id == credential.eval_verdict_id && verdict.passed)
    );
}

#[test]
fn memory_writes_pass_through_consolidation_gate() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let memory = kernel.memory_records().last().expect("memory");
    assert_eq!(memory.consolidation_decision, ConsolidationDecision::Retain);
    assert_eq!(memory.provenance.driver_id, LOCAL_MEMORY_DRIVER.driver_id);
    assert_eq!(memory.provenance.provider, "InMemoryProvider");
    assert_eq!(
        memory.provenance.consolidation_gate,
        "local_consolidation_gate_v1"
    );
    assert_eq!(memory.provenance.source_receipt_kind, "credential.minted");
    assert_eq!(memory.provenance.temporal_status, "event_completed");
    assert!(
        kernel
            .ledger()
            .entries()
            .iter()
            .any(|entry| entry.receipt_id == memory.source_receipt_id)
    );
    assert!(
        kernel
            .ledger()
            .entries()
            .iter()
            .any(|entry| entry.receipt_id == memory.provenance.gate_receipt_id)
    );
}

#[test]
fn model_gateway_trace_is_recorded_on_eval_receipt() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let receipt = kernel
        .ledger()
        .entries()
        .iter()
        .find(|entry| entry.kind == "eval.suite.ran")
        .expect("eval suite receipt");

    for (key, expected) in [
        ("model_gateway", "deterministic_stub"),
        ("model_gateway_variant", "deterministic_local_v1"),
        ("model_gateway_driver", "local_model_gateway"),
        ("model_gateway_provider", "DeterministicModelGateway"),
        ("model_gateway_model_id", "deterministic_local_v1"),
        ("model_gateway_routing", "single_deterministic_stub"),
        (
            "model_gateway_stream_contract",
            "normalized_model_stream_v1",
        ),
        ("model_gateway_stream_event_count", "4"),
        ("model_gateway_terminal_event", "model_stream_completed"),
        ("model_gateway_fallback_strategy", "local_first_fail_closed"),
        (
            "model_gateway_fallback_provider",
            "none_local_single_provider",
        ),
        ("model_gateway_first_byte_latency_ms", "1"),
        ("model_gateway_failover_slo_ms", "3000"),
        ("provider_call_allowed", "false"),
    ] {
        assert_eq!(receipt.payload.get(key).map(String::as_str), Some(expected));
    }
    assert!(receipt.payload.contains_key("model_gateway_inference_id"));
}

#[test]
fn evals_runner_enqueues_receipt_linked_outbox_signal() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let event = kernel.outbox_events().last().expect("outbox event");
    assert_eq!(event.topic, "observatory.credential_ready");
    assert_eq!(event.tenant_id, TenantId::new("tenant_local"));
    assert_eq!(
        event.payload.get("credential_status").map(String::as_str),
        Some("MINTED")
    );
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&event.source_receipt_id)
            .is_some()
    );
}

#[test]
fn charter_records_link_evidence_and_receipt() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");

    let charter = kernel.charter_records().last().expect("charter");
    assert!(
        kernel
            .ledger()
            .entries()
            .iter()
            .any(|entry| entry.receipt_id == charter.source_receipt_id)
    );
    assert!(
        kernel
            .ledger()
            .entries()
            .iter()
            .any(|entry| entry.receipt_id == charter.receipt_id)
    );
}

#[test]
fn treasury_authority_exists_for_economic_actions() {
    let mut kernel = MdxKernel::boot_local();
    let authorization = kernel
        .authorize_treasury(
            &local_correlation(),
            100,
            "local_stub_counterparty",
            "prove transaction-scoped authority",
        )
        .expect("treasury authorization");

    assert_eq!(authorization.status, "ACTIVE");
    assert_eq!(authorization.max_amount_cents, 100);
    assert!(
        kernel
            .ledger()
            .entries()
            .iter()
            .any(|entry| entry.receipt_id == authorization.receipt_id)
    );
}

#[test]
fn role_views_read_declared_sources() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("loop run");
    let view = kernel.observatory_view();

    assert!(view.contains("surface: observatory"));
    for source in OBSERVATORY_ROLE_VIEW.declared_sources {
        assert!(view.contains(source.as_str()), "{}", source.as_str());
    }
}

#[test]
fn read_models_are_typed_before_rendering() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel.run_evals_runner_agent().expect("loop run");

    let observatory = kernel.observatory_read_model();
    assert_eq!(observatory.surface, "observatory");
    assert_eq!(
        observatory.latest_run_label(),
        "evals_runner_agent COMPLETED"
    );
    assert_eq!(observatory.receipt_count, report.receipts.len());
    assert_eq!(observatory.receipt_evidence.receipt_ids, report.receipts);
    assert_eq!(observatory.policy_decision_count, 7);
    assert_eq!(observatory.role_modes.len(), 8);
    assert!(observatory.declared_sources.contains(&"ledger_entries"));
    let observatory_json = observatory.render_json();
    assert!(observatory_json.contains("\"surface\": \"observatory\""));
    assert!(observatory_json.contains("\"latest_run\""));

    let concierge = kernel.concierge_read_model();
    assert_eq!(
        concierge.suite_id.as_deref(),
        Some("local_credentialing_smoke")
    );
    assert_eq!(concierge.score, Some(100));
    assert_eq!(concierge.credential_status.as_deref(), Some("MINTED"));
    assert_eq!(concierge.receipt_evidence.receipt_ids, report.receipts);
    assert_eq!(
        observatory.receipt_evidence.receipt_ids,
        concierge.receipt_evidence.receipt_ids
    );
    assert!(concierge.render_text().contains("Source receipts:"));
    let concierge_json = concierge.render_json();
    assert!(concierge_json.contains("\"suite_id\": \"local_credentialing_smoke\""));
    assert!(concierge_json.contains("\"credential_status\": \"MINTED\""));
    assert!(concierge_json.contains("\"receipt_ids\""));

    let twin = kernel.twin_text_response("twin_advisor");
    assert_eq!(twin.companion_id, "twin_advisor");
    assert_eq!(twin.role, "advisor");
    assert_eq!(twin.runtime_status, "PENDING-LIVE-RUN");
    assert_eq!(twin.receipt_evidence.receipt_ids, report.receipts);
    assert!(twin.answer.contains("No action is available from Twin."));
    assert!(
        twin.world_model_sources
            .contains(&"generated/world-model/mdx-local-world-model.json")
    );
    let twin_json = twin.render_json();
    assert!(twin_json.contains("\"companion_id\": \"twin_advisor\""));
    assert!(twin_json.contains("\"runtime_status\": \"PENDING-LIVE-RUN\""));
    assert!(twin_json.contains("\"receipt_ids\""));
    assert!(twin_json.contains("\"world_model_sources\""));

    let strategy = kernel.strategy_ratification_response();
    assert_eq!(strategy.proposal_id, "strategy_local_ratification_001");
    assert_eq!(strategy.runtime_status, "DECLARED-LOCAL-READ-SURFACE");
    assert!(strategy.ratification_required);
    assert_eq!(strategy.receipt_evidence.receipt_ids, report.receipts);
    assert!(strategy.blocked_actions.contains(&"allocate_live_budget"));
    assert!(strategy.blocked_actions.contains(&"spawn_worker"));
    assert!(
        strategy
            .source_contracts
            .contains(&"generated/strategy/strategy-ratification-surface.json")
    );
    let strategy_text = strategy.render_text();
    assert!(strategy_text.contains("Ratification required: true"));
    assert!(strategy_text.contains("Blocked actions:"));
    let strategy_json = strategy.render_json();
    assert!(strategy_json.contains("\"proposal_id\": \"strategy_local_ratification_001\""));
    assert!(strategy_json.contains("\"runtime_status\": \"DECLARED-LOCAL-READ-SURFACE\""));
    assert!(strategy_json.contains("\"ratification_required\": true"));
    assert!(strategy_json.contains("\"receipt_ids\""));
    assert!(strategy_json.contains("\"source_contracts\""));
}

#[test]
fn read_model_json_rendering_escapes_strings() {
    let observatory = ObservatoryReadModel {
        surface: "observatory",
        latest_run: Some(LoopRunSummary {
            loop_id: "loop\"id".to_string(),
            status: "DONE\\OK".to_string(),
        }),
        receipt_evidence: ReceiptEvidence {
            receipt_ids: vec!["receipt\"1".to_string(), "receipt\\2".to_string()],
        },
        receipt_count: 0,
        policy_decision_count: 0,
        eval_verdict_count: 0,
        credential_count: 0,
        charter_record_count: 0,
        memory_record_count: 0,
        role_modes: vec!["Leader", "Risk\tMode"],
        declared_sources: vec!["ledger_entries", "source\nwith newline"],
    };

    let observatory_json = observatory.render_json();
    assert!(observatory_json.contains(r#"loop\"id"#));
    assert!(observatory_json.contains(r#"DONE\\OK"#));
    assert!(observatory_json.contains(r#"receipt\"1"#));
    assert!(observatory_json.contains(r#"receipt\\2"#));
    assert!(observatory_json.contains(r#"Risk\tMode"#));
    assert!(observatory_json.contains(r#"source\nwith newline"#));

    let concierge = ConciergeReadModel {
        suite_id: Some("suite\"x\\y\nz".to_string()),
        score: Some(99),
        credential_id: Some("credential\t1".to_string()),
        credential_status: Some("MINTED\rOK".to_string()),
        receipt_evidence: ReceiptEvidence {
            receipt_ids: vec!["receipt\"1".to_string(), "receipt\\2".to_string()],
        },
    };

    let concierge_json = concierge.render_json();
    assert!(concierge_json.contains(r#"suite\"x\\y\nz"#));
    assert!(concierge_json.contains(r#"credential\t1"#));
    assert!(concierge_json.contains(r#"MINTED\rOK"#));
    assert!(concierge_json.contains(r#"receipt\"1"#));
    assert!(concierge_json.contains(r#"receipt\\2"#));

    let twin = TwinTextResponse {
        companion_id: "twin_advisor",
        role: "advisor",
        runtime_status: "PENDING-LIVE-RUN",
        answer: "answer \"with\" \n evidence".to_string(),
        receipt_evidence: ReceiptEvidence {
            receipt_ids: vec!["receipt\"1".to_string(), "receipt\\2".to_string()],
        },
        world_model_sources: vec!["source\"1", "source\\2"],
    };
    let twin_json = twin.render_json();
    assert!(twin_json.contains(r#"answer \"with\" \n evidence"#));
    assert!(twin_json.contains(r#"receipt\"1"#));
    assert!(twin_json.contains(r#"receipt\\2"#));
    assert!(twin_json.contains(r#"source\"1"#));
    assert!(twin_json.contains(r#"source\\2"#));

    let strategy = StrategyRatificationResponse {
        proposal_id: "strategy_local_ratification_001",
        runtime_status: "DECLARED-LOCAL-READ-SURFACE",
        question: "question \"with\" \n evidence",
        options: vec!["option\"1", "option\\2"],
        why_now: "why \"now\" \n evidence".to_string(),
        blocked_actions: vec!["blocked\"1", "blocked\\2"],
        ratification_required: true,
        receipt_evidence: ReceiptEvidence {
            receipt_ids: vec!["receipt\"1".to_string(), "receipt\\2".to_string()],
        },
        source_contracts: vec!["source\"1", "source\\2"],
    };
    let strategy_json = strategy.render_json();
    assert!(strategy_json.contains(r#"question \"with\" \n evidence"#));
    assert!(strategy_json.contains(r#"why \"now\" \n evidence"#));
    assert!(strategy_json.contains(r#"option\"1"#));
    assert!(strategy_json.contains(r#"blocked\\2"#));
    assert!(strategy_json.contains(r#"receipt\"1"#));
    assert!(strategy_json.contains(r#"source\\2"#));
}

#[test]
fn json_string_literal_escapes_json_control_characters() {
    assert_eq!(
        json_string_literal("quote\"slash\\line\ncarriage\rtab\t"),
        r#""quote\"slash\\line\ncarriage\rtab\t""#
    );
}

#[test]
fn observatory_role_view_declares_all_human_edge_modes() {
    assert_eq!(OBSERVATORY_ROLE_VIEW.role_modes.len(), 8);
    for role in [
        RoleMode::Leader,
        RoleMode::Regulator,
        RoleMode::Risk,
        RoleMode::Advisor,
        RoleMode::Member,
        RoleMode::Engineer,
        RoleMode::Operator,
        RoleMode::Product,
    ] {
        assert!(OBSERVATORY_ROLE_VIEW.role_modes.contains(&role));
    }
}

#[test]
fn observatory_role_view_does_not_read_private_internals() {
    let allowed_sources = OBSERVATORY_ROLE_VIEW
        .declared_sources
        .iter()
        .map(DeclaredSource::as_str)
        .collect::<Vec<_>>();
    for forbidden in ["storage", "ids", "policy", "memory_provider"] {
        assert!(!allowed_sources.contains(&forbidden));
    }
}
#[test]
fn local_read_surfaces_declare_receipt_backed_sources() {
    let surfaces = local_read_surfaces();
    assert_eq!(surfaces.len(), 4);
    for surface in surfaces {
        assert!(surface.route.starts_with('/'));
        assert!(surface.receipt_evidence_required);
        assert!(
            surface
                .declared_sources
                .contains(&DeclaredSource::LedgerEntries)
        );
    }
    let concierge = surfaces
        .iter()
        .find(|surface| surface.surface == "concierge")
        .expect("concierge read surface");
    assert_eq!(
        concierge.response_schema_path,
        "generated/response-schemas/concierge-read-model.schema.json"
    );
    assert!(
        concierge
            .declared_sources
            .contains(&DeclaredSource::EvalVerdicts)
    );
    assert!(
        concierge
            .declared_sources
            .contains(&DeclaredSource::AgentCredentials)
    );
    let pages = surfaces
        .iter()
        .find(|surface| surface.surface == "pages")
        .expect("pages read surface");
    assert_eq!(pages.route, "/pages.json");
    assert!(
        pages
            .declared_sources
            .contains(&DeclaredSource::PagesProjections)
    );
    let message = surfaces
        .iter()
        .find(|surface| surface.surface == "message")
        .expect("message read surface");
    assert_eq!(message.route, "/messages/threads.json");
    assert!(
        message
            .declared_sources
            .contains(&DeclaredSource::MessageProjections)
    );
}
#[test]
fn pages_onboarding_documents_are_shared_receipt_backed_contracts() {
    let documents = pages::pages_onboarding_documents();
    assert_eq!(documents.len(), 9);
    assert!(pages::pages_onboarding_document_by_id("page_ui_app_experience_upgrade").is_some());
    assert!(pages::pages_onboarding_document_by_id("page_activation_runway").is_some());
    assert!(pages::pages_onboarding_document_by_id("page_harness_safe_tool_plane").is_some());
    for document in documents {
        assert!(document.document_id.starts_with("page_"));
        assert_eq!(document.author_actor_id, "agent:codex");
        assert!(!document.source_receipt_ids.is_empty());
        assert_eq!(
            pages::pages_onboarding_document_by_id(document.document_id),
            Some(document)
        );
    }
}
#[test]
fn local_json_routes_declare_response_schema_paths() {
    let assert_schema = |path, schema_path| {
        let route = local_http_routes()
            .iter()
            .find(|route| route.local_path == path)
            .expect("declared local route");
        assert_eq!(route.response_schema_path, Some(schema_path));
    };
    let status_schema = "generated/response-schemas/status.schema.json";
    assert_schema("/status.json", status_schema);
    assert_schema(
        "/local/index.json",
        "generated/response-schemas/local-api-index-response.schema.json",
    );
    let evidence_schema = "generated/response-schemas/local-evidence-index-response.schema.json";
    assert_schema("/local/evidence-index.json", evidence_schema);
    let auth_schema = "generated/response-schemas/auth-session-response.schema.json";
    assert_schema("/local/auth-session.json", auth_schema);
    assert_schema(
        "/local/activation-summary.json",
        "generated/response-schemas/local-activation-summary-response.schema.json",
    );
    assert_schema(
        "/local/activation-report.json",
        "generated/response-schemas/local-activation-report-response.schema.json",
    );
    let dogfood_schema = "generated/response-schemas/local-dogfood-proof-response.schema.json";
    assert_schema("/local/dogfood-proof.json", dogfood_schema);
    let handoff = "generated/response-schemas/local-operational-handoff-response.schema.json";
    assert_schema("/local/operational-handoff.json", handoff);
    assert_schema(
        "/observatory.json",
        "generated/response-schemas/observatory-read-model.schema.json",
    );
    assert_schema(
        "/concierge.json",
        "generated/response-schemas/concierge-read-model.schema.json",
    );
    let twin_schema = "generated/response-schemas/twin-text-response.schema.json";
    assert_schema("/twin.json", twin_schema);
    assert_schema(
        "/strategy.json",
        "generated/response-schemas/strategy-ratification-response.schema.json",
    );
    assert_schema(
        "/product.json",
        "generated/response-schemas/product-ratification-response.schema.json",
    );
    assert_schema(
        "/receipts.json",
        "generated/response-schemas/receipt-list.schema.json",
    );
    assert_schema(
        "/receipts/{receipt_id}",
        "generated/response-schemas/receipt.schema.json",
    );
    assert_schema(
        "/memory/brain-runtime.json",
        "generated/response-schemas/memory-brain-runtime-response.schema.json",
    );
}
#[test]
fn migration_contracts_have_tenant_ids_and_rls() {
    let report = validate_migration_contracts().expect("migration contracts");
    assert_eq!(report.migration_count, 48);
    assert!(report.tenant_owned_tables >= TENANT_OWNED_TABLES.len());
    assert_eq!(report.rls_enabled_tables, RLS_TABLES.len());
    // +30 in both: the live-path migration declares the dynamic persist-plane
    // policy once, the ctx vector migration re-declares its access policy, and
    // the post-live-path Memory Brain, flywheel, Model Fabric, and ledger archive
    // tables declare explicit persist-plane policies.
    assert_eq!(
        report.policy_definitions,
        RLS_TABLES.len() + RESOURCE_AWARE_RLS_OVERRIDES.len() + 30
    );
    assert_eq!(report.policy_drop_guards, report.policy_definitions);
}

#[test]
fn rls_v2_overrides_deny_same_tenant_access_without_actor_context() {
    // The same-tenant negative posture, asserted structurally: each sensitive
    // surface's authoritative policy requires actor or resource context, so a
    // request that only matches the tenant is denied. A tenant-only policy on any
    // of these would be a leak. Resource-awareness shows up one of three ways:
    // a direct actor read (mdx.actor_id), the Pages visibility function
    // (mdx_pages_page_visible), or inheritance from a parent via EXISTS.
    let joined = migration_sources().join("\n");
    for table in RESOURCE_AWARE_RLS_OVERRIDES {
        let marker = format!("CREATE POLICY {table}_tenant_access ON {table}");
        let last = joined
            .rfind(&marker)
            .unwrap_or_else(|| panic!("{table} has no resource-aware policy"));
        let section = &joined[last..];
        let end = section.find(';').unwrap_or(section.len());
        let policy = &section[..end];
        assert!(
            policy.contains("mdx.tenant_id"),
            "{table} policy must still scope by tenant"
        );
        assert!(
            policy.contains("mdx.actor_id")
                || policy.contains("mdx.actor_role")
                || policy.contains("mdx_pages_page_visible")
                || policy.contains("mdx_receipt_")
                || policy.contains("EXISTS ("),
            "{table} policy must add actor/resource context (same-tenant access alone must be denied)"
        );
    }
    // And the contract validator agrees the whole set is resource-aware.
    validate_migration_contracts().expect("rls override contract holds");
}
