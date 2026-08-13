// Learn candidate rejection records the accountable no on a drafted lesson.
// It verifies the source receipt in the ledger, refuses a double rejection,
// and removes the candidate from the draft lane without deleting history,
// writing memory, refreshing generated artifacts, or adapting behavior.
use crate::RouteResponse;
use mdx_core::{LearningCandidateRejection, MdxKernel, json_string_literal};
use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/candidate-rejections.json" => Some(handle_post(method, body, kernel)),
        "/learning/candidate-rejections/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

/// The source receipt ids whose candidates were rejected. The draft lane
/// excludes these sources so a rejected candidate does not reappear, while
/// the rejection receipts stay visible in the projection.
pub(crate) fn rejected_source_receipt_ids(kernel: &MdxKernel) -> BTreeSet<String> {
    kernel
        .ledger()
        .query()
        .by_kind("learning.candidate.rejected")
        .iter()
        .filter_map(|receipt| receipt.payload.get("source_receipt_id").cloned())
        .filter(|receipt_id| !receipt_id.is_empty())
        .collect()
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
    let report = match kernel.reject_learning_candidate_with_identity(
        LearningCandidateRejection {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            candidate_id: &field("candidate_id"),
            source_receipt_id: &field("source_receipt_id"),
            reason: &field("reason"),
            review_owner: &field("review_owner"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-candidate-rejection-local-post","status":"RECORDED","auth_session_status":{},"candidate_rejection_id":{},"candidate_id":{},"source_receipt_id":{},"lane_state":{},"candidate_rejection_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/candidate-rejections/projection.json","candidate_excluded_from_drafts":true,"receipts_deleted":false,"memory_write_allowed":false,"generated_refresh_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.candidate_rejection_id),
            json_string_literal(&report.candidate_id),
            json_string_literal(&report.source_receipt_id),
            json_string_literal(&report.lane_state),
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
    let rejections: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.candidate.rejected")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"candidate_rejection_receipt_id":{},"candidate_rejection_id":{},"candidate_id":{},"source_receipt_id":{},"source_receipt_kind":{},"reason":{},"review_owner":{},"actor_id":{},"lane_state":{},"candidate_excluded_from_drafts":true,"receipts_deleted":false,"memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("candidate_rejection_id"),
                value("candidate_id"),
                value("source_receipt_id"),
                value("source_receipt_kind"),
                value("reason"),
                value("review_owner"),
                json_string_literal(receipt.actor_id.as_str()),
                value("lane_state"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-candidate-rejection-local-projection","receipt_kind":"learning.candidate.rejected","writes_route":"/learning/candidate-rejections.json","rejection_count":{},"rejections":[{}],"candidate_excluded_from_drafts":true,"receipts_deleted":false,"memory_write_allowed":false,"generated_refresh_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            rejections.len(),
            rejections.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-candidate-rejection-local-post","status":"REFUSED","reason":{},"candidate_rejection_receipt_id":"","candidate_excluded_from_drafts":false,"receipts_deleted":false,"memory_write_allowed":false,"generated_refresh_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
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
    use mdx_core::ForgeOutcomeSignal;

    fn outcome_signal_receipt_id(kernel: &Arc<RwLock<MdxKernel>>) -> String {
        let mut kernel = kernel.write().expect("kernel");
        kernel.run_evals_runner_agent().expect("seed receipt");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .expect("source receipt")
            .receipt_id
            .clone();
        kernel
            .record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "local_tenant",
                actor_id: "agent:forge",
                run_id: "forge_run_1",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Run completed.",
                capability_ids: "svelte_ui_pack",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Run UI checks for similar work.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome signal")
            .receipt_id
    }

    #[test]
    fn post_rejects_candidate_and_projection_lists_it() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let source_receipt_id = outcome_signal_receipt_id(&kernel);
        let post = route_response(
            "POST",
            "/learning/candidate-rejections.json",
            &format!(
                r#"{{"candidate_id":"learning_candidate_forge_outcome_signal_1","source_receipt_id":"{source_receipt_id}","reason":"The lesson overfits one run.","review_owner":"human:eng"}}"#
            ),
            &kernel,
        )
        .expect("candidate rejection route")
        .expect("candidate rejection response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(post.body.contains("\"lane_state\":\"rejected\""));
        assert!(
            post.body
                .contains("\"candidate_excluded_from_drafts\":true")
        );
        assert!(post.body.contains("\"memory_write_allowed\":false"));

        let projection = route_response(
            "GET",
            "/learning/candidate-rejections/projection.json",
            "",
            &kernel,
        )
        .expect("candidate rejection projection route")
        .expect("candidate rejection projection response");
        assert_eq!(projection.status, "200 OK");
        assert!(projection.body.contains("\"rejection_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"learning.candidate.rejected\"")
        );
        assert!(
            projection
                .body
                .contains(&format!("\"source_receipt_id\":\"{source_receipt_id}\""))
        );

        let rejected = {
            let kernel = kernel.read().expect("kernel");
            rejected_source_receipt_ids(&kernel)
        };
        assert!(rejected.contains(&source_receipt_id));
    }

    #[test]
    fn post_refuses_unknown_source_and_double_rejection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let unknown = route_response(
            "POST",
            "/learning/candidate-rejections.json",
            r#"{"candidate_id":"learning_candidate_missing","source_receipt_id":"receipt_that_does_not_exist","reason":"No source evidence.","review_owner":"human:eng"}"#,
            &kernel,
        )
        .expect("candidate rejection route")
        .expect("candidate rejection response");
        assert!(unknown.body.contains("\"status\":\"REFUSED\""));
        assert!(unknown.body.contains("is unknown"));

        let source_receipt_id = outcome_signal_receipt_id(&kernel);
        let body = format!(
            r#"{{"candidate_id":"learning_candidate_forge_outcome_signal_1","source_receipt_id":"{source_receipt_id}","reason":"The lesson overfits one run.","review_owner":"human:eng"}}"#
        );
        let first = route_response(
            "POST",
            "/learning/candidate-rejections.json",
            &body,
            &kernel,
        )
        .expect("candidate rejection route")
        .expect("candidate rejection response");
        assert!(first.body.contains("\"status\":\"RECORDED\""));
        let second = route_response(
            "POST",
            "/learning/candidate-rejections.json",
            &body,
            &kernel,
        )
        .expect("candidate rejection route")
        .expect("candidate rejection response");
        assert!(second.body.contains("\"status\":\"REFUSED\""));
        assert!(second.body.contains("already rejected"));
    }
}
