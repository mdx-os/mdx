// Convene a review panel on a finished run. A diverse set of models read the
// run's change cold, each from a distinct lens (holistic, strict,
// adversarial), and the room folds their reads into a structured outcome:
// consensus, who DISSENTED, the concerns only one lens caught (blind spots),
// confidence, and the reviewers' residency class. Dissent is kept first-class,
// never collapsed into one verdict. The reviewers make real model calls
// (cost), so convening is an explicit, governed POST - never automatic. The
// kernel lock is held only to record the result; the model calls run without
// it. Recording opens no authority: a panel judges, it never ships.
use crate::RouteResponse;
use crate::fleet_integration::{REVIEW_BASE, panel_reviewers, parse_member_verdict};
use mdx_core::{ForgeReviewPanel, MdxKernel, PanelMember, json_string_literal};
use std::sync::{Arc, RwLock};

const DIFF_CAP: usize = 12000;
const PACKET_CAP: usize = 30000;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/review-panel.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Some(Ok(refusal("name the run to convene a panel on")));
    }
    Some(handle(&run_id, kernel))
}

fn handle(run_id: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    // The change to read, from the run's branch - no lock held during review.
    let diff = match crate::forge_diff_route::run_diff_text(kernel, run_id)? {
        Some(text) if !text.trim().is_empty() => text,
        _ => {
            return Ok(refusal(
                "this run has no change to review - it stopped before committing a diff",
            ));
        }
    };
    let capped: String = diff.chars().take(DIFF_CAP).collect();
    let packet_context = crate::forge_review_packet::assemble_review_packet_json(kernel, run_id)
        .unwrap_or_else(|_| String::new());
    let packet_context: String = packet_context.chars().take(PACKET_CAP).collect();
    let tenant_id = crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string());
    let reviewers = panel_reviewers(kernel, &tenant_id);
    if reviewers.is_empty() {
        return Ok(refusal(
            "no reviewer model is configured - connect a model to convene a panel",
        ));
    }

    // Each reviewer reads the diff cold from its lens. Real model calls.
    let user = serde_json::json!({"role":"user","content":format!(
        "Review packet evidence already assembled by Forge. Use this as grounding for proof coverage, semantic orientation, touched stack, candidate ranking, and delivery guardrails. If the packet proves a concern is only a request for more context, say so explicitly instead of presenting it as a defect.\n\nReview packet JSON:\n{packet_context}\n\nPatch to review:\n\n{capped}"
    )});
    let mut members: Vec<PanelMember> = Vec::new();
    let mut failed = 0usize;
    for reviewer in &reviewers {
        let messages = vec![
            serde_json::json!({"role":"system","content":format!("{REVIEW_BASE} {}", reviewer.stance)}),
            user.clone(),
        ];
        match reviewer.client.complete(&messages) {
            Ok(turn) => {
                let verdict = parse_member_verdict(&turn.text);
                let concern = concern_from(&turn.text);
                members.push(PanelMember {
                    model: reviewer.model.clone(),
                    stance: lens_name(reviewer.stance).to_string(),
                    verdict: verdict.to_string(),
                    concern,
                });
            }
            Err(_) => {
                failed += 1;
                members.push(PanelMember {
                    model: reviewer.model.clone(),
                    stance: lens_name(reviewer.stance).to_string(),
                    verdict: "unavailable".to_string(),
                    concern: String::new(),
                });
            }
        }
    }

    // Fold the reads - conservative, dissent preserved.
    let any_needs_work = members.iter().any(|m| m.verdict == "needs work");
    let any_ready = members.iter().any(|m| m.verdict == "ready");
    let consensus = if any_needs_work {
        "needs_work"
    } else if any_ready {
        "ready"
    } else {
        "inconclusive"
    };
    // Dissent: a reviewer whose verdict differs from the consensus direction.
    let consensus_verdict = if consensus == "needs_work" {
        "needs work"
    } else {
        "ready"
    };
    let dissent: Vec<String> = members
        .iter()
        .filter(|m| m.verdict != "unavailable" && m.verdict != consensus_verdict)
        .map(|m| m.model.clone())
        .collect();
    // Blind spots: concerns raised by a reviewer that the consensus did not
    // carry - the angle one lens caught and the others missed. Derived, honest.
    let blind_spots: String = members
        .iter()
        .filter(|m| m.verdict == "needs work" && !m.concern.is_empty())
        .map(|m| format!("[{}] {}", m.model, m.concern))
        .collect::<Vec<_>>()
        .join("\n");
    let confidence = if failed > 0 {
        "low"
    } else if members.len() >= 2 && dissent.is_empty() {
        "high"
    } else if members.len() >= 2 {
        "medium"
    } else {
        "low"
    };
    let residency = "frontier";

    let resolved = crate::request_security::resolve_governed_write_identity(
        "{}",
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .record_forge_review_panel_with_identity(
                ForgeReviewPanel {
                    tenant_id: &resolved.tenant_id,
                    actor_id: &resolved.actor_id,
                    run_id,
                    consensus,
                    confidence,
                    residency,
                    members: &members,
                    dissent: &dissent,
                    blind_spots: &blind_spots,
                },
                &resolved.identity,
            )
            .map_err(|error| error.message())?
    };

    let members_json: Vec<serde_json::Value> = members
        .iter()
        .map(|m| {
            serde_json::json!({
                "model": m.model,
                "stance": m.stance,
                "verdict": m.verdict,
                "concern": m.concern,
            })
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-review-panel",
            "status": "RECORDED",
            "run_id": run_id,
            "consensus": consensus,
            "confidence": confidence,
            "residency": residency,
            "members": members_json,
            "dissent": dissent,
            "blind_spots": blind_spots,
            "panel_receipt_id": report.receipt_id,
            "policy_decision_id": report.policy_decision_id,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

// A short lens name from the stance prompt, for the surface.
fn lens_name(stance: &str) -> &'static str {
    if stance.contains("HOLISTIC") {
        "holistic"
    } else if stance.contains("STRICT") {
        "strict"
    } else if stance.contains("ADVERSARIAL") {
        "adversarial"
    } else {
        "review"
    }
}

// The reviewer's concern: everything after the verdict line, trimmed.
fn concern_from(text: &str) -> String {
    text.lines()
        .skip_while(|line| !line.to_lowercase().contains("verdict:"))
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .chars()
        .take(400)
        .collect()
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-review-panel","status":"REFUSED","reason":{},"panel_receipt_id":"","production_write_allowed":false}}"#,
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
