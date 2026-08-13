use super::*;

#[test]
fn ctx_local_engine_assembles_memory_and_world_model_sources() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("seed memory");
    let packet = kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            query: "developer onboarding memory proof",
            token_budget: 240,
            source_preferences: &[],
            query_entities: &["developer"],
            local_memories: &[],
        })
        .expect("context packet");

    assert_eq!(packet.status, "LIVE-LOCAL-CONTEXT-ASSEMBLY");
    assert_eq!(packet.ctx_driver, "local_ctx_engine");
    assert_eq!(packet.memory_driver, "local_memory_store");
    assert_eq!(packet.memory_provider, "InMemoryProvider");
    assert_eq!(packet.vector_driver, "pgvector_ready_local_projection");
    assert_eq!(packet.working_cache_driver, "valkey_ready_local_projection");
    assert!(!packet.provider_call_allowed);
    assert!(!packet.embedding_provider_call_allowed);
    assert!(!packet.production_write_allowed);
    assert!(packet.total_tokens <= packet.token_budget);
    assert_eq!(packet.source_type_count, 10);
    assert!(packet.included_source_count >= 4);
    assert!(packet.budget_allocations.len() >= 4);
    assert!(packet.hot_path_budget_ms >= packet.estimated_p95_ms);
    assert_eq!(packet.query_entity_count, 1);
    assert!(packet.hygiene_report.tokens_out <= packet.hygiene_report.tokens_in);
    assert_eq!(packet.memory_tiers.len(), 4);
    for tier in ["working", "episodic", "semantic", "procedural"] {
        assert!(
            packet
                .memory_tiers
                .iter()
                .any(|report| report.tier == tier && report.source_count > 0),
            "missing memory tier {tier}"
        );
    }
    assert_eq!(
        packet.consolidation_report.status,
        "LIVE-LOCAL-CONSOLIDATION-PROPOSAL-FLOOR"
    );
    assert!(packet.consolidation_report.human_review_required);
    assert!(!packet.consolidation_report.auto_promotion_allowed);
    assert_eq!(
        packet.handoff_report.strategy_order,
        vec![
            "dedupe",
            "prune_by_recency",
            "prune_by_relevance",
            "summarize"
        ]
    );
    assert_eq!(
        packet.vector_report.status,
        "LOCAL-PGVECTOR-PROJECTION-READY-PROVIDER-BLOCKED"
    );
    assert!(!packet.vector_report.live_index_ready);
    assert_eq!(
        packet.working_cache_report.status,
        "LOCAL-VALKEY-PROJECTION-READY-PROVIDER-BLOCKED"
    );
    assert!(!packet.working_cache_report.live_cache_ready);
    assert_eq!(
        packet.signal_report.status,
        "LIVE-LOCAL-SIGNAL-INTELLIGENCE-FLOOR"
    );
    assert_eq!(packet.event_stream_report.status, "PENDING-LIVE-RUN");
    assert!(
        packet
            .sources
            .iter()
            .any(|source| source.source_type == "episodic_memory")
    );
    assert!(
        packet
            .sources
            .iter()
            .any(|source| source.source_type == "pages_world_model_document")
    );
    assert!(
        packet
            .sources
            .iter()
            .any(|source| source.source_type == "system_prompt" && source.fixed)
    );
    assert!(
        packet
            .sources
            .iter()
            .all(|source| source.score.composite > 0)
    );

    let assembly = kernel
        .ledger()
        .query()
        .by_id(&packet.assembly_receipt_id)
        .expect("assembly receipt");
    assert_eq!(assembly.kind, "ctx.context.assembled");
    assert_eq!(assembly.payload["ctx_driver"], "local_ctx_engine");
    assert_eq!(assembly.payload["provider_call_allowed"], "false");
    assert_eq!(assembly.payload["assembly_log_id"], packet.assembly_log_id);
    assert_eq!(
        assembly.payload["memory_tiers"],
        "working,episodic,semantic,procedural"
    );
    assert_eq!(
        assembly.payload["consolidation_status"],
        "LIVE-LOCAL-CONSOLIDATION-PROPOSAL-FLOOR"
    );
    assert_eq!(
        assembly.payload["handoff_status"],
        "LIVE-LOCAL-HANDOFF-SUMMARY-FLOOR"
    );

    let refusal = kernel
        .ledger()
        .query()
        .by_id(&packet.embedding_refusal_receipt_id)
        .expect("embedding refusal receipt");
    assert_eq!(refusal.kind, "ctx.embedding_provider.refused");
    assert_eq!(refusal.payload["embedding_provider_call_allowed"], "false");
}

