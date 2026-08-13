use mdx_core::{MdxKernel, json_string_literal};

pub(crate) fn render_brain_map_json(kernel: &MdxKernel) -> String {
    let memory_count = kernel.memory_records().len();
    // Phase 1 read: Twin now reads ranked private memory into its live
    // prompt context, so its influence flag is honestly true. Every other
    // surface stays false until its own read wire lands.
    let surfaces = [
        (
            "twin",
            "first_consumer",
            "/twin/intelligence-readiness.json",
            true,
        ),
        (
            "ctx",
            "recall_runtime",
            "/ctx/operational-health.json",
            false,
        ),
        ("learn", "promotion_activation", "/learn", false),
        ("pages", "source_of_truth_world_model", "/pages.json", false),
        (
            "message",
            "team_decision_plane",
            "/messages/threads.json",
            false,
        ),
        (
            "forge",
            "work_outcome_plane",
            "/forge/intake-plan.json",
            false,
        ),
    ];
    let surface_rows = surfaces
        .iter()
        .map(|(id, role, route, influence)| {
            format!(
                r#"{{"id":{},"role":{},"source_route":{},"runtime_influence_allowed":{}}}"#,
                json_string_literal(id),
                json_string_literal(role),
                json_string_literal(route),
                influence
            )
        })
        .collect::<Vec<_>>();
    let scopes = [
        ("private_user_memory", "private", "private_by_default"),
        ("shared_project_memory", "project", "explicit_project_share"),
        ("team_department_memory", "team", "reviewed_team_share"),
        ("company_memory", "company", "source_of_truth_required"),
        (
            "agent_operational_memory",
            "operational",
            "receipt_backed_only",
        ),
    ];
    let scope_rows = scopes
        .iter()
        .map(|(id, visibility, sharing)| {
            format!(
                r#"{{"id":{},"visibility":{},"sharing_default":{}}}"#,
                json_string_literal(id),
                json_string_literal(visibility),
                json_string_literal(sharing)
            )
        })
        .collect::<Vec<_>>();
    let primitives = [
        "MemoryEpisode",
        "MemoryAtom",
        "MemoryScope",
        "MemoryTier",
        "MemoryGraph",
        "RecallPacket",
        "ConsolidationProposal",
        "MemoryActivation",
        "AdaptationProposal",
    ];
    let primitive_rows = primitives
        .iter()
        .map(|primitive| json_string_literal(primitive))
        .collect::<Vec<_>>();
    // The two sources that actually reach a live prompt today: ranked
    // kernel memory records and activated lesson summaries, both through
    // the Twin prompt-context wire. Prompt context only; adaptation stays
    // closed.
    let sources = [
        (
            "kernel_memory_records",
            "MemoryStore",
            memory_count > 0,
            true,
        ),
        ("learning_active_memory_contracts", "Learn", true, true),
        ("ctx_runtime_contract", "CTX", true, false),
        ("twin_session_memory", "Twin", true, false),
        ("pages_world_model", "Pages", true, false),
        ("message_events", "Message", true, false),
        ("forge_outcomes", "Forge", true, false),
    ];
    let source_rows = sources
        .iter()
        .map(|(id, source_type, receipt_backed, influence)| {
            format!(
                r#"{{"id":{},"source_type":{},"receipt_backed":{},"runtime_influence_allowed":{}}}"#,
                json_string_literal(id),
                json_string_literal(source_type),
                receipt_backed,
                influence
            )
        })
        .collect::<Vec<_>>();
    let scorecards = [
        "LoCoMo",
        "LongMemEval",
        "LongMemEval-V2",
        "Memora",
        "MemoryAgentBench",
        "BEAM",
        "MDx Brain Score",
    ];
    let scorecard_rows = scorecards
        .iter()
        .map(|scorecard| json_string_literal(scorecard))
        .collect::<Vec<_>>();
    let work_items = [
        "memory_brain_map_contract",
        "twin_brain_recall_preflight",
        "twin_memory_episode_local",
        "trusted_time_arc",
        "world_model_origin_reconciliation",
        "memory_durable_restart_proof",
        "memory_consolidation_lane",
        "memory_graph_local",
        "memory_eval_harness",
        "vendor_memory_comparator",
        "memory_scale_latency_harness",
        "brain_service_topology_contract",
    ];
    let work_item_rows = work_items
        .iter()
        .map(|item| json_string_literal(item))
        .collect::<Vec<_>>();
    r#"{"name":"mdx-memory-brain-map","status":"READ_PHASE_1","read_only":true,"route":"/memory/brain-map.json","writes_allowed":false,"memory_driver":"local_memory_store","memory_provider":"InMemoryProvider","durable_driver":"postgres_memory_records","vendor_memory_driver":"mem0_memory_store","vendor_status":"PENDING-LIVE-RUN","source_contract":"docs/MDX-MEMORY-BRAIN-RUNWAY.md","runtime_influence_allowed":true,"runtime_influence_scope":"prompt_context_only","adaptation_allowed":false,"provider_call_allowed":false,"production_write_allowed":false,"memory_record_count":__MEMORY_COUNT__,"surface_count":__SURFACE_COUNT__,"scope_count":__SCOPE_COUNT__,"primitive_count":__PRIMITIVE_COUNT__,"scale_fixture_targets":[1,10,50,100,500,1000,5000],"scorecards":[__SCORECARDS__],"surfaces":[__SURFACES__],"scopes":[__SCOPES__],"primitives":[__PRIMITIVES__],"sources":[__SOURCES__],"next_work_items":[__WORK_ITEMS__]}"#
        .replace("__MEMORY_COUNT__", &memory_count.to_string())
        .replace("__SURFACE_COUNT__", &surfaces.len().to_string())
        .replace("__SCOPE_COUNT__", &scopes.len().to_string())
        .replace("__PRIMITIVE_COUNT__", &primitives.len().to_string())
        .replace("__SCORECARDS__", &scorecard_rows.join(","))
        .replace("__SURFACES__", &surface_rows.join(","))
        .replace("__SCOPES__", &scope_rows.join(","))
        .replace("__PRIMITIVES__", &primitive_rows.join(","))
        .replace("__SOURCES__", &source_rows.join(","))
        .replace("__WORK_ITEMS__", &work_item_rows.join(","))
}

