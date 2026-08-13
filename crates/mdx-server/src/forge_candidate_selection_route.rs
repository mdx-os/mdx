// Human candidate selection for bounded parallel Forge runs. This records the
// operator's chosen candidate after deterministic comparison, without pushing,
// opening a PR, or granting delivery authority.
use crate::RouteResponse;
use mdx_core::{ForgeRunEvent, GovernedWriteIdentity, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/candidate-selections.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    Some(handle(body, kernel))
}

fn handle(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let run_id = json_string_field(&parsed, "run_id");
    if run_id.trim().is_empty() {
        return Ok(refusal(
            "name the run whose candidate set should be selected",
        ));
    }
    if !json_bool_field(&parsed, "confirm_selection") {
        return Ok(refusal(
            "confirm_selection=true is required before Forge records a candidate selection",
        ));
    }
    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, &run_id)?;
    let packet: serde_json::Value = match serde_json::from_str(&packet_body) {
        Ok(value) => value,
        Err(_) => return Ok(refusal("review packet did not return valid JSON")),
    };
    if packet["status"].as_str() != Some("OK") {
        let reason = packet["reason"]
            .as_str()
            .unwrap_or("review packet is not available for this run");
        return Ok(refusal(reason));
    }
    let comparison = &packet["candidate_comparison"];
    let candidate_count = comparison["candidate_count"].as_u64().unwrap_or(1);
    if candidate_count <= 1 {
        return Ok(refusal(
            "candidate selection is only needed after bounded parallel execution",
        ));
    }
    let selected_run_id = json_string_field(&parsed, "selected_run_id");
    if selected_run_id.trim().is_empty() {
        return Ok(refusal("selected_run_id is required"));
    }
    let candidate_run_ids = comparison["candidates"]
        .as_array()
        .map(|candidates| {
            candidates
                .iter()
                .filter_map(|candidate| candidate["run_id"].as_str())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !candidate_run_ids
        .iter()
        .any(|candidate_run_id| candidate_run_id == selected_run_id.trim())
    {
        return Ok(refusal("selected_run_id is not in this candidate set"));
    }
    let primary_run_id = comparison["primary_run_id"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            comparison["current_run_id"]
                .as_str()
                .unwrap_or(run_id.trim())
        });
    let recommended_run_id = comparison["recommended_run_id"].as_str().unwrap_or("");
    let selected_matches_recommendation =
        recommended_run_id.trim().is_empty() || selected_run_id.trim() == recommended_run_id.trim();
    let override_reason = json_string_field(&parsed, "override_recommendation_reason");
    if !selected_matches_recommendation && override_reason.trim().is_empty() {
        return Ok(refusal(
            "override_recommendation_reason is required when selecting a non-recommended candidate",
        ));
    }
    let actor_id = json_string_field(&parsed, "actor_id").trim().to_string();
    let actor_id = if actor_id.is_empty() {
        "human:forge_operator".to_string()
    } else {
        actor_id
    };
    let selection_reason = json_string_field(&parsed, "selection_reason");
    let candidate_count_text = candidate_count.to_string();
    let mut receipt_ids = Vec::new();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let selection_id = kernel.mint_id("forge_candidate_selection");
    let identity = GovernedWriteIdentity::local_demo(&actor_id);
    let selected_matches_recommendation_text = if selected_matches_recommendation {
        "true"
    } else {
        "false"
    };
    let override_reason_recorded = if override_reason.trim().is_empty() {
        "false"
    } else {
        "true"
    };
    for candidate_run_id in &candidate_run_ids {
        let current_run_is_selected = if candidate_run_id == selected_run_id.trim() {
            "true"
        } else {
            "false"
        };
        let report = kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: &actor_id,
                    run_id: candidate_run_id,
                    event: "evidence_appended",
                    work_item_id: "forge_candidate_selection",
                    detail: &format!(
                        "candidate_selection_recorded selection_id={} selected_run_id={} primary_run_id={}",
                        selection_id,
                        selected_run_id.trim(),
                        primary_run_id.trim()
                    ),
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    ("candidate_selection_id", selection_id.as_str()),
                    ("candidate_selection_primary_run_id", primary_run_id.trim()),
                    ("candidate_selection_selected_run_id", selected_run_id.trim()),
                    ("candidate_selection_current_run_id", candidate_run_id.as_str()),
                    (
                        "candidate_selection_current_run_is_selected",
                        current_run_is_selected,
                    ),
                    (
                        "candidate_selection_recommended_run_id",
                        recommended_run_id.trim(),
                    ),
                    (
                        "candidate_selection_matches_recommendation",
                        selected_matches_recommendation_text,
                    ),
                    (
                        "candidate_selection_override_reason_recorded",
                        override_reason_recorded,
                    ),
                    ("candidate_selection_reason", selection_reason.trim()),
                    (
                        "candidate_selection_override_recommendation_reason",
                        override_reason.trim(),
                    ),
                    ("candidate_selection_candidate_count", candidate_count_text.as_str()),
                    ("candidate_selection_actor_id", actor_id.as_str()),
                    ("candidate_selection_grants_delivery_authority", "false"),
                    ("candidate_selection_production_write_allowed", "false"),
                ],
            )
            .map_err(|error| error.message())?;
        receipt_ids.push(report.receipt_id);
    }
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-candidate-selection-local-post","status":"RECORDED","route":"/forge/candidate-selections.json","review_packet_route":"/forge/review-packet.json","source_host_pr_draft_route":"/forge/source-host-pr-drafts.json","selection_id":{},"primary_run_id":{},"selected_run_id":{},"recommended_run_id":{},"candidate_count":{},"receipt_ids":[{}],"selected_matches_recommendation":{},"override_reason_recorded":{},"selection_grants_delivery_authority":false,"production_write_allowed":false}}"#,
            json_string_literal(&selection_id),
            json_string_literal(primary_run_id.trim()),
            json_string_literal(selected_run_id.trim()),
            json_string_literal(recommended_run_id.trim()),
            candidate_count,
            receipt_ids
                .iter()
                .map(|receipt_id| json_string_literal(receipt_id))
                .collect::<Vec<_>>()
                .join(","),
            selected_matches_recommendation,
            !override_reason.trim().is_empty()
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-candidate-selection-local-post","status":"REFUSED","reason":{},"receipt_ids":[],"selection_grants_delivery_authority":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn json_string_field(value: &serde_json::Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").trim().to_string()
}

fn json_bool_field(value: &serde_json::Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_candidate_selection_without_explicit_confirmation() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/candidate-selections.json",
            r#"{"run_id":"forge_run_1","selected_run_id":"forge_run_1"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("confirm_selection=true"));
        assert!(
            response
                .body
                .contains(r#""selection_grants_delivery_authority":false"#)
        );
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn refuses_candidate_selection_when_review_packet_is_unavailable() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/candidate-selections.json",
            r#"{"run_id":"forge_run_missing","selected_run_id":"forge_run_missing","confirm_selection":true}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(
            response
                .body
                .contains(r#""selection_grants_delivery_authority":false"#)
        );
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }
}