#[test]
fn ctx_local_engine_includes_only_active_trusted_context_sources() {
    let mut kernel = MdxKernel::boot_local();
    let source = kernel
        .save_twin_artifact_context_local(TwinArtifactContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            session_id: "twin_artifact_session_context_source",
            artifact_name: "architecture-standard.md",
            mime_type: "text/markdown",
            artifact_text: "Architecture standards require the shared Source and Artifact rail for context.",
            privacy_class: "user_private_sensitive",
            company_context: "Pages says architecture standards must be receipt backed.",
            company_context_source: "Pages:Context source rail contract",
        })
        .expect("source artifact");
    let trust = kernel
        .save_pages_context_source_trust_decision_local(PagesContextSourceTrustDecision {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            trust_decision_id: "context_trust_architecture_standard",
            source_id: &source.artifact_id,
            decision_outcome: "trusted_for_ai",
            decision_note: "Approved as company architecture context.",
            source_route: "/pages/context-sources/trust-decisions.json",
        })
        .expect("trust source");

    let packet = kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            query: "architecture shared source rail context",
            token_budget: 260,
            source_preferences: &[],
            query_entities: &["architecture", "context"],
            local_memories: &[],
        })
        .expect("context packet");

    let trusted_source = packet
        .sources
        .iter()
        .find(|candidate| candidate.source_type == "trusted_context_source")
        .expect("trusted context source");
    assert_eq!(trusted_source.source_id, source.artifact_id);
    assert_eq!(trusted_source.title, "architecture-standard.md");
    assert_eq!(
        trusted_source.world_model_source,
        Some("pages_context_source_rail")
    );
    assert!(
        trusted_source
            .source_receipt_ids
            .contains(&trust.trust_decision_receipt_id)
    );
    assert!(
        trusted_source
            .content
            .contains("approved for AI use from twin")
    );

    let assembly = kernel
        .ledger()
        .query()
        .by_id(&packet.assembly_receipt_id)
        .expect("assembly receipt");
    assert!(
        assembly.payload["source_types"].contains("trusted_context_source"),
        "{assembly:?}"
    );

    kernel
        .delete_twin_artifact_local(TwinArtifactDeletionRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            artifact_id: &source.artifact_id,
            reason: "Delete trusted context source.",
            local_blob_bytes_deleted: false,
        })
        .expect("delete source");

    let after_delete = kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            query: "architecture shared source rail context",
            token_budget: 260,
            source_preferences: &[],
            query_entities: &["architecture", "context"],
            local_memories: &[],
        })
        .expect("context packet after delete");
    assert!(!after_delete.sources.iter().any(|candidate| {
        candidate.source_type == "trusted_context_source"
            && candidate.source_id == source.artifact_id
    }));
}