pub(crate) fn render_brain_substrate_json(kernel: &MdxKernel) -> String {
    let memory_record_count = kernel.memory_records().len();
    let episode_count = kernel
        .memory_records()
        .iter()
        .filter(|record| !record.episode_id.is_empty())
        .count();
    let graph_node_count = kernel.memory_graph_nodes().len();
    let graph_edge_count = kernel.memory_graph_edges().len();
    // Only the local self-retrieval smoke has actually run; external
    // benchmark targets live in the brain-map runway contract.
    let scorecards = ["self_retrieval_smoke", "MDx Brain Score"]
        .iter()
        .map(|scorecard| json_string_literal(scorecard))
        .collect::<Vec<_>>();
    let fixture_targets = [1, 10, 50, 100, 500, 1000, 5000]
        .iter()
        .map(|target| target.to_string())
        .collect::<Vec<_>>();
    let services = [
        (
            "brain-api",
            "mdx-server-route",
            "read_projection",
            "LOCAL_READY",
        ),
        (
            "memory-store",
            "kernel-port",
            "episode_write_path",
            "LIVE_LOCAL",
        ),
        (
            "ctx-recall",
            "rust-runtime",
            "hot_context_assembly",
            "CONTRACT_READY",
        ),
        (
            "consolidation-worker",
            "background-worker",
            "async_summary_and_graph_updates",
            "DECLARED_BLOCKED",
        ),
        (
            "valkey-hot-cache",
            "cache-service",
            "low_latency_recall_cache",
            "OPTIONAL_PENDING",
        ),
        (
            "postgres-durable-memory",
            "database",
            "restart_replay_source",
            "EXPORT_PROOF_READY",
        ),
    ];
    let service_rows = services
        .iter()
        .map(|(service, shape, role, status)| {
            format!(
                r#"{{"service":{},"deployment_shape":{},"latency_role":{},"status":{}}}"#,
                json_string_literal(service),
                json_string_literal(shape),
                json_string_literal(role),
                json_string_literal(status)
            )
        })
        .collect::<Vec<_>>();
    let current_score = kernel
        .memory_brain_eval_runs()
        .iter()
        .rev()
        .find(|run| run.fixture_family == "MDx Brain Score")
        .map(|run| run.brain_score)
        .unwrap_or(0);
    r#"{"name":"mdx-memory-brain-substrate","status":"LOCAL_SUBSTRATE_READY","read_only":true,"route":"/memory/brain-substrate.json","brain_map_route":"/memory/brain-map.json","memory_records_route":"/memory/records.json","writes_allowed":false,"provider_call_allowed":false,"production_write_allowed":false,"memory_record_count":__MEMORY_RECORD_COUNT__,"episode_count":__EPISODE_COUNT__,"durable_memory":{"driver":"postgres_memory_records","table":"memory_records","export_command":"cargo run -p mdx-server -- export-memory-store-sql","restart_replay_proof":"deterministic_sql_export","live_database_write_allowed":false},"consolidation_lane":{"policy":"local_session_summary_v1","human_review_required":true,"promotion_allowed":false,"contradiction_detection":"lifecycle_events_and_ranked_recall","decay_policy":"local_recent_session_decay_v1"},"memory_graph":{"graph_id":"local_memory_graph_v1","node_kinds":["MemoryEpisode","MemoryAtom","SourceReceipt","Surface"],"edge_kinds":["CONTAINS_ATOM","DERIVED_FROM","MENTIONS_SURFACE","RETRIEVAL_TRAVERSES","SUPERSEDES","CONTRADICTS"],"write_allowed":true,"node_count":__GRAPH_NODE_COUNT__,"edge_count":__GRAPH_EDGE_COUNT__},"scale_eval":{"score_name":"MDx Brain Score","scorecards":[__SCORECARDS__],"fixture_targets":[__FIXTURE_TARGETS__],"latency_budget_ms":250,"target_sessions_supported":1000,"current_local_score":__CURRENT_SCORE__},"service_topology":[__SERVICE_TOPOLOGY__],"governance":{"shared_memory_allowed":true,"team_memory_allowed":true,"company_memory_allowed":true,"private_memory_export_allowed":false,"vendor_swap_allowed":false}}"#
        .replace("__MEMORY_RECORD_COUNT__", &memory_record_count.to_string())
        .replace("__EPISODE_COUNT__", &episode_count.to_string())
        .replace("__GRAPH_NODE_COUNT__", &graph_node_count.to_string())
        .replace("__GRAPH_EDGE_COUNT__", &graph_edge_count.to_string())
        .replace("__CURRENT_SCORE__", &current_score.to_string())
        .replace("__SCORECARDS__", &scorecards.join(","))
        .replace("__FIXTURE_TARGETS__", &fixture_targets.join(","))
        .replace("__SERVICE_TOPOLOGY__", &service_rows.join(","))
}

