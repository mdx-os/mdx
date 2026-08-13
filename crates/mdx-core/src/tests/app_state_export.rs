use super::*;

#[test]
fn direct_product_bet_mints_the_signal_receipt_required_by_postgres() {
    let mut kernel = MdxKernel::boot_local();
    let identity = GovernedWriteIdentity::local_demo("founder");
    kernel
        .save_product_bet_draft_local_with_identity(
            ProductBetDraft {
                tenant_id: "tenant_test",
                actor_id: "founder",
                bet_id: "",
                bet: "Prove the beta survives a restart",
                for_whom: "",
                success_metric: "",
                kill_condition: "",
                direction_ref: "",
                slice: "",
                slice_not: "",
                signal_ref: "",
            },
            &identity,
        )
        .expect("direct bet");

    let query = kernel.ledger().query();
    let signal = query.by_kind("product.signal.ingested")[0];
    let shaped = query.by_kind("product.bet.shaped")[0];
    assert_eq!(
        shaped.payload.get("source_signal_receipt_id"),
        Some(&signal.receipt_id)
    );
    assert_eq!(shaped.payload.get("signal_ref"), Some(&signal.receipt_id));
    let sql = render_postgres_app_state_export_sql(kernel.ledger().entries(), &[]);
    assert!(sql.contains(&sql_string_literal(&signal.receipt_id)));
}

#[test]
fn current_strategy_decision_kind_exports_its_durable_snapshot() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .save_strategy_ratification_decision_local(StrategyRatificationDecision {
            tenant_id: "tenant_test",
            actor_id: "founder",
            proposal_id: "strategy_local_ratification_001",
            decision: "request_more_evidence",
            reason: "Finish the restart proof first.",
        })
        .expect("strategy decision");

    let sql = render_postgres_app_state_export_sql(kernel.ledger().entries(), &[]);
    assert!(sql.contains("INSERT INTO strategy_ratification_snapshots"));
    assert!(sql.contains(&sql_string_literal(&report.decision_receipt_id)));
    assert!(sql.contains("MORE_EVIDENCE_REQUESTED_RATIFICATION_OPEN"));
}

