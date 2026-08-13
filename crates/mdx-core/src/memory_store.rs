#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryDriverContract {
    pub driver_id: &'static str,
    pub provider: &'static str,
    pub mode: &'static str,
    pub status: &'static str,
    pub persistence: &'static str,
    pub durable_driver: &'static str,
    pub durable_table: &'static str,
    pub durable_status: &'static str,
    pub receipt_driver_field: &'static str,
    pub live_substrate_required: bool,
    pub live_database_write_allowed: bool,
    pub production_write_allowed: bool,
    pub vendor_swappable: bool,
}

pub const LOCAL_MEMORY_DRIVER: MemoryDriverContract = MemoryDriverContract {
    driver_id: "local_memory_store",
    provider: "InMemoryProvider",
    mode: "local",
    status: "LIVE-LOCAL-CONFORMANCE",
    persistence: "kernel_local_memory_plus_postgres_memory_records",
    durable_driver: "postgres_memory_records",
    durable_table: "memory_records",
    durable_status: "POSTGRES_DURABLE_MEMORY_SHAPE",
    receipt_driver_field: "memory_driver",
    live_substrate_required: false,
    live_database_write_allowed: false,
    production_write_allowed: false,
    vendor_swappable: true,
};

pub const MEM0_VENDOR_MEMORY_DRIVER: MemoryDriverContract = MemoryDriverContract {
    driver_id: "mem0_memory_store",
    provider: "Mem0MemoryProvider",
    mode: "vendor",
    status: "PENDING-LIVE-RUN",
    persistence: "external_mem0",
    durable_driver: "external_mem0",
    durable_table: "external_mem0",
    durable_status: "PENDING-LIVE-RUN",
    receipt_driver_field: "memory_driver",
    live_substrate_required: true,
    live_database_write_allowed: false,
    production_write_allowed: false,
    vendor_swappable: true,
};

pub fn memory_store_driver_contracts() -> &'static [MemoryDriverContract] {
    &[LOCAL_MEMORY_DRIVER, MEM0_VENDOR_MEMORY_DRIVER]
}

