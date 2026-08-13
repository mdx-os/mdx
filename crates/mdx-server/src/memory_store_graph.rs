use mdx_core::{MdxKernel, json_string_literal};

use crate::memory_store::{json_string_array, tenant_visible, verified_read_tenant};

pub(crate) fn render_graph_json(kernel: &MdxKernel) -> String {
    let read_tenant = verified_read_tenant();
    let source_receipt_ids = json_string_array(
        kernel
            .memory_graph_nodes()
            .iter()
            .filter(|node| tenant_visible(read_tenant.as_deref(), node.tenant_id.as_str()))
            .map(|node| node.source_receipt_id.as_str())
            .chain(
                kernel
                    .memory_graph_edges()
                    .iter()
                    .filter(|edge| tenant_visible(read_tenant.as_deref(), edge.tenant_id.as_str()))
                    .map(|edge| edge.source_receipt_id.as_str()),
            ),
    );
    let nodes = kernel
        .memory_graph_nodes()
        .iter()
        .filter(|node| tenant_visible(read_tenant.as_deref(), node.tenant_id.as_str()))
        .map(|node| {
            format!(
                r#"{{"node_id":{},"tenant_id":{},"node_kind":{},"label":{},"memory_id":{},"source_receipt_id":{},"atom_origin":{},"valid_from_receipt_timestamp":{},"lifecycle_state":{}}}"#,
                json_string_literal(&node.node_id),
                json_string_literal(node.tenant_id.as_str()),
                json_string_literal(node.node_kind),
                json_string_literal(&node.label),
                json_string_literal(node.memory_id.as_deref().unwrap_or("")),
                json_string_literal(&node.source_receipt_id),
                json_string_literal(node.atom_origin),
                json_string_literal(&node.valid_from_receipt_timestamp),
                json_string_literal(node.lifecycle_state)
            )
        })
        .collect::<Vec<_>>();
    let edges = kernel
        .memory_graph_edges()
        .iter()
        .filter(|edge| tenant_visible(read_tenant.as_deref(), edge.tenant_id.as_str()))
        .map(|edge| {
            format!(
                r#"{{"edge_id":{},"tenant_id":{},"from_node_id":{},"to_node_id":{},"edge_kind":{},"source_receipt_id":{},"weight":{},"valid_from_receipt_timestamp":{}}}"#,
                json_string_literal(&edge.edge_id),
                json_string_literal(edge.tenant_id.as_str()),
                json_string_literal(&edge.from_node_id),
                json_string_literal(&edge.to_node_id),
                json_string_literal(edge.edge_kind),
                json_string_literal(&edge.source_receipt_id),
                edge.weight,
                json_string_literal(&edge.valid_from_receipt_timestamp)
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-memory-brain-graph","status":"LOCAL_GRAPH_STORAGE_READY","route":"/memory/graph.json","read_only":true,"node_count":{},"edge_count":{},"temporal_truth_requires_trusted_time":true,"node_storage":"memory_graph_nodes","edge_storage":"memory_graph_edges","source_receipt_ids":[{}],"nodes":[{}],"edges":[{}]}}"#,
        nodes.len(),
        edges.len(),
        source_receipt_ids,
        nodes.join(","),
        edges.join(",")
    )
}

pub(crate) fn render_lifecycle_json(kernel: &MdxKernel) -> String {
    let read_tenant = verified_read_tenant();
    let source_receipt_ids = json_string_array(
        kernel
            .memory_lifecycle_events()
            .iter()
            .filter(|event| tenant_visible(read_tenant.as_deref(), event.tenant_id.as_str()))
            .map(|event| event.receipt_id.as_str())
            .chain(
                kernel
                    .memory_lifecycle_evaluations()
                    .iter()
                    .filter(|evaluation| {
                        tenant_visible(read_tenant.as_deref(), evaluation.tenant_id.as_str())
                    })
                    .map(|evaluation| evaluation.receipt_id.as_str()),
            ),
    );
    let events = kernel
        .memory_lifecycle_events()
        .iter()
        .filter(|event| tenant_visible(read_tenant.as_deref(), event.tenant_id.as_str()))
        .map(|event| {
            format!(
                r#"{{"event_id":{},"tenant_id":{},"memory_id":{},"action":{},"lifecycle_state":{},"reason":{},"source_receipt_id":{},"valid_from_receipt_timestamp":{},"receipt_id":{}}}"#,
                json_string_literal(&event.event_id),
                json_string_literal(event.tenant_id.as_str()),
                json_string_literal(&event.memory_id),
                json_string_literal(event.action),
                json_string_literal(event.lifecycle_state),
                json_string_literal(&event.reason),
                json_string_literal(&event.source_receipt_id),
                json_string_literal(&event.valid_from_receipt_timestamp),
                json_string_literal(&event.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    let evaluations = kernel
        .memory_lifecycle_evaluations()
        .iter()
        .filter(|evaluation| tenant_visible(read_tenant.as_deref(), evaluation.tenant_id.as_str()))
        .map(|evaluation| {
            format!(
                r#"{{"evaluation_id":{},"tenant_id":{},"policy":{},"evaluated_memory_count":{},"stale_count":{},"contradiction_count":{},"supersession_count":{},"trigger_receipt_id":{},"trusted_time_floor":{},"receipt_id":{}}}"#,
                json_string_literal(&evaluation.evaluation_id),
                json_string_literal(evaluation.tenant_id.as_str()),
                json_string_literal(evaluation.policy),
                evaluation.evaluated_memory_count,
                evaluation.stale_count,
                evaluation.contradiction_count,
                evaluation.supersession_count,
                json_string_literal(&evaluation.trigger_receipt_id),
                json_string_literal(&evaluation.trusted_time_floor),
                json_string_literal(&evaluation.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-memory-brain-lifecycle","status":"LOCAL_LIFECYCLE_READY","route":"/memory/lifecycle.json","action_route":"/memory/lifecycle-actions.json","evaluation_route":"/memory/lifecycle-evaluations.json","read_only":true,"event_count":{},"evaluation_count":{},"trusted_time_required":true,"storage":"memory_lifecycle_events","evaluation_storage":"memory_lifecycle_evaluations","source_receipt_ids":[{}],"events":[{}],"evaluations":[{}]}}"#,
        events.len(),
        evaluations.len(),
        source_receipt_ids,
        events.join(","),
        evaluations.join(",")
    )
}

pub(crate) fn render_rankings_json(kernel: &MdxKernel) -> String {
    let read_tenant = verified_read_tenant();
    let source_receipt_ids = json_string_array(
        kernel
            .memory_recall_rankings()
            .iter()
            .filter(|ranking| tenant_visible(read_tenant.as_deref(), ranking.tenant_id.as_str()))
            .map(|ranking| ranking.receipt_id.as_str()),
    );
    let rankings = kernel
        .memory_recall_rankings()
        .iter()
        .filter(|ranking| tenant_visible(read_tenant.as_deref(), ranking.tenant_id.as_str()))
        .map(|ranking| {
            format!(
                r#"{{"ranking_id":{},"tenant_id":{},"surface":{},"query":{},"memory_id":{},"lexical_score":{},"content_checksum_score":{},"graph_score":{},"recency_score":{},"importance_score":{},"scope_score":{},"source_authority_score":{},"final_score":{},"rank":{},"source_receipt_id":{},"receipt_id":{}}}"#,
                json_string_literal(&ranking.ranking_id),
                json_string_literal(ranking.tenant_id.as_str()),
                json_string_literal(ranking.surface),
                json_string_literal(&ranking.query),
                json_string_literal(&ranking.memory_id),
                ranking.lexical_score,
                ranking.content_checksum_score,
                ranking.graph_score,
                ranking.recency_score,
                ranking.importance_score,
                ranking.scope_score,
                ranking.source_authority_score,
                ranking.final_score,
                ranking.rank,
                json_string_literal(&ranking.source_receipt_id),
                json_string_literal(&ranking.receipt_id)
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-memory-brain-recall-rankings","status":"LOCAL_RANKING_READY","route":"/memory/recall-rankings.json","read_only":true,"ranking_count":{},"components":["lexical","vector","graph","recency","importance","scope","source_authority"],"storage":"memory_recall_rankings","source_receipt_ids":[{}],"rankings":[{}]}}"#,
        rankings.len(),
        source_receipt_ids,
        rankings.join(",")
    )
}
