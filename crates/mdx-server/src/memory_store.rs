use mdx_core::{MdxKernel, json_string_literal};

pub(crate) use crate::memory_store_eval::{
    render_beta_readiness_json, render_comparators_json, render_evals_json, render_governance_json,
    render_topology_json,
};
pub(crate) use crate::memory_store_graph::{
    render_graph_json, render_lifecycle_json, render_rankings_json,
};
pub(crate) use crate::memory_store_runtime::{
    render_brain_map_json, render_brain_runtime_json, render_brain_substrate_json,
};

/// The tenant a verified read session is scoped to, if any. Local-demo
/// reads carry no verified identity and keep the shared local world; a
/// verified trusted session must never see another tenant's memory content.
/// The serving kernel is otherwise one shared world, so without this filter
/// the memory projections would answer a verified tenant with every
/// tenant's memory.
pub(crate) fn verified_read_tenant() -> Option<String> {
    crate::request_security::current_verified_identity().map(|identity| identity.tenant_id)
}

pub(crate) fn tenant_visible(read_tenant: Option<&str>, tenant_id: &str) -> bool {
    read_tenant.is_none_or(|tenant| tenant == tenant_id)
}

pub(crate) fn render_records_json(kernel: &MdxKernel) -> String {
    let read_tenant = verified_read_tenant();
    let records = kernel
        .memory_records()
        .iter()
        .filter(|record| tenant_visible(read_tenant.as_deref(), record.tenant_id.as_str()))
        .map(|record| {
            format!(
                r#"{{"memory_id":{},"episode_id":{},"tenant_id":{},"source_receipt_id":{},"source_receipt_kind":{},"atom_origin":{},"valid_from_receipt_timestamp":{},"valid_until_receipt_timestamp":{},"invalidated_by_receipt_id":{},"consolidation_state":{},"pending_ratification":{},"consolidation_decision":{},"memory_scope":{},"memory_tier":{},"decay_policy":{},"importance_score":{},"memory_driver":{},"memory_provider":{},"memory_durable_driver":{},"memory_durable_table":{},"live_database_write_allowed":false,"consolidation_gate":{},"gate_receipt_id":{},"temporal_status":{},"content":{},"production_write_allowed":false}}"#,
                json_string_literal(&record.memory_id),
                json_string_literal(&record.episode_id),
                json_string_literal(record.tenant_id.as_str()),
                json_string_literal(&record.source_receipt_id),
                json_string_literal(&record.provenance.source_receipt_kind),
                json_string_literal(record.atom_origin),
                json_string_literal(&record.valid_from_receipt_timestamp),
                json_string_literal(&record.valid_until_receipt_timestamp),
                json_string_literal(&record.invalidated_by_receipt_id),
                json_string_literal(record.consolidation_state),
                record.consolidation_state == mdx_core::MEMORY_CONSOLIDATION_PENDING,
                json_string_literal(record.consolidation_decision.as_str()),
                json_string_literal(record.memory_scope),
                json_string_literal(record.memory_tier),
                json_string_literal(record.decay_policy),
                record.importance_score,
                json_string_literal(record.provenance.driver_id),
                json_string_literal(record.provenance.provider),
                json_string_literal(record.provenance.durable_driver),
                json_string_literal(record.provenance.durable_table),
                json_string_literal(record.provenance.consolidation_gate),
                json_string_literal(&record.provenance.gate_receipt_id),
                json_string_literal(record.provenance.temporal_status),
                json_string_literal(&record.content)
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-memory-store-records","status":"OK","read_only":true,"writes_allowed":false,"auth_session_route":"/local/auth-session.json","memory_driver":"local_memory_store","memory_provider":"InMemoryProvider","memory_durable_driver":"postgres_memory_records","memory_durable_table":"memory_records","memory_durable_status":"POSTGRES_DURABLE_MEMORY_SHAPE","live_database_write_allowed":false,"vendor_memory_driver":"mem0_memory_store","vendor_status":"PENDING-LIVE-RUN","memory_record_count":{},"live_substrate_required":false,"production_write_allowed":false,"records":[{}]}}"#,
        records.len(),
        records.join(",")
    )
}

pub(crate) fn json_string_array<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    values
        .into_iter()
        .map(json_string_literal)
        .collect::<Vec<_>>()
        .join(",")
}