pub(crate) fn render_brain_runtime_json(kernel: &MdxKernel) -> String {
    let receipts = kernel.ledger().entries();
    let count_kind = |kind: &str| {
        receipts
            .iter()
            .filter(|receipt| receipt.kind == kind)
            .count()
    };
    let proposal_count = count_kind("memory.consolidation.proposed");
    let review_count = count_kind("memory.consolidation.reviewed");
    let approved_review_count = receipts
        .iter()
        .filter(|receipt| {
            receipt.kind == "memory.consolidation.reviewed"
                && receipt.payload.get("review_state").map(String::as_str)
                    == Some("APPROVED_LOCAL_REVIEW")
        })
        .count();
    let retained_count = kernel
        .memory_records()
        .iter()
        .filter(|record| record.consolidation_decision.as_str() == "RETAIN")
        .count();
    let graph_node_count = kernel.memory_graph_nodes().len();
    let graph_edge_count = kernel.memory_graph_edges().len();
    let origin_rows = ["derived", "asserted", "external"]
        .iter()
        .map(|origin| {
            let count = kernel
                .memory_records()
                .iter()
                .filter(|record| record.atom_origin == *origin)
                .count();
            format!(
                r#"{{"origin":{},"count":{}}}"#,
                json_string_literal(origin),
                count
            )
        })
        .collect::<Vec<_>>();
    let comparator_rows = kernel
        .memory_vendor_comparator_runs()
        .iter()
        .map(|run| {
        format!(
            r#"{{"id":{},"driver":{},"status":{},"accuracy_score":{},"latency_ms":{},"cost_micros":{},"allowed_to_write":false}}"#,
            json_string_literal(run.vendor_id),
            json_string_literal(if run.vendor_id == "owned_mdx" { "local_memory_store" } else { run.vendor_id }),
            json_string_literal(run.status),
            run.accuracy_score,
            run.latency_ms,
            run.cost_micros
        )
        })
        .collect::<Vec<_>>();
    let service_rows = kernel
        .memory_production_topology_checks()
        .iter()
        .map(|check| {
        format!(
            r#"{{"service":{},"deployment_shape":{},"latency_role":{},"queue_or_cache":{},"observed_latency_ms":{},"status":"LOCAL_READY"}}"#,
            json_string_literal(check.service),
            json_string_literal(check.deployment_shape),
            json_string_literal(check.latency_role),
            json_string_literal(check.queue_or_cache),
            check.observed_latency_ms
        )
        })
        .collect::<Vec<_>>();
    let brain_score = kernel
        .memory_brain_eval_runs()
        .iter()
        .rev()
        .find(|run| run.fixture_family == "MDx Brain Score")
        .map(|run| u64::from(run.brain_score))
        .unwrap_or(0);
    format!(
        r#"{{"name":"mdx-memory-brain-runtime","status":"LOCAL_RUNTIME_READY","read_only":true,"route":"/memory/brain-runtime.json","memory_records_route":"/memory/records.json","brain_substrate_route":"/memory/brain-substrate.json","graph_route":"/memory/graph.json","lifecycle_route":"/memory/lifecycle.json","lifecycle_evaluation_route":"/memory/lifecycle-evaluations.json","ranking_route":"/memory/recall-rankings.json","eval_route":"/memory/brain-evals.json","eval_run_route":"/memory/brain-eval-runs.json","topology_validation_route":"/memory/topology-validations.json","writes_allowed":false,"provider_call_allowed":false,"production_write_allowed":false,"consolidation_lane":{{"proposal_count":{},"review_count":{},"approved_review_count":{},"retained_memory_count":{},"human_review_required":true,"promotion_allowed_without_review":false,"proposal_receipt_kind":"memory.consolidation.proposed","review_receipt_kind":"memory.consolidation.reviewed"}},"memory_graph":{{"graph_id":"local_memory_graph_v1","node_count":{},"edge_count":{},"origin_taxonomy":[{}],"edge_kinds":["CONTAINS_ATOM","DERIVED_FROM","MENTIONS_SURFACE","RETRIEVAL_TRAVERSES","SUPERSEDES","CONTRADICTS"],"temporal_truth_requires_trusted_time":true}},"recall_eval":{{"score_name":"MDx Brain Score","current_local_score":{},"target_sessions_supported":1000,"scale_fixture_targets":[1,10,50,100,500,1000,5000],"scorecards":["self_retrieval_smoke","MDx Brain Score"],"latency_budget_ms":250,"ranking_count":{},"fixture_result_count":{}}},"shared_scope_governance":{{"private_memory_default":true,"project_memory_allowed":true,"team_memory_allowed":true,"company_memory_allowed":true,"share_requires_review":true,"private_memory_export_allowed":false}},"vendor_comparators":[{}],"service_topology":[{}],"topology_runtime_event_count":{}}}"#,
        proposal_count,
        review_count,
        approved_review_count,
        retained_count,
        graph_node_count,
        graph_edge_count,
        origin_rows.join(","),
        brain_score,
        kernel.memory_recall_rankings().len(),
        kernel.memory_eval_fixture_results().len(),
        comparator_rows.join(","),
        service_rows.join(","),
        kernel.memory_topology_runtime_events().len()
    )
}