pub fn render_postgres_memory_export_sql(kernel: &MdxKernel) -> String {
    let mut sql = String::from("-- mdx MemoryStore local export proof\n");
    sql.push_str(&crate::render_postgres_ledger_export_sql(
        kernel.ledger().entries(),
        "memory_store_export",
    ));
    for record in kernel.memory_records() {
        sql.push_str(&format!(
            "-- memory_episode episode_id={} scope={} tier={} origin={} valid_from_receipt_timestamp={} decay_policy={} importance_score={}\n",
            record.episode_id,
            record.memory_scope,
            record.memory_tier,
            record.atom_origin,
            record.valid_from_receipt_timestamp,
            record.decay_policy,
            record.importance_score
        ));
        sql.push_str(&format!(
            "INSERT INTO memory_records (memory_id, tenant_id, source_receipt_id, atom_origin, valid_from_receipt_timestamp, valid_until_receipt_timestamp, invalidated_by_receipt_id, consolidation_state, memory_scope, memory_tier, decay_policy, importance_score, consolidation_decision, content, embedding) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (memory_id) DO UPDATE SET source_receipt_id = EXCLUDED.source_receipt_id, atom_origin = EXCLUDED.atom_origin, valid_from_receipt_timestamp = EXCLUDED.valid_from_receipt_timestamp, valid_until_receipt_timestamp = EXCLUDED.valid_until_receipt_timestamp, invalidated_by_receipt_id = EXCLUDED.invalidated_by_receipt_id, consolidation_state = EXCLUDED.consolidation_state, memory_scope = EXCLUDED.memory_scope, memory_tier = EXCLUDED.memory_tier, decay_policy = EXCLUDED.decay_policy, importance_score = EXCLUDED.importance_score, consolidation_decision = EXCLUDED.consolidation_decision, content = EXCLUDED.content, embedding = EXCLUDED.embedding;\n",
            sql_string_literal(&record.memory_id),
            sql_string_literal(record.tenant_id.as_str()),
            sql_string_literal(&record.source_receipt_id),
            sql_string_literal(record.atom_origin),
            sql_string_literal(&record.valid_from_receipt_timestamp),
            sql_string_literal(&record.valid_until_receipt_timestamp),
            sql_string_literal(&record.invalidated_by_receipt_id),
            sql_string_literal(record.consolidation_state),
            sql_string_literal(record.memory_scope),
            sql_string_literal(record.memory_tier),
            sql_string_literal(record.decay_policy),
            record.importance_score,
            sql_string_literal(record.consolidation_decision.as_str()),
            sql_string_literal(&record.content),
            sql_string_literal(&record.embedding)
        ));
    }
    for node in kernel.memory_graph_nodes() {
        sql.push_str(&format!(
            "INSERT INTO memory_graph_nodes (node_id, tenant_id, node_kind, label, memory_id, source_receipt_id, atom_origin, valid_from_receipt_timestamp, lifecycle_state) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (node_id) DO UPDATE SET lifecycle_state = EXCLUDED.lifecycle_state;\n",
            sql_string_literal(&node.node_id),
            sql_string_literal(node.tenant_id.as_str()),
            sql_string_literal(node.node_kind),
            sql_string_literal(&node.label),
            sql_string_literal(node.memory_id.as_deref().unwrap_or("")),
            sql_string_literal(&node.source_receipt_id),
            sql_string_literal(node.atom_origin),
            sql_string_literal(&node.valid_from_receipt_timestamp),
            sql_string_literal(node.lifecycle_state)
        ));
    }
    for edge in kernel.memory_graph_edges() {
        sql.push_str(&format!(
            "INSERT INTO memory_graph_edges (edge_id, tenant_id, from_node_id, to_node_id, edge_kind, source_receipt_id, weight, valid_from_receipt_timestamp) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (edge_id) DO UPDATE SET weight = EXCLUDED.weight;\n",
            sql_string_literal(&edge.edge_id),
            sql_string_literal(edge.tenant_id.as_str()),
            sql_string_literal(&edge.from_node_id),
            sql_string_literal(&edge.to_node_id),
            sql_string_literal(edge.edge_kind),
            sql_string_literal(&edge.source_receipt_id),
            edge.weight,
            sql_string_literal(&edge.valid_from_receipt_timestamp)
        ));
    }
    for event in kernel.memory_lifecycle_events() {
        sql.push_str(&format!(
            "INSERT INTO memory_lifecycle_events (event_id, tenant_id, memory_id, action, lifecycle_state, reason, source_receipt_id, valid_from_receipt_timestamp, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (event_id) DO UPDATE SET lifecycle_state = EXCLUDED.lifecycle_state;\n",
            sql_string_literal(&event.event_id),
            sql_string_literal(event.tenant_id.as_str()),
            sql_string_literal(&event.memory_id),
            sql_string_literal(event.action),
            sql_string_literal(event.lifecycle_state),
            sql_string_literal(&event.reason),
            sql_string_literal(&event.source_receipt_id),
            sql_string_literal(&event.valid_from_receipt_timestamp),
            sql_string_literal(&event.receipt_id)
        ));
    }
    for ranking in kernel.memory_recall_rankings() {
        sql.push_str(&format!(
            "INSERT INTO memory_recall_rankings (ranking_id, tenant_id, surface, query, memory_id, lexical_score, content_checksum_score, graph_score, recency_score, importance_score, scope_score, source_authority_score, final_score, rank, source_receipt_id, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (ranking_id) DO UPDATE SET final_score = EXCLUDED.final_score, rank = EXCLUDED.rank;\n",
            sql_string_literal(&ranking.ranking_id),
            sql_string_literal(ranking.tenant_id.as_str()),
            sql_string_literal(ranking.surface),
            sql_string_literal(&ranking.query),
            sql_string_literal(&ranking.memory_id),
            ranking.lexical_score,
            ranking.content_checksum_score,
            ranking.graph_score,
            ranking.recency_score,
            ranking.importance_score,
            ranking.scope_score,
            ranking.source_authority_score,
            ranking.final_score,
            ranking.rank,
            sql_string_literal(&ranking.source_receipt_id),
            sql_string_literal(&ranking.receipt_id)
        ));
    }
    for run in kernel.memory_brain_eval_runs() {
        sql.push_str(&format!(
            "INSERT INTO memory_brain_eval_runs (eval_run_id, tenant_id, fixture_family, fixture_count, correct_count, latency_budget_ms, observed_latency_ms, brain_score, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (eval_run_id) DO UPDATE SET brain_score = EXCLUDED.brain_score;\n",
            sql_string_literal(&run.eval_run_id),
            sql_string_literal(run.tenant_id.as_str()),
            sql_string_literal(run.fixture_family),
            run.fixture_count,
            run.correct_count,
            run.latency_budget_ms,
            run.observed_latency_ms,
            run.brain_score,
            sql_string_literal(&run.receipt_id)
        ));
    }
    for evaluation in kernel.memory_lifecycle_evaluations() {
        sql.push_str(&format!(
            "INSERT INTO memory_lifecycle_evaluations (evaluation_id, tenant_id, policy, evaluated_memory_count, stale_count, contradiction_count, supersession_count, trigger_receipt_id, trusted_time_floor, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (evaluation_id) DO UPDATE SET stale_count = EXCLUDED.stale_count, contradiction_count = EXCLUDED.contradiction_count, supersession_count = EXCLUDED.supersession_count;\n",
            sql_string_literal(&evaluation.evaluation_id),
            sql_string_literal(evaluation.tenant_id.as_str()),
            sql_string_literal(evaluation.policy),
            evaluation.evaluated_memory_count,
            evaluation.stale_count,
            evaluation.contradiction_count,
            evaluation.supersession_count,
            sql_string_literal(&evaluation.trigger_receipt_id),
            sql_string_literal(&evaluation.trusted_time_floor),
            sql_string_literal(&evaluation.receipt_id)
        ));
    }
    for result in kernel.memory_eval_fixture_results() {
        sql.push_str(&format!(
            "INSERT INTO memory_eval_fixture_results (fixture_result_id, tenant_id, fixture_family, query, expected_memory_id, matched_memory_id, final_score, passed, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (fixture_result_id) DO UPDATE SET matched_memory_id = EXCLUDED.matched_memory_id, final_score = EXCLUDED.final_score, passed = EXCLUDED.passed;\n",
            sql_string_literal(&result.fixture_result_id),
            sql_string_literal(result.tenant_id.as_str()),
            sql_string_literal(result.fixture_family),
            sql_string_literal(&result.query),
            sql_string_literal(&result.expected_memory_id),
            sql_string_literal(&result.matched_memory_id),
            result.final_score,
            result.passed,
            sql_string_literal(&result.receipt_id)
        ));
    }
    for run in kernel.memory_vendor_comparator_runs() {
        sql.push_str(&format!(
            "INSERT INTO memory_vendor_comparator_runs (comparator_run_id, tenant_id, vendor_id, status, accuracy_score, latency_ms, cost_micros, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (comparator_run_id) DO UPDATE SET status = EXCLUDED.status;\n",
            sql_string_literal(&run.comparator_run_id),
            sql_string_literal(run.tenant_id.as_str()),
            sql_string_literal(run.vendor_id),
            sql_string_literal(run.status),
            run.accuracy_score,
            run.latency_ms,
            run.cost_micros,
            sql_string_literal(&run.receipt_id)
        ));
    }
    for access in kernel.memory_surface_access() {
        sql.push_str(&format!(
            "INSERT INTO memory_surface_access (access_id, tenant_id, surface, scope, can_read, can_write, review_required, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (access_id) DO UPDATE SET can_read = EXCLUDED.can_read, can_write = EXCLUDED.can_write;\n",
            sql_string_literal(&access.access_id),
            sql_string_literal(access.tenant_id.as_str()),
            sql_string_literal(access.surface),
            sql_string_literal(access.scope),
            access.can_read,
            access.can_write,
            access.review_required,
            sql_string_literal(&access.receipt_id)
        ));
    }
    for check in kernel.memory_production_topology_checks() {
        sql.push_str(&format!(
            "INSERT INTO memory_production_topology_checks (topology_check_id, tenant_id, service, deployment_shape, latency_role, queue_or_cache, observed_latency_ms, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (topology_check_id) DO UPDATE SET observed_latency_ms = EXCLUDED.observed_latency_ms;\n",
            sql_string_literal(&check.topology_check_id),
            sql_string_literal(check.tenant_id.as_str()),
            sql_string_literal(check.service),
            sql_string_literal(check.deployment_shape),
            sql_string_literal(check.latency_role),
            sql_string_literal(check.queue_or_cache),
            check.observed_latency_ms,
            sql_string_literal(&check.receipt_id)
        ));
    }
    for event in kernel.memory_topology_runtime_events() {
        sql.push_str(&format!(
            "INSERT INTO memory_topology_runtime_events (topology_event_id, tenant_id, service, event_kind, queue_or_cache, cache_key, observed_latency_ms, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (topology_event_id) DO UPDATE SET observed_latency_ms = EXCLUDED.observed_latency_ms;\n",
            sql_string_literal(&event.topology_event_id),
            sql_string_literal(event.tenant_id.as_str()),
            sql_string_literal(event.service),
            sql_string_literal(event.event_kind),
            sql_string_literal(event.queue_or_cache),
            sql_string_literal(&event.cache_key),
            event.observed_latency_ms,
            sql_string_literal(&event.receipt_id)
        ));
    }
    for import in kernel.memory_benchmark_imports() {
        sql.push_str(&format!(
            "INSERT INTO memory_benchmark_imports (import_id, tenant_id, fixture_family, source_kind, task_shape, fixture_count, synthetic, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (import_id) DO UPDATE SET fixture_count = EXCLUDED.fixture_count;\n",
            sql_string_literal(&import.import_id),
            sql_string_literal(import.tenant_id.as_str()),
            sql_string_literal(import.fixture_family),
            sql_string_literal(import.source_kind),
            sql_string_literal(import.task_shape),
            import.fixture_count,
            import.synthetic,
            sql_string_literal(&import.receipt_id)
        ));
    }
    for run in kernel.memory_scale_load_runs() {
        sql.push_str(&format!(
            "INSERT INTO memory_scale_load_runs (scale_run_id, tenant_id, synthetic_session_count, memory_record_count, ranking_count, latency_budget_ms, observed_p95_latency_ms, brain_score, receipt_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}) ON CONFLICT (scale_run_id) DO UPDATE SET observed_p95_latency_ms = EXCLUDED.observed_p95_latency_ms, brain_score = EXCLUDED.brain_score;\n",
            sql_string_literal(&run.scale_run_id),
            sql_string_literal(run.tenant_id.as_str()),
            run.synthetic_session_count,
            run.memory_record_count,
            run.ranking_count,
            run.latency_budget_ms,
            run.observed_p95_latency_ms,
            run.brain_score,
            sql_string_literal(&run.receipt_id)
        ));
    }
    for check in kernel.memory_cloud_turn_on_checks() {
        sql.push_str(&format!(
            "INSERT INTO memory_cloud_turn_on_checks (check_id, tenant_id, check_kind, status, evidence, receipt_id) VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT (check_id) DO UPDATE SET status = EXCLUDED.status, evidence = EXCLUDED.evidence;\n",
            sql_string_literal(&check.check_id),
            sql_string_literal(check.tenant_id.as_str()),
            sql_string_literal(check.check_kind),
            sql_string_literal(check.status),
            sql_string_literal(&check.evidence),
            sql_string_literal(&check.receipt_id)
        ));
    }
    sql.push_str("SELECT count(*) FROM memory_records;\n");
    sql.push_str("SELECT count(*) FROM memory_graph_nodes;\n");
    sql.push_str("SELECT count(*) FROM memory_lifecycle_events;\n");
    sql.push_str("SELECT count(*) FROM memory_lifecycle_evaluations;\n");
    sql.push_str("SELECT count(*) FROM memory_recall_rankings;\n");
    sql.push_str("SELECT count(*) FROM memory_brain_eval_runs;\n");
    sql.push_str("SELECT count(*) FROM memory_eval_fixture_results;\n");
    sql.push_str("SELECT count(*) FROM memory_topology_runtime_events;\n");
    sql.push_str("SELECT count(*) FROM memory_benchmark_imports;\n");
    sql.push_str("SELECT count(*) FROM memory_scale_load_runs;\n");
    sql.push_str("SELECT count(*) FROM memory_cloud_turn_on_checks;\n");
    sql
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
use crate::MdxKernel;