#[test]
fn app_state_export_renders_first_governed_write_tables() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("seed receipts");
    kernel
        .run_product_shaping_agent()
        .expect("product receipts");
    kernel.run_aegis_scanner_agent().expect("aegis receipts");
    kernel
        .run_charter_attestation_agent()
        .expect("charter receipts");
    kernel.run_talent_autonomy_agent().expect("talent receipts");
    let spawn_receipt_id = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "worker.spawn_requested")
        .expect("worker spawn receipt")
        .receipt_id
        .clone();
    let credential_check_receipt_id = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "worker.credential.checked")
        .expect("worker credential receipt")
        .receipt_id
        .clone();
    kernel
        .run_local_worker_runtime(WorkerRuntimeRequest {
            spawn_receipt_id,
            credential_check_receipt_id,
            output_artifacts: vec!["local://worker/export-proof-output".to_string()],
            verification_evidence: vec!["make local-full-check".to_string()],
            summary: "Export proof local worker handoff".to_string(),
            next_owner: "human:local_operator".to_string(),
        })
        .expect("local worker handoff");
    let source_receipt_id = kernel.ledger().entries()[0].receipt_id.clone();
    kernel
        .save_twin_session_draft_local(TwinSessionDraftRequest {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            session_id: "local_twin_session_export",
            companion_id: "advisor",
            companion_stance: "advisor",
            persona_profile_id: "local_persona_v1",
            prompt_shape: "operator_question",
            evidence_receipt_id: &source_receipt_id,
            draft_text: "Export-proof Twin draft",
            live_answer: None,
            recall_packet: None,
        })
        .expect("twin draft");
    let run_event_receipt_id = kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            run_id: "local_forge_run_export",
            event: "run_started",
            work_item_id: "w_export",
            detail: "accepted",
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        })
        .expect("forge run event")
        .receipt_id;
    kernel
        .record_forge_outcome_signal(ForgeOutcomeSignal {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            run_id: "local_forge_run_export",
            source_receipt_id: &run_event_receipt_id,
            source_receipt_kind: "forge.run.event",
            disposition: "completed",
            summary: "Export-proof Forge outcome signal.",
            capability_ids: "rust_backend_skill",
            model_or_worker: "local_forge_worker",
            lesson_candidate: "Similar governed builds should cite prior outcome signals before planning.",
            lesson_source: "",
            message_channel_id: "forge",
        })
        .expect("forge outcome signal");
    kernel
        .save_marketplace_act(MarketplaceAct {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            actor_role: "operator",
            act: "install",
            source_route: "/marketplace/installs.json",
            capability_id: "rust_backend_skill",
            scope: "repo",
            decision: "install",
            read_only: true,
            note: "Export-proof Marketplace install.",
            ..MarketplaceAct::default()
        })
        .expect("marketplace install");
    kernel
        .save_marketplace_act(MarketplaceAct {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            actor_role: "operator",
            act: "approval",
            source_route: "/marketplace/approvals.json",
            capability_id: "rust_backend_skill",
            scope: "repo",
            decision: "approve",
            read_only: true,
            note: "Export-proof Marketplace approval.",
            ..MarketplaceAct::default()
        })
        .expect("marketplace approval");
    kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            thread_id: "local_thread_export",
            channel_id: "local_ops",
            message_id: "local_message_export",
            content_type: "text",
            body: "Export-proof governed message",
            content_payload: "Export-proof governed message",
            source_receipt_id: &source_receipt_id,
            outbox_topic: "message.thread.local_ops",
        })
        .expect("message thread message");
    kernel
        .save_message_presence_request_local(MessagePresenceRequest {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            presence_request_id: "local_presence_export",
            fanout_request_receipt_id: "",
            thread_id: "local_thread_export",
            channel_id: "local_ops",
            presence_scope: "local projection only",
        })
        .expect("message presence request");
    kernel
        .save_message_realtime_cutover_preflight_local(MessageRealtimeCutoverPreflight {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            preflight_id: "local_message_realtime_export",
            presence_request_receipt_id: "",
            thread_id: "local_thread_export",
            channel_id: "local_ops",
            requested_realtime_scope: "tenant channel replay",
        })
        .expect("message realtime preflight");
    kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            document_id: "local_page_export",
            title: "Export Proof Page",
            body_ref: "world_model://pages/local_page_export/body/v1",
            source_receipt_id: &source_receipt_id,
            revision_id: "local_page_revision_export",
            page_type: "knowledge",
        })
        .expect("pages publication");
    kernel
        .save_pages_search_preflight_local(PagesSearchPreflight {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            preflight_id: "local_pages_search_export",
            source_approval_request_receipt_id: "",
            document_id: "local_page_export",
            revision_id: "local_page_revision_export",
            citation_handle: "citation_local_pages_export",
            attachment_policy_id: "local_attachment_policy",
            requested_index_scope: "local projection only",
        })
        .expect("pages search preflight");
    kernel
        .save_auth_tenant_policy_preflight_local(AuthTenantPolicyPreflight {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            preflight_id: "local_auth_tenant_policy_export",
            source_auth_session_evidence_id: "auth.session.local.stub",
            tenant_membership_scope: "tenant_org_members",
            role_mapping_scope: "mdx_roles",
            invite_state_scope: "invites",
            approved_model_scope: "tenant_approved_models",
        })
        .expect("auth tenant policy preflight");
    let model_correlation = CorrelationIds {
        tenant_id: TenantId::new("local_tenant"),
        trace_id: TraceId::new("trace_model_export"),
        actor_id: ActorId::new("human:local_user"),
        loop_id: LoopId::new("model_fabric"),
        workflow_id: WorkflowId::new("workflow_model_export"),
    };
    let connection = ProviderConnection {
        connection_id: "local_tenant:openai".to_string(),
        tenant_id: "local_tenant".to_string(),
        provider_id: "openai".to_string(),
        credential_ref: "env:OPENAI_API_KEY".to_string(),
        endpoint_base_url: "https://api.openai.com/v1".to_string(),
        region: "global".to_string(),
        residency: "provider_managed".to_string(),
        data_retention: "unknown".to_string(),
        training_policy: "unknown".to_string(),
        data_policy_provenance: "test_fixture".to_string(),
        data_policy_observed_at: "2026-07-12".to_string(),
        health: "disconnected".to_string(),
        live_call_allowed: false,
    };
    kernel.record_model_connection_configured(&model_correlation, &connection);
    let model = Model {
        model_id: "openai/test".to_string(),
        provider_model_id: "test".to_string(),
        provider_id: "openai".to_string(),
        display_name: "Test".to_string(),
        lifecycle: "active".to_string(),
        modality: "text".to_string(),
    };
    let deployment = ModelDeployment {
        deployment_id: "openai-test-global".to_string(),
        connection_id: connection.connection_id.clone(),
        provider_id: "openai".to_string(),
        model_id: model.model_id.clone(),
        privacy_class: "frontier".to_string(),
        region: "global".to_string(),
        residency: "provider_managed".to_string(),
        enabled: true,
        capabilities: CapabilityProfile {
            tools: true,
            structured_output: true,
            vision: false,
            context_tokens: 100_000,
            provenance: "test".to_string(),
            observed_at: "test-time".to_string(),
        },
    };
    kernel.record_model_deployment_registered(
        &model_correlation,
        &model,
        &deployment,
        &ModelPriceObservation {
            deployment_id: deployment.deployment_id.clone(),
            input_microusd_per_million: 1,
            output_microusd_per_million: 2,
            currency: "USD".to_string(),
            source: "test".to_string(),
            observed_at: "test-time".to_string(),
        },
    );
    let policy = RoutePolicy {
        policy_id: "forge-default".to_string(),
        policy_version: "v2".to_string(),
        lifecycle: "adaptive".to_string(),
        state: "draft".to_string(),
        canary_percent: 10,
        tenant_id: "local_tenant".to_string(),
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
    };
    kernel.record_model_route_policy_configured(&model_correlation, &policy);
    let decision = ModelRouteDecision {
        status: "ROUTE_SELECTED",
        workload_id: "mdx/forge/builder".to_string(),
        app_id: "forge".to_string(),
        environment: "local".to_string(),
        session_id: "run-1".to_string(),
        preset: "balanced".to_string(),
        policy_id: "core".to_string(),
        policy_version: "core-v1".to_string(),
        selected_deployment_id: Some(deployment.deployment_id.clone()),
        selected_provider_id: Some("openai".to_string()),
        selected_model_id: Some(model.model_id.clone()),
        selection_reason: "policy_objective",
        session_sticky_applied: false,
        provider_failover_deployment_ids: Vec::new(),
        model_fallback_deployment_ids: Vec::new(),
        fallback_deployment_ids: Vec::new(),
        exclusions: Vec::new(),
        grants_execution_authority: false,
    };
    let decision_receipt = kernel.record_model_route_decision(&model_correlation, &decision);
    kernel
        .record_model_outcome(
            &model_correlation,
            &ModelOutcome {
                decision_id: decision_receipt.receipt_id.clone(),
                workload_id: decision.workload_id.clone(),
                app_id: decision.app_id.clone(),
                environment: decision.environment.clone(),
                session_id: decision.session_id.clone(),
                deployment_id: deployment.deployment_id.clone(),
                latency_ms: 100,
                cost_microusd: Some(2),
                quality_score: Some(90),
                safety_status: "passed".to_string(),
                task_status: "succeeded".to_string(),
                correction_status: "accepted".to_string(),
                provenance: "test_evaluator".to_string(),
            },
        )
        .expect("model outcome");
    kernel
        .record_model_adaptive_comparison(
            &model_correlation,
            MODEL_ADAPTIVE_REPLAY_RECEIPT_KIND,
            &policy,
            &decision_receipt.receipt_id,
            &deployment.deployment_id,
            &decision,
        )
        .expect("adaptive replay");
    kernel
        .record_adaptive_policy_evaluation(
            &model_correlation,
            &AdaptivePolicyEvidence {
                policy_id: policy.policy_id.clone(),
                policy_version: policy.policy_version.clone(),
                state: "draft".to_string(),
                replay_cases: 1,
                shadow_decisions: 0,
                canary_decisions: 0,
                guardrail_evidence_sufficient: false,
                quality_guardrail_passed: false,
                safety_guardrail_passed: false,
                latency_guardrail_passed: false,
                cost_guardrail_passed: false,
            },
        )
        .expect("adaptive evaluation");

    let sql =
        render_postgres_app_state_export_sql(kernel.ledger().entries(), kernel.memory_records());

    for expected in [
        "-- mdx governed app-state local export proof",
        "-- memory_episode episode_id=episode_memory_",
        "INSERT INTO ledger_entries",
        "INSERT INTO model_provider_connections",
        "INSERT INTO model_catalog_models",
        "INSERT INTO model_deployments",
        "INSERT INTO model_price_observations",
        "INSERT INTO model_route_policies",
        "INSERT INTO model_route_decisions",
        "INSERT INTO model_outcomes",
        "INSERT INTO model_adaptive_comparisons",
        "INSERT INTO model_adaptive_policy_versions",
        "INSERT INTO memory_records",
        "INSERT INTO twin_sessions",
        "INSERT INTO twin_session_messages",
        "INSERT INTO twin_memory_snapshots",
        "INSERT INTO twin_model_traces",
        "INSERT INTO twin_conversation_sessions",
        "INSERT INTO twin_companion_stance_records",
        "INSERT INTO twin_memory_retrievals",
        "INSERT INTO twin_grounded_answers",
        "INSERT INTO twin_conversation_summaries",
        "INSERT INTO message_threads",
        "INSERT INTO message_thread_messages",
        "INSERT INTO message_fanout_requests",
        "INSERT INTO message_presence_records",
        "INSERT INTO message_channels",
        "INSERT INTO message_channel_members",
        "INSERT INTO message_thread_participants",
        "INSERT INTO message_envelopes",
        "INSERT INTO message_realtime_cutover_preflights",
        "INSERT INTO message_delivery_replay_batches",
        "INSERT INTO message_subscription_isolation_checks",
        "INSERT INTO message_service_role_fanout_refusals",
        "INSERT INTO pages_documents",
        "INSERT INTO pages_revisions",
        "INSERT INTO pages_publications",
        "INSERT INTO pages_approval_requests",
        "INSERT INTO pages_search_preflights",
        "INSERT INTO pages_citations",
        "INSERT INTO pages_publication_targets",
        "INSERT INTO pages_revision_citations",
        "INSERT INTO pages_attachments",
        "INSERT INTO pages_search_index_records",
        "ON CONFLICT (message_id) DO NOTHING",
        "INSERT INTO forge_outcome_signals",
        "INSERT INTO marketplace_acts",
        "INSERT INTO marketplace_installed_capabilities",
        "stronger_isolation_required_before_execution",
        "INSERT INTO product_signals",
        "INSERT INTO product_bet_drafts",
        "INSERT INTO product_handoff_requests",
        "INSERT INTO eval_guardrail_verdicts",
        "INSERT INTO security_findings",
        "INSERT INTO charter_attestations",
        "INSERT INTO strategy_ratification_snapshots",
        "INSERT INTO talent_sponsor_chain_authorities",
        "INSERT INTO talent_worker_lease_authorities",
        "INSERT INTO talent_budget_authorities",
        "INSERT INTO talent_tool_allowlist_authorities",
        "INSERT INTO talent_worker_spawn_requests",
        "INSERT INTO worker_runtime_handoffs",
        "INSERT INTO worker_runtime_retirements",
        "INSERT INTO observatory_role_view_snapshots",
        "INSERT INTO treasury_reserve_postures",
        "INSERT INTO auth_tenant_orgs",
        "INSERT INTO auth_tenant_memberships",
        "INSERT INTO auth_role_mappings",
        "INSERT INTO auth_invite_states",
        "INSERT INTO auth_visibility_policies",
        "INSERT INTO auth_approved_model_policies",
        "INSERT INTO auth_session_evidence",
        "INSERT INTO auth_tenant_policy_preflights",
        "SELECT count(*) FROM forge_outcome_signals",
        "SELECT count(*) FROM marketplace_installed_capabilities",
        "SELECT count(*) FROM twin_conversation_summaries",
        "SELECT count(*) FROM message_envelopes",
        "SELECT count(*) FROM message_realtime_cutover_preflights",
        "SELECT count(*) FROM pages_search_index_records",
        "SELECT count(*) FROM product_handoff_requests",
        "SELECT count(*) FROM security_findings",
        "SELECT count(*) FROM talent_worker_spawn_requests",
        "SELECT count(*) FROM observatory_role_view_snapshots",
        "SELECT count(*) FROM auth_tenant_policy_preflights",
        "SELECT count(*) FROM model_route_decisions",
    ] {
        assert!(sql.contains(expected), "{expected}");
    }
}