#[test]
fn ctx_local_engine_includes_trusted_tenant_connector_sources() {
    let mut kernel = MdxKernel::boot_local();
    let connector = kernel
        .record_external_item(ExternalItem {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            source_id: "github:mdx-stack",
            source_kind: "github",
            external_ref: "https://github.com/mdx-stack/docs/architecture.md",
            title: "Architecture guide",
            summary: "Company architecture standards require shared source rails.",
            body_ref: "",
            data_sensitivity: "normal",
            handling: "frontier",
            grade: "operational",
            scope: "tenant",
        })
        .expect("connector item");
    let personal = kernel
        .record_external_item(ExternalItem {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            source_id: "notion:private",
            source_kind: "notion",
            external_ref: "https://notion/private",
            title: "Private note",
            summary: "Personal material.",
            body_ref: "",
            data_sensitivity: "normal",
            handling: "frontier",
            grade: "none",
            scope: "personal",
        })
        .expect("personal connector item");
    let trust = kernel
        .save_pages_context_source_trust_decision_local(PagesContextSourceTrustDecision {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            trust_decision_id: "context_trust_connector_architecture",
            source_id: &connector.receipt_id,
            decision_outcome: "trusted_for_ai",
            decision_note: "Approved as company architecture context.",
            source_route: "/pages/context-sources/trust-decisions.json",
        })
        .expect("trust connector source");
    let personal_trust =
        kernel.save_pages_context_source_trust_decision_local(PagesContextSourceTrustDecision {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            trust_decision_id: "context_trust_personal_connector",
            source_id: &personal.receipt_id,
            decision_outcome: "trusted_for_ai",
            decision_note: "Should not enter company context.",
            source_route: "/pages/context-sources/trust-decisions.json",
        });
    assert!(personal_trust.is_err());

    let packet = kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            query: "architecture shared source rail context",
            token_budget: 260,
            source_preferences: &[],
            query_entities: &["architecture", "context"],
            local_memories: &[],
        })
        .expect("context packet");

    let trusted_source = packet
        .sources
        .iter()
        .find(|candidate| {
            candidate.source_type == "trusted_context_source"
                && candidate.source_id == connector.receipt_id
        })
        .expect("trusted connector context source");
    assert_eq!(trusted_source.title, "Architecture guide");
    assert!(
        trusted_source
            .source_receipt_ids
            .contains(&trust.trust_decision_receipt_id)
    );
    assert!(
        trusted_source
            .content
            .contains("approved for AI use from connector"),
        "{}",
        trusted_source.content
    );
    assert!(!packet.sources.iter().any(|candidate| {
        candidate.source_type == "trusted_context_source"
            && candidate.source_id == personal.receipt_id
    }));
}

#[test]
fn ctx_local_engine_applies_source_preferences_and_budget_truncation() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("seed memory");
    let preferences = [LocalContextSourcePreference {
        source_type: "pages_world_model_document",
        enabled: false,
        token_budget_override: None,
    }];
    let packet = kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            query: "developer onboarding memory proof world model",
            token_budget: 36,
            source_preferences: &preferences,
            query_entities: &[],
            local_memories: &[],
        })
        .expect("context packet");

    assert_eq!(packet.disabled_source_count, 1);
    assert_eq!(packet.source_preferences_applied.len(), 1);
    assert!(
        !packet
            .sources
            .iter()
            .any(|source| source.source_type == "pages_world_model_document")
    );
    assert!(packet.truncated_source_count > 0);
    assert!(!packet.truncated_sources.is_empty());
    assert!(packet.total_tokens <= packet.token_budget);
    assert!(
        packet
            .memory_tiers
            .iter()
            .any(|tier| tier.tier == "working" && tier.cache_driver.is_some())
    );
}

#[test]
fn ctx_local_engine_accepts_request_bound_memory_inputs() {
    let mut kernel = MdxKernel::boot_local();
    let memories = [LocalContextMemoryInput {
        memory_id: "memory_local_procedural_0001",
        tier: "procedural",
        content: "When Forge prepares a worker run, require CTX grounding before DXR execution.",
        source_receipt_id: "receipt_local_memory_input_0001",
        importance: 96,
    }];
    let packet = kernel
        .assemble_local_context(LocalContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            query: "forge worker ctx grounding",
            token_budget: 120,
            source_preferences: &[],
            query_entities: &["forge", "ctx"],
            local_memories: &memories,
        })
        .expect("context packet");

    assert_eq!(packet.local_memory_input_count, 1);
    assert!(packet.sources.iter().any(|source| {
        source.source_id == "memory_local_procedural_0001"
            && source.memory_tier == Some("procedural")
            && source
                .source_receipt_ids
                .contains(&"receipt_local_memory_input_0001".to_string())
    }));
    assert!(
        packet
            .memory_tiers
            .iter()
            .any(|tier| tier.tier == "procedural" && tier.source_count >= 2)
    );
    let assembly = kernel
        .ledger()
        .query()
        .by_id(&packet.assembly_receipt_id)
        .expect("assembly receipt");
    assert_eq!(assembly.payload["local_memory_input_count"], "1");
}
