// The Forge run ship decision over HTTP. A human reviewed a run's diff and
// ratifies that it ships; the kernel gate reads the run's own receipts and
// refuses anything that did not finish green with a branch. Governed: the
// ship decision is a person's verb, recorded with their identity. This
// route opens no deployment authority - it records the ratification the
// separate deploy step reads.
use crate::RouteResponse;
use mdx_core::{ForgeRunRefusal, ForgeRunShipDecision, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/run-ship-decisions.json" => Some(handle_post(method, body, kernel)),
        "/forge/run-ship-decisions/projection.json" => Some(handle_projection(method, kernel)),
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
    let run_id = field("run_id");
    // Observe the branch tip in the actual repo BEFORE taking the write
    // lock. The kernel gate compares the ratified commit against this
    // observation, so a decision can never bind to a commit the branch
    // does not currently hold.
    let observed_branch_tip = observed_branch_tip(kernel, &run_id)?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_forge_run_ship_decision_with_identity(
        ForgeRunShipDecision {
            tenant_id: &resolved.tenant_id,
            human_actor_id: &resolved.actor_id,
            run_id: &run_id,
            commit_sha: &field("commit_sha"),
            observed_branch_tip: &observed_branch_tip,
            reason: &field("reason"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            // Every ship-decision refusal is receipted with its reason, on the
            // same governed chain as recorded ship decisions, and the receipt
            // id comes back in the response so the refusal is never receiptless.
            let reason = error.message();
            let refusal_receipt_id = kernel
                .record_forge_run_refusal_with_identity(
                    ForgeRunRefusal {
                        tenant_id: &resolved.tenant_id,
                        actor_id: &resolved.actor_id,
                        route: "/forge/run-ship-decisions.json",
                        reason: &reason,
                        repo_id: "mdx",
                        requested_run_id: &run_id,
                    },
                    &resolved.identity,
                )
                .map(|report| report.receipt_id)
                .unwrap_or_default();
            return Ok(refusal(&reason, &refusal_receipt_id));
        }
    };
    // Derive the implicit learning signal from the disposition just recorded:
    // a human read the diff and accepted it. A clean acceptance is the quieter
    // signal, but it still teaches what kind of change on what kind of work
    // ships cleanly. Fail-soft: never break the ship path.
    {
        let lesson = crate::learning_routes::implicit_signal::disposition_lesson(
            "change_accepted",
            &run_id,
            "",
        );
        crate::learning_routes::implicit_signal::derive_implicit_signal(
            &mut kernel,
            &resolved.identity,
            mdx_core::LearningImplicitSignal {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                source_receipt_id: &report.receipt_id,
                signal_kind: "change_accepted",
                work_item: &run_id,
                delta_summary: "",
                lesson_candidate: &lesson,
            },
        );
    }
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-ship-local-post","status":"RECORDED","auth_session_status":{},"run_id":{},"branch":{},"ship_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"deployment_eligible":true,"deployment_authority_granted":false,"projection_route":"/forge/run-ship-decisions/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&run_id),
            json_string_literal(&report.branch),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
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
    let decisions: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("forge.run.ship.decided")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"ship_receipt_id":{},"run_id":{},"branch":{},"commit_sha":{},"reason":{},"passed_checks":{},"deployment_eligible":true,"actor_id":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("run_id"),
                value("branch"),
                value("commit_sha"),
                value("reason"),
                value("passed_checks"),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-ship-local-projection","receipt_kind":"forge.run.ship.decided","decision_count":{},"decisions":[{}],"production_write_allowed":false}}"#,
            decisions.len(),
            decisions.join(","),
        ),
    ))
}

/// Read the run's branch and repo from its own receipts, then ask git for
/// the branch tip. Returns an empty string when no tip is observable -
/// the kernel gate refuses that, so an unverifiable ratification never
/// records. Read lock only; the git call happens with no lock held.
fn observed_branch_tip(kernel: &Arc<RwLock<MdxKernel>>, run_id: &str) -> Result<String, String> {
    let (branch, repo_root) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let mut branch = String::new();
        let mut repo_id = String::new();
        for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
            if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
                continue;
            }
            if let Some(value) = receipt.payload.get("repo_id")
                && !value.trim().is_empty()
            {
                repo_id = value.trim().to_string();
            }
            let detail = receipt
                .payload
                .get("detail")
                .map(String::as_str)
                .unwrap_or("");
            if receipt.payload.get("event").map(String::as_str) == Some("evidence_appended")
                && detail.starts_with("branch=")
            {
                branch = detail
                    .trim_start_matches("branch=")
                    .split(' ')
                    .next()
                    .unwrap_or("")
                    .to_string();
            }
        }
        let repo_root = if repo_id.is_empty() {
            None
        } else {
            kernel.forge_repo_root(&repo_id)
        };
        match repo_root {
            Some(root) => (branch, root),
            None => return Ok(String::new()),
        }
    };
    if branch.is_empty() {
        return Ok(String::new());
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        _ => Ok(String::new()),
    }
}

fn refusal(reason: &str, refusal_receipt_id: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-ship-local-post","status":"REFUSED","reason":{},"ship_receipt_id":"","refusal_receipt_id":{},"production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_string_literal(refusal_receipt_id)
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

    fn refusal_receipt_count(kernel: &Arc<RwLock<MdxKernel>>) -> usize {
        kernel
            .read()
            .expect("ledger readable")
            .ledger()
            .query()
            .by_kind("forge.run.refused")
            .len()
    }

    #[test]
    fn a_ship_refusal_is_receipted_and_returns_the_receipt_id() {
        // The kernel gate refuses a ship decision for a run that never finished
        // green with a branch. That refusal must still land a receipt with the
        // reason and hand its id back, so no ship refusal is ever receiptless.
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let before = refusal_receipt_count(&kernel);
        let response = handle_post(
            "POST",
            r#"{"run_id":"forge_run_never_ran","commit_sha":"abc123def456","reason":"looks good"}"#,
            &kernel,
        )
        .expect("handler ran");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains(r#""status":"REFUSED""#));
        assert_eq!(
            refusal_receipt_count(&kernel),
            before + 1,
            "a ship refusal must land exactly one refusal receipt"
        );
        let receipt_id = json_string_field(response.body_text(), "refusal_receipt_id")
            .expect("refusal_receipt_id present");
        assert!(
            !receipt_id.is_empty(),
            "the refusal must surface the receipt id it just recorded"
        );
        assert!(response.body_text().contains(r#""ship_receipt_id":"""#));
    }
}
