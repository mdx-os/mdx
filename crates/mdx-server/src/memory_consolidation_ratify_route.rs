// Memory consolidation ratification: the governed clear for shared-scope
// memory that landed pending. The kernel refuses self-ratification (the
// proposer on the gate receipt cannot approve their own consolidation), so
// this route is what retires the old self-approving review stub. The
// projection lists the pending queue so /learn can show it.
use crate::RouteResponse;
use mdx_core::{MdxKernel, MemoryConsolidationRatification, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/memory/consolidation-ratifications.json" => Some(handle_post(method, body, kernel)),
        "/memory/consolidation-ratifications/projection.json" => {
            Some(handle_projection(method, kernel))
        }
        _ => None,
    }
}

fn handle_post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.ratify_memory_consolidation(MemoryConsolidationRatification {
        tenant_id: &resolved.tenant_id,
        actor_id: &resolved.actor_id,
        memory_id: &field("memory_id"),
        decision: &field("decision"),
        note: &field("note"),
    }) {
        Ok(report) => report,
        Err(reason) => return Ok(refusal(&reason)),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-memory-consolidation-ratification-local-post","status":"RECORDED","auth_session_status":{},"memory_id":{},"ratification_decision":{},"consolidation_state":{},"proposed_by":{},"ratified_by":{},"ratification_receipt_id":{},"policy_decision_id":{},"projection_route":"/memory/consolidation-ratifications/projection.json","distinct_actor_required":true,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.memory_id),
            json_string_literal(&report.ratification_decision),
            json_string_literal(report.consolidation_state),
            json_string_literal(&report.proposed_by),
            json_string_literal(&report.ratified_by),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let read_tenant = crate::memory_store::verified_read_tenant();
    let pending: Vec<String> = kernel
        .pending_memory_consolidations()
        .iter()
        .filter(|record| {
            crate::memory_store::tenant_visible(read_tenant.as_deref(), record.tenant_id.as_str())
        })
        .map(|record| {
            let proposed_by = kernel
                .ledger()
                .query()
                .by_id(&record.provenance.gate_receipt_id)
                .map(|receipt| receipt.actor_id.as_str().to_string())
                .unwrap_or_default();
            format!(
                r#"{{"memory_id":{},"memory_scope":{},"content":{},"proposed_by":{},"source_receipt_id":{},"gate_receipt_id":{},"valid_from_receipt_timestamp":{},"consolidation_state":{}}}"#,
                json_string_literal(&record.memory_id),
                json_string_literal(record.memory_scope),
                json_string_literal(&record.content),
                json_string_literal(&proposed_by),
                json_string_literal(&record.source_receipt_id),
                json_string_literal(&record.provenance.gate_receipt_id),
                json_string_literal(&record.valid_from_receipt_timestamp),
                json_string_literal(record.consolidation_state),
            )
        })
        .collect();
    let ratified: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("memory.consolidation.ratified")
        .iter()
        .filter(|receipt| {
            crate::memory_store::tenant_visible(read_tenant.as_deref(), receipt.tenant_id.as_str())
        })
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            let memory_id = receipt
                .payload
                .get("memory_id")
                .map(String::as_str)
                .unwrap_or("");
            let record = kernel
                .memory_records()
                .iter()
                .find(|record| record.memory_id == memory_id);
            let record_value = |value: Option<&str>| json_string_literal(value.unwrap_or(""));
            format!(
                r#"{{"ratification_receipt_id":{},"memory_id":{},"memory_scope":{},"content":{},"source_receipt_id":{},"gate_receipt_id":{},"ratification_decision":{},"consolidation_state":{},"proposed_by":{},"ratified_by":{},"note":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("memory_id"),
                record_value(record.map(|record| record.memory_scope)),
                record_value(record.map(|record| record.content.as_str())),
                record_value(record.map(|record| record.source_receipt_id.as_str())),
                record_value(record.map(|record| record.provenance.gate_receipt_id.as_str())),
                value("ratification_decision"),
                value("consolidation_state"),
                value("proposed_by"),
                value("ratified_by"),
                value("note"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-memory-consolidation-ratification-local-projection","status":"OK","receipt_kind":"memory.consolidation.ratified","writes_route":"/memory/consolidation-ratifications.json","pending_count":{},"pending":[{}],"ratification_count":{},"ratifications":[{}],"distinct_actor_required":true,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            pending.len(),
            pending.join(","),
            ratified.len(),
            ratified.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-memory-consolidation-ratification-local-post","status":"REFUSED","reason":{},"ratification_receipt_id":"","distinct_actor_required":true,"adaptation_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    value.push(escaped);
                }
            }
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel_with_pending() -> (Arc<RwLock<MdxKernel>>, String) {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("loop run");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .last()
            .expect("source receipt")
            .receipt_id
            .clone();
        let memory_id = kernel
            .record_surface_memory_local(
                "local_tenant",
                "agent:proposer",
                "messages",
                "team_memory",
                &source_receipt_id,
                "Team decided the beta gate ships first",
            )
            .expect("pending memory");
        (Arc::new(RwLock::new(kernel)), memory_id)
    }

    #[test]
    fn projection_lists_pending_and_distinct_actor_post_activates() {
        let (kernel, memory_id) = kernel_with_pending();
        let projection = route_response(
            "GET",
            "/memory/consolidation-ratifications/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert!(projection.body.contains("\"pending_count\":1"));
        assert!(projection.body.contains(&memory_id));
        assert!(
            projection
                .body
                .contains("\"proposed_by\":\"agent:proposer\"")
        );

        let post = route_response(
            "POST",
            "/memory/consolidation-ratifications.json",
            &format!(
                r#"{{"actor_id":"human:reviewer","memory_id":"{memory_id}","decision":"approve","note":"matches the thread"}}"#
            ),
            &kernel,
        )
        .expect("ratify route")
        .expect("ratify response");
        assert!(
            post.body.contains("\"status\":\"RECORDED\""),
            "{}",
            post.body
        );
        assert!(post.body.contains("\"consolidation_state\":\"active\""));

        let projection = route_response(
            "GET",
            "/memory/consolidation-ratifications/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert!(projection.body.contains("\"pending_count\":0"));
        assert!(projection.body.contains("\"ratification_count\":1"));
        assert!(projection.body.contains("\"memory_scope\":\"team_memory\""));
        assert!(
            projection
                .body
                .contains("\"content\":\"Team decided the beta gate ships first\"")
        );
        assert!(projection.body.contains("\"source_receipt_id\":"));
        assert!(projection.body.contains("\"gate_receipt_id\":"));
    }

    #[test]
    fn self_ratification_is_refused_through_the_route() {
        let (kernel, memory_id) = kernel_with_pending();
        let post = route_response(
            "POST",
            "/memory/consolidation-ratifications.json",
            &format!(
                r#"{{"actor_id":"agent:proposer","memory_id":"{memory_id}","decision":"approve","note":""}}"#
            ),
            &kernel,
        )
        .expect("ratify route")
        .expect("ratify response");
        assert!(
            post.body.contains("\"status\":\"REFUSED\""),
            "{}",
            post.body
        );
        assert!(post.body.contains("distinct actor"));
    }

    #[test]
    fn route_cannot_bypass_distinct_person_check_by_dropping_actor_prefix() {
        let (kernel, memory_id) = kernel_with_pending();
        let post = route_response(
            "POST",
            "/memory/consolidation-ratifications.json",
            &format!(
                r#"{{"actor_id":"proposer","memory_id":"{memory_id}","decision":"approve","note":"same person without a prefix"}}"#
            ),
            &kernel,
        )
        .expect("ratify route")
        .expect("ratify response");
        assert!(
            post.body.contains("\"status\":\"REFUSED\""),
            "{}",
            post.body
        );
        assert!(post.body.contains("distinct actor"));
    }
}