#[test]
fn postgres_app_state_writer_requires_database_url() {
    let result = PostgresAppStateWriter::connect(None);

    assert_eq!(result, Err(StorageAdapterError::MissingDatabaseUrl));
}

#[test]
fn postgres_app_state_writer_fails_closed_until_observed_migrations() {
    let result = PostgresAppStateWriter::connect(Some("postgres://mdx:mdx@localhost/mdx"));

    assert_eq!(
        result,
        Err(StorageAdapterError::PendingLiveRun {
            adapter: "PostgresAppStateWriter",
            reason: "durable app-state writes have not observed local Postgres migrations"
        })
    );
}

#[test]
fn postgres_app_state_writer_accepts_observed_migrations() {
    let writer = PostgresAppStateWriter::connect_after_observed_migrations(
        Some("postgres://mdx:mdx@localhost/mdx"),
        PostgresMigrationEvidence {
            migration_count: 44,
            tenant_owned_tables: 123,
            rls_enabled_tables: 125,
            observed_by: "test".to_string(),
        },
    )
    .expect("observed migrations");

    assert_eq!(writer.database_url(), "postgres://mdx:mdx@localhost/mdx");
    let contract = PostgresAppStateWriter::contract();
    assert_eq!(contract.adapter, "PostgresAppStateWriter");
    assert_eq!(contract.table_count, 70);
    assert!(contract.database_url_required);
    assert!(contract.observed_migrations_required);
    assert!(contract.writes_live_database);
}

#[test]
fn postgres_app_state_writer_rejects_migration_evidence_mismatch() {
    let result = PostgresAppStateWriter::connect_after_observed_migrations(
        Some("postgres://mdx:mdx@localhost/mdx"),
        PostgresMigrationEvidence {
            migration_count: 8,
            tenant_owned_tables: 67,
            rls_enabled_tables: 70,
            observed_by: "test".to_string(),
        },
    );

    assert_eq!(
        result,
        Err(StorageAdapterError::MigrationEvidenceMismatch {
            expected_migrations: 44,
            observed_migrations: 8,
            expected_tenant_owned_tables: 123,
            observed_tenant_owned_tables: 67,
            expected_rls_enabled_tables: 125,
            observed_rls_enabled_tables: 70,
        })
    );
}
