// Live source-host delivery for Forge. This is the narrow bridge from a
// reviewed Forge branch to a GitHub or Bitbucket Cloud pull request. It is fail-closed: the
// operator must confirm, the process must opt in with an env gate, Review
// Packet delivery guardrails must pass, and credentials are checked by
// presence only. No credential values are recorded.
use crate::RouteResponse;
use mdx_core::{ForgeRunEvent, MdxKernel, SelfDeliveryScope, json_string_literal};
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    // The autonomous CI convergence step (ADR 0488, SelfConvergeCi): observe CI
    // for a run's PR and, on red, spend one bounded governed fix iteration to
    // drive it green. Poll-driven: each call does one step.
    if path == "/forge/self-delivery/convergences.json" {
        if let Some(response) = crate::reject_unless_method(method, "POST") {
            return Some(Ok(response));
        }
        let run_id = json_string_field(body, "run_id").unwrap_or_default();
        if run_id.trim().is_empty() {
            return Some(Ok(refusal("name the run to converge")));
        }
        return Some(handle_self_converge(&run_id, body, kernel));
    }
    // The autonomous SelfMerge advance (ADR 0488, Tier 3): merge a run's own PR
    // into the default branch once CI is green and the self-delivery
    // authorization re-clears. Repo self-improvement, never a deploy.
    if path == "/forge/self-delivery/merges.json" {
        if let Some(response) = crate::reject_unless_method(method, "POST") {
            return Some(Ok(response));
        }
        let run_id = json_string_field(body, "run_id").unwrap_or_default();
        if run_id.trim().is_empty() {
            return Some(Ok(refusal("name the run to merge")));
        }
        return Some(handle_self_merge(&run_id, body, kernel));
    }
    if path != "/forge/source-host-live-deliveries.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Some(Ok(refusal("name the run to deliver to a source host")));
    }
    Some(handle(&run_id, body, kernel))
}

fn handle(
    run_id: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !env_enabled("MDX_FORGE_SOURCE_HOST_LIVE") {
        return Ok(refusal(
            "set MDX_FORGE_SOURCE_HOST_LIVE=1 in the server environment before live source-host delivery",
        ));
    }
    crate::forge_execution_posture::record_current_posture(kernel, "live_delivery");
    // The ship gate: live delivery requires the run's recorded human ship
    // decision, not a request-body boolean. When there is none, a run may
    // still self-deliver IF its envelope grants a self-delivery scope and its
    // diff clears the firewall and earns the fresh-eyes concurrence (ADR 0488);
    // that path is resolved below, once the branch is known. Either way the
    // ratified commit is re-verified against the branch tip at delivery time,
    // so a branch that moved after review refuses instead of shipping
    // unreviewed work.
    let ship_decision = latest_ship_decision(kernel, run_id)?;

    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, run_id)?;
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

    let repo_id = packet["repo_intelligence"]["repo_id"]
        .as_str()
        .unwrap_or("");
    let (repo_root, origin_url) = repo_target(kernel, repo_id)?;
    let requested_host =
        json_string_field(body, "target_host").unwrap_or_else(|| "github".to_string());
    let source_host =
        crate::forge_source_host_pr_draft_route::normalize_source_host(&requested_host);
    let source_repo = match source_host {
        "github" => match github_repo_slug(&origin_url) {
            Some(value) => value,
            None => {
                return Ok(refusal(
                    "connect the repo with a GitHub origin URL before live delivery",
                ));
            }
        },
        "bitbucket" => match bitbucket_repo_slug(&origin_url) {
            Some(value) => value,
            None => {
                return Ok(refusal(
                    "connect the repo with a Bitbucket Cloud origin URL before live delivery",
                ));
            }
        },
        _ => {
            return Ok(refusal(
                "live source-host delivery supports github or bitbucket",
            ));
        }
    };
    if source_host == "github" && !github_credentials_present() {
        return Ok(refusal(
            "GITHUB_TOKEN, GH_TOKEN, or gh auth login is required before live GitHub delivery",
        ));
    }
    if source_host == "bitbucket" && bitbucket_auth_config().is_none() {
        return Ok(refusal(
            "BITBUCKET_TOKEN or BITBUCKET_USERNAME plus BITBUCKET_APP_PASSWORD is required before live Bitbucket delivery",
        ));
    }

    let title = packet["pr_handoff"]["title"].as_str().unwrap_or("");
    let body_markdown = packet["pr_handoff"]["body_markdown"].as_str().unwrap_or("");
    let source_branch = packet["branch"].as_str().unwrap_or("");
    let base_branch = json_string_field(body, "base_branch")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());
    if title.trim().is_empty() || body_markdown.trim().is_empty() {
        return Ok(refusal("review packet did not include a PR title and body"));
    }
    if source_branch.trim().is_empty() {
        return Ok(refusal("review packet did not include a source branch"));
    }

    let review_status = packet["review_status"].as_str().unwrap_or("");
    let delivery_guardrails =
        crate::forge_source_host_pr_draft_route::delivery_guardrails(&packet, review_status);
    let blockers =
        crate::forge_source_host_pr_draft_route::delivery_guardrail_blockers(&delivery_guardrails);
    if !blockers.is_empty() {
        let reason = format!(
            "review guardrails block live delivery: {}",
            blockers.join(",")
        );
        let escalation_receipt_id = record_self_delivery_event(
            kernel,
            run_id,
            "self.delivery.escalated",
            &blockers.join(","),
            "",
            "",
        );
        return Ok(refusal_with_receipt(&reason, &escalation_receipt_id));
    }
    if !branch_exists(Path::new(&repo_root), source_branch) {
        return Ok(refusal(
            "source branch does not exist in the connected repo",
        ));
    }
    let current_tip = branch_tip(Path::new(&repo_root), source_branch);

    // Resolve authority: a human ship decision, or governed self-delivery.
    // Self-delivery ratifies the tip its concurrence just approved, so its
    // ratified commit IS the current tip (the tip re-verification below still
    // runs and would catch any move between here and the push).
    // When Forge self-delivers, the PR must SAY so - unambiguously and
    // auditably - so no one mistakes an autonomously delivered change for a
    // human-reviewed one. Empty for a human ship decision.
    let mut self_delivery_marker = String::new();
    let ratified_owned: String = match &ship_decision {
        Some(ship) => ship.commit_sha.clone(),
        None => {
            let authorization = crate::forge_self_delivery::authorize_self_delivery(
                kernel,
                "local_tenant",
                run_id,
                title,
                &base_branch,
                mdx_core::SelfDeliveryScope::SelfOpenPr,
            );
            match authorization.denied_reason {
                Some(reason) => {
                    // No human decision and self-delivery not authorized:
                    // escalate to a human with the exact reason and refuse.
                    let escalation_receipt_id = record_self_delivery_event(
                        kernel,
                        run_id,
                        "self.delivery.escalated",
                        &reason,
                        &authorization.envelope_id,
                        authorization.scope.as_str(),
                    );
                    return Ok(refusal_with_receipt(
                        &format!(
                            "no human ship decision, and self-delivery is not authorized: {reason}. Record a human ship decision at /forge/run-ship-decisions.json, or grant a self-delivery envelope."
                        ),
                        &escalation_receipt_id,
                    ));
                }
                None => {
                    // Authorized: the run opens its own PR under its envelope.
                    record_self_delivery_event(
                        kernel,
                        run_id,
                        "self.delivery.pr_opened",
                        "self_delivery_authorized_open_pr",
                        &authorization.envelope_id,
                        authorization.scope.as_str(),
                    );
                    self_delivery_marker = autonomous_delivery_marker(
                        run_id,
                        &authorization.envelope_id,
                        authorization.scope.as_str(),
                        &current_tip,
                    );
                    current_tip.clone()
                }
            }
        }
    };
    // Prepend the marker to the body and flag the title so the autonomy is
    // visible at a glance and in the PR record.
    let pr_title_owned = if self_delivery_marker.is_empty() {
        title.to_string()
    } else {
        format!("[Forge autonomous] {title}")
    };
    let pr_body_owned = if self_delivery_marker.is_empty() {
        body_markdown.to_string()
    } else {
        format!("{self_delivery_marker}\n\n{body_markdown}")
    };
    let title = pr_title_owned.as_str();
    let body_markdown = pr_body_owned.as_str();
    let ratified = ratified_owned.as_str();
    let tip_matches_ratified = !current_tip.is_empty()
        && (current_tip == ratified || (ratified.len() >= 12 && current_tip.starts_with(ratified)));
    if !tip_matches_ratified {
        record_delivery_event(
            kernel,
            run_id,
            "LIVE_DELIVERY_FAILED",
            "ship_commit_mismatch",
            source_host,
            source_branch,
            &base_branch,
            "",
        );
        return Ok(refusal(
            "the human ship decision no longer matches the branch tip - the branch moved after review; re-review the diff and record a fresh ship decision",
        ));
    }

    let push = git_push(Path::new(&repo_root), source_branch);
    if !push.success {
        record_delivery_event(
            kernel,
            run_id,
            "LIVE_DELIVERY_FAILED",
            "git_push_failed",
            source_host,
            source_branch,
            &base_branch,
            "",
        );
        return Ok(failure("git_push_failed", &push.stderr));
    }

    let pr = match source_host {
        "github" => github_pr_create(
            &source_repo,
            source_branch,
            &base_branch,
            title,
            body_markdown,
        ),
        "bitbucket" => bitbucket_pr_create(
            &source_repo,
            source_branch,
            &base_branch,
            title,
            body_markdown,
        ),
        _ => CommandReport {
            success: false,
            stdout: String::new(),
            stderr: "unsupported source host".to_string(),
        },
    };
    if !pr.success {
        let reason = format!("{source_host}_pr_create_failed");
        record_delivery_event(
            kernel,
            run_id,
            "LIVE_DELIVERY_FAILED",
            &reason,
            source_host,
            source_branch,
            &base_branch,
            "",
        );
        return Ok(failure(&reason, &pr.stderr));
    }
    let pr_url = match source_host {
        "bitbucket" => pr_url_from_bitbucket_response(&pr.stdout)
            .or_else(|| pr_url_from_output(&pr.stdout))
            .unwrap_or_default(),
        _ => pr_url_from_output(&pr.stdout).unwrap_or_default(),
    };
    record_delivery_event(
        kernel,
        run_id,
        "LIVE_PR_OPENED",
        &format!("{source_host}_pr_opened"),
        source_host,
        source_branch,
        &base_branch,
        &pr_url,
    );

    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-source-host-live-delivery",
            "status": "LIVE_PR_OPENED",
            "run_id": run_id,
            "source_host": source_host,
            "source_repo": source_repo,
            "source_branch": source_branch,
            "target_base_branch": base_branch.trim(),
            "pull_request_url": pr_url,
            "delivery_guardrails": delivery_guardrails,
            "delivery_blocked_reasons": [],
            "dry_run": false,
            "credential_values_recorded": false,
            "network_call_allowed": true,
            "remote_push_allowed": true,
            "pull_request_open_allowed": true,
            "approval_allowed": false,
            "deployment_authority_granted": false,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

fn repo_target(kernel: &Arc<RwLock<MdxKernel>>, repo_id: &str) -> Result<(String, String), String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let Some(root) = kernel.forge_repo_root(repo_id) else {
        return Err("connect the repo before live source-host delivery".to_string());
    };
    let origin = kernel.forge_repo_origin(repo_id).unwrap_or_default();
    Ok((root, origin))
}

// The autonomous merge (ADR 0488, Tier SelfMerge). Every gate must hold, in
// fail-closed order: the source-host live gate is on; the run's envelope grants
// SelfMerge and is still active; CI is OBSERVED green and bound to the exact
// branch tip; the self-delivery authorization re-clears (firewall + fresh-eyes
// concurrence, re-run against the current diff); and the tip has not moved since
// authorization. Only then does Forge merge its own PR into the default branch -
// and even then it is repo self-improvement, never a production deploy. Any gate
// open escalates to a human with the reason and merges nothing.
fn handle_self_merge(
    run_id: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !env_enabled("MDX_FORGE_SOURCE_HOST_LIVE") {
        return Ok(refusal(
            "set MDX_FORGE_SOURCE_HOST_LIVE=1 in the server environment before autonomous merge",
        ));
    }
    crate::forge_execution_posture::record_current_posture(kernel, "live_delivery");

    let (scope, envelope_id) = crate::forge_self_delivery::run_self_delivery_scope(kernel, run_id);
    let escalate = |kernel: &Arc<RwLock<MdxKernel>>, reason: &str, human: &str| {
        let escalation_receipt_id = record_self_delivery_event(
            kernel,
            run_id,
            "self.delivery.escalated",
            reason,
            &envelope_id,
            scope.as_str(),
        );
        Ok(refusal_with_receipt(human, &escalation_receipt_id))
    };
    if scope < SelfDeliveryScope::SelfMerge {
        return escalate(
            kernel,
            "self_merge_scope_required",
            "autonomous merge requires an active self_merge envelope bound to this run",
        );
    }

    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, run_id)?;
    let packet: serde_json::Value = match serde_json::from_str(&packet_body) {
        Ok(value) => value,
        Err(_) => return Ok(refusal("review packet did not return valid JSON")),
    };
    if packet["status"].as_str() != Some("OK") {
        return escalate(
            kernel,
            "review_packet_unavailable",
            packet["reason"]
                .as_str()
                .unwrap_or("review packet is not available for this run"),
        );
    }
    let repo_id = packet["repo_intelligence"]["repo_id"]
        .as_str()
        .unwrap_or("");
    let (repo_root, origin_url) = repo_target(kernel, repo_id)?;
    let source_branch = packet["branch"].as_str().unwrap_or("");
    let base_branch = json_string_field(body, "base_branch")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());
    let title = packet["pr_handoff"]["title"].as_str().unwrap_or("");
    if source_branch.trim().is_empty() {
        return Ok(refusal("review packet did not include a source branch"));
    }
    if !branch_exists(Path::new(&repo_root), source_branch) {
        return Ok(refusal(
            "source branch does not exist in the connected repo",
        ));
    }
    let current_tip = branch_tip(Path::new(&repo_root), source_branch);

    // CI must be OBSERVED green for this exact commit and branch. The adapter
    // returns None unless a completed run is bound to this tip; a non-success
    // conclusion escalates. No fabricated green can pass here.
    let workflow_run_id = json_string_field(body, "workflow_run_id").unwrap_or_default();
    let repo_slug = github_repo_slug(&origin_url).unwrap_or_default();
    let observed = crate::harness_ci_evidence_adapter::ServerCiEvidenceAdapter.observe(
        &repo_slug,
        &workflow_run_id,
        &current_tip,
        source_branch,
    );
    let Some(observed) = observed else {
        return escalate(
            kernel,
            "ci_not_observed_green_for_commit",
            "CI evidence is not observable and bound to this exact commit - set MDX_CI_EVIDENCE_OBSERVE=1 and wait for the workflow to complete",
        );
    };
    if observed.conclusion != "success" {
        return escalate(
            kernel,
            &format!("ci_conclusion:{}", observed.conclusion),
            "CI did not conclude green for this commit - autonomous merge requires observed green CI",
        );
    }

    // Re-authorize at SelfMerge: scope still active, the diff still clears the
    // firewall, and the fresh-eyes concurrence is re-earned against the current
    // diff. This runs AFTER the CI observation so a diff that changed under a
    // stale grant cannot slip through.
    let authorization = crate::forge_self_delivery::authorize_self_delivery(
        kernel,
        "local_tenant",
        run_id,
        title,
        &base_branch,
        SelfDeliveryScope::SelfMerge,
    );
    if let Some(reason) = authorization.denied_reason {
        return escalate(
            kernel,
            &reason,
            &format!("autonomous merge is not authorized: {reason}"),
        );
    }

    // The merge itself, with a final tip-stability check so a branch that moved
    // between authorization and merge aborts instead of merging unreviewed work.
    let merge = merge_branch(
        Path::new(&repo_root),
        &base_branch,
        source_branch,
        &current_tip,
    );
    if !merge.success {
        return escalate(
            kernel,
            &format!("merge_failed:{}", merge.stderr.trim()),
            &format!("autonomous merge did not complete: {}", merge.stderr.trim()),
        );
    }

    record_self_delivery_event(
        kernel,
        run_id,
        "self.delivery.merged",
        &format!(
            "merged {source_branch} into {base_branch} at {current_tip} on green CI ({})",
            observed.evidence_url
        ),
        &envelope_id,
        scope.as_str(),
    );
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            "{{\"name\":\"mdx-forge-self-delivery-merge\",\"status\":\"SELF_MERGED\",\"run_id\":{},\"merged_branch\":{},\"base_branch\":{},\"commit\":{},\"ci_conclusion\":\"success\",\"ci_evidence_url\":{},\"self_delivery_scope\":\"{}\",\"envelope_id\":{},\"deployment_authority_granted\":false,\"production_write_allowed\":false}}",
            json_string_literal(run_id),
            json_string_literal(source_branch),
            json_string_literal(&base_branch),
            json_string_literal(&current_tip),
            json_string_literal(&observed.evidence_url),
            scope.as_str(),
            json_string_literal(&envelope_id),
        ),
    ))
}

// One poll-driven CI convergence step (ADR 0488, SelfConvergeCi). Observe CI
// for the run's PR: green means converged; red spends ONE bounded governed fix
// iteration - a fix run revising the branch under the same envelope, its writes
// bounded to the envelope's path scopes - and reports "converging". The
// iteration ceiling is the anti-runaway backstop: once it is spent, convergence
// escalates to a human rather than looping. A fix that touches a protected path
// is caught at the merge gate and never lands.
fn handle_self_converge(
    run_id: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !env_enabled("MDX_FORGE_SOURCE_HOST_LIVE") {
        return Ok(refusal(
            "set MDX_FORGE_SOURCE_HOST_LIVE=1 in the server environment before autonomous convergence",
        ));
    }
    crate::forge_execution_posture::record_current_posture(kernel, "live_delivery");

    let (scope, envelope_id) = crate::forge_self_delivery::run_self_delivery_scope(kernel, run_id);
    let escalate = |kernel: &Arc<RwLock<MdxKernel>>, reason: &str, human: &str| {
        let escalation_receipt_id = record_self_delivery_event(
            kernel,
            run_id,
            "self.delivery.escalated",
            reason,
            &envelope_id,
            scope.as_str(),
        );
        Ok(refusal_with_receipt(human, &escalation_receipt_id))
    };
    if scope < SelfDeliveryScope::SelfConvergeCi {
        return escalate(
            kernel,
            "self_converge_scope_required",
            "autonomous CI convergence requires an active self_converge_ci (or stronger) envelope bound to this run",
        );
    }

    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, run_id)?;
    let packet: serde_json::Value = match serde_json::from_str(&packet_body) {
        Ok(value) => value,
        Err(_) => return Ok(refusal("review packet did not return valid JSON")),
    };
    if packet["status"].as_str() != Some("OK") {
        return escalate(
            kernel,
            "review_packet_unavailable",
            packet["reason"]
                .as_str()
                .unwrap_or("review packet is not available for this run"),
        );
    }
    let repo_id = packet["repo_intelligence"]["repo_id"]
        .as_str()
        .unwrap_or("");
    let (repo_root, origin_url) = repo_target(kernel, repo_id)?;
    let source_branch = packet["branch"].as_str().unwrap_or("");
    if source_branch.trim().is_empty() {
        return Ok(refusal("review packet did not include a source branch"));
    }
    if !branch_exists(Path::new(&repo_root), source_branch) {
        return Ok(refusal(
            "source branch does not exist in the connected repo",
        ));
    }
    let current_tip = branch_tip(Path::new(&repo_root), source_branch);
    let repo_slug = github_repo_slug(&origin_url).unwrap_or_default();
    let workflow_run_id = json_string_field(body, "workflow_run_id").unwrap_or_default();

    let observed = crate::harness_ci_evidence_adapter::ServerCiEvidenceAdapter.observe(
        &repo_slug,
        &workflow_run_id,
        &current_tip,
        source_branch,
    );
    let Some(observed) = observed else {
        return escalate(
            kernel,
            "ci_not_observed_for_commit",
            "CI evidence is not observable and bound to this exact commit - set MDX_CI_EVIDENCE_OBSERVE=1 and wait for the workflow to complete",
        );
    };

    if observed.conclusion == "success" {
        // Converged. The merge is a separate governed step; convergence just
        // witnesses that CI is green for this commit.
        record_self_delivery_event(
            kernel,
            run_id,
            "self.delivery.ci_converged",
            &format!("ci green at {current_tip} ({})", observed.evidence_url),
            &envelope_id,
            scope.as_str(),
        );
        return Ok(RouteResponse::json(
            "200 OK",
            format!(
                "{{\"name\":\"mdx-forge-self-delivery-converge\",\"status\":\"CONVERGED\",\"run_id\":{},\"commit\":{},\"ci_conclusion\":\"success\",\"self_delivery_scope\":\"{}\",\"deployment_authority_granted\":false}}",
                json_string_literal(run_id),
                json_string_literal(&current_tip),
                scope.as_str(),
            ),
        ));
    }

    // Red CI: spend one bounded fix iteration, or escalate if exhausted.
    let spent = crate::forge_self_delivery::ci_fix_iteration_count(kernel, run_id);
    let ceiling = crate::forge_self_delivery::max_ci_fix_iterations();
    if spent >= ceiling {
        return escalate(
            kernel,
            &format!("ci_fix_iterations_exhausted:{spent}/{ceiling}"),
            "CI is still red after the bounded fix iterations - escalating to a human rather than looping",
        );
    }

    let write_scope =
        crate::forge_self_delivery::envelope_allowed_path_scopes(kernel, &envelope_id);
    let fix_intent = format!(
        "CI concluded '{}' for this change ({}). Make the smallest fix to get CI green. Do not widen scope or touch anything unrelated to the failure.",
        observed.conclusion, observed.evidence_url
    );
    let allowed_commands = packet["repo_intelligence"]["selected_checks"]
        .as_array()
        .map(|checks| {
            checks
                .iter()
                .filter_map(|check| check.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let fix_run_id = match crate::forge_run_route::spawn_self_delivery_fix_run(
        kernel,
        run_id,
        &envelope_id,
        &fix_intent,
        std::path::PathBuf::from(&repo_root),
        source_branch,
        write_scope,
        allowed_commands,
        spent + 1,
    ) {
        Ok(id) => id,
        Err(error) => {
            return escalate(
                kernel,
                &format!("fix_run_spawn_failed:{error}"),
                &format!("could not start a CI fix run: {error}"),
            );
        }
    };

    record_self_delivery_event(
        kernel,
        run_id,
        "self.delivery.ci_fix_started",
        &format!(
            "iteration {} of {ceiling}; fix run {fix_run_id} on {source_branch}",
            spent + 1
        ),
        &envelope_id,
        scope.as_str(),
    );
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            "{{\"name\":\"mdx-forge-self-delivery-converge\",\"status\":\"CONVERGING\",\"run_id\":{},\"ci_conclusion\":{},\"fix_run_id\":{},\"iteration\":{},\"max_iterations\":{},\"self_delivery_scope\":\"{}\",\"deployment_authority_granted\":false}}",
            json_string_literal(run_id),
            json_string_literal(&observed.conclusion),
            json_string_literal(&fix_run_id),
            spent + 1,
            ceiling,
            scope.as_str(),
        ),
    ))
}

// Merge a source branch into the base and push, with a final tip-stability
// guard. Fails closed on any git error and aborts a partial merge so the
// working tree is never left mid-conflict.
fn merge_branch(
    repo_root: &Path,
    base_branch: &str,
    source_branch: &str,
    expected_tip: &str,
) -> CommandReport {
    if branch_tip(repo_root, source_branch) != expected_tip {
        return CommandReport {
            success: false,
            stdout: String::new(),
            stderr: "source branch tip moved after authorization; merge aborted".to_string(),
        };
    }
    let checkout = git(repo_root, &["checkout", base_branch]);
    if !checkout.success {
        return checkout;
    }
    let message = format!("Forge self-merge of {source_branch} (ADR 0488, repo self-improvement)");
    let merge = git(
        repo_root,
        &["merge", "--no-ff", "-m", &message, source_branch],
    );
    if !merge.success {
        let _ = git(repo_root, &["merge", "--abort"]);
        return merge;
    }
    git(repo_root, &["push", "origin", base_branch])
}

// The marker stamped into a self-delivered PR's body so the autonomy is
// unambiguous and auditable: it names the governing (revocable) envelope, the
// scope, the gates that had to clear, and states plainly that no production
// deploy authority was granted. A human ship decision produces no marker.
fn autonomous_delivery_marker(
    run_id: &str,
    envelope_id: &str,
    scope: &str,
    commit: &str,
) -> String {
    let concurrences = crate::forge_self_delivery::required_concurrences();
    format!(
        "> \u{1F916} **Autonomously delivered by Forge** - no human ship decision (ADR 0488).\n\
>\n\
> - Self-delivery scope: `{scope}`\n\
> - Governing envelope: `{envelope_id}` (human-owned, revocable - revoke to stop)\n\
> - Gates cleared: self-modification firewall clear; {concurrences} independent fresh-eyes concurrence(s)\n\
> - Run: `{run_id}` - Commit: `{commit}`\n\
> - `deployment_authority_granted: false` - repo self-improvement, never a production deploy"
    )
}

// Witness a self-delivery decision on the ledger (ADR 0488): the authorization
// to open a PR without a human ship decision, or the escalation when it was
// refused. Bound to the run and its envelope so the chain is auditable, and
// always stamped with deployment_authority_granted=false - self-delivery is
// repo self-improvement, never a deploy. Returns the receipt id so a refusal
// can surface the escalation receipt it just recorded; empty only if the write
// itself could not land.
//
// production_write_allowed is NOT passed as an evidence field here: the event
// recorder reserves that key and stamps it false on every receipt itself, so
// passing it again is rejected as an invalid evidence field and would swallow
// the whole write.
fn record_self_delivery_event(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    event_kind: &str,
    reason: &str,
    envelope_id: &str,
    scope: &str,
) -> String {
    let Ok(mut kernel) = kernel.write() else {
        return String::new();
    };
    match kernel.record_forge_run_event_with_evidence_fields(
        ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge_self_delivery",
            run_id,
            event: "evidence_appended",
            work_item_id: "",
            detail: &format!("{event_kind} reason={reason} envelope={envelope_id}"),
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        },
        &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_self_delivery"),
        &[
            ("self_delivery_event", event_kind),
            ("self_delivery_reason", reason),
            ("self_delivery_envelope_id", envelope_id),
            ("self_delivery_scope", scope),
            ("human_ship_decision", "false"),
            ("deployment_authority_granted", "false"),
        ],
    ) {
        Ok(report) => report.receipt_id,
        Err(_) => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn record_delivery_event(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    status: &str,
    reason: &str,
    source_host: &str,
    source_branch: &str,
    base_branch: &str,
    pull_request_url: &str,
) {
    if let Ok(mut kernel) = kernel.write() {
        let _ = kernel.record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_source_host_delivery",
                run_id,
                event: "evidence_appended",
                work_item_id: "",
                detail: &format!("status={status} reason={reason} host={source_host}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_source_host_delivery"),
            &[
                ("delivery_status", status),
                ("delivery_reason", reason),
                ("source_host", source_host),
                ("source_branch", source_branch),
                ("target_base_branch", base_branch),
                ("pull_request_url", pull_request_url),
                ("credential_values_recorded", "false"),
                ("approval_allowed", "false"),
                ("deployment_authority_granted", "false"),
            ],
        );
    }
}

fn branch_exists(repo_root: &Path, branch: &str) -> bool {
    git(repo_root, &["rev-parse", "--verify", branch]).success
}

fn branch_tip(repo_root: &Path, branch: &str) -> String {
    let report = git(
        repo_root,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    );
    if report.success {
        report.stdout.trim().to_string()
    } else {
        String::new()
    }
}

/// The most recent human ship decision recorded for this run, straight
/// from the ledger. None means no human has ratified this run's diff.
struct ShipDecision {
    commit_sha: String,
}

fn latest_ship_decision(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
) -> Result<Option<ShipDecision>, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let decision = kernel
        .ledger()
        .query()
        .by_kind("forge.run.ship.decided")
        .iter()
        .rfind(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))
        .map(|receipt| {
            // Prefer the full tip the kernel observed at ratification;
            // fall back to the ratified sha for older receipts.
            let field = |key: &str| {
                receipt
                    .payload
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            let observed = field("observed_branch_tip");
            ShipDecision {
                commit_sha: if observed.is_empty() {
                    field("commit_sha")
                } else {
                    observed
                },
            }
        });
    Ok(decision)
}

fn git_push(repo_root: &Path, branch: &str) -> CommandReport {
    git(repo_root, &["push", "-u", "origin", branch])
}

fn git(repo_root: &Path, args: &[&str]) -> CommandReport {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output();
    command_report(output)
}

fn github_pr_create(
    github_repo: &str,
    source_branch: &str,
    base_branch: &str,
    title: &str,
    body_markdown: &str,
) -> CommandReport {
    let body_path = std::env::temp_dir().join(format!(
        "mdx-forge-pr-body-{}-{}.md",
        std::process::id(),
        sanitize(source_branch)
    ));
    if let Err(error) = std::fs::write(&body_path, body_markdown) {
        return CommandReport {
            success: false,
            stdout: String::new(),
            stderr: format!("could not write PR body: {error}"),
        };
    }
    let output = std::process::Command::new("gh")
        .args([
            "pr",
            "create",
            "--repo",
            github_repo,
            "--base",
            base_branch,
            "--head",
            source_branch,
            "--title",
            title,
            "--body-file",
        ])
        .arg(&body_path)
        .output();
    let _ = std::fs::remove_file(&body_path);
    command_report(output)
}

fn bitbucket_pr_create(
    bitbucket_repo: &str,
    source_branch: &str,
    base_branch: &str,
    title: &str,
    body_markdown: &str,
) -> CommandReport {
    let Some(auth) = bitbucket_auth_config() else {
        return CommandReport {
            success: false,
            stdout: String::new(),
            stderr: "missing Bitbucket credentials".to_string(),
        };
    };
    let body_path = std::env::temp_dir().join(format!(
        "mdx-forge-bitbucket-pr-body-{}-{}.json",
        std::process::id(),
        sanitize(source_branch)
    ));
    let payload = serde_json::json!({
        "title": title,
        "description": body_markdown,
        "source": {
            "branch": { "name": source_branch }
        },
        "destination": {
            "branch": { "name": base_branch }
        }
    })
    .to_string();
    if let Err(error) = std::fs::write(&body_path, payload) {
        return CommandReport {
            success: false,
            stdout: String::new(),
            stderr: format!("could not write Bitbucket PR payload: {error}"),
        };
    }
    let url = format!("https://api.bitbucket.org/2.0/repositories/{bitbucket_repo}/pullrequests");
    let mut config = format!(
        "url = \"{}\"\nrequest = \"POST\"\nheader = \"Content-Type: application/json\"\ndata-binary = \"@{}\"\n",
        curl_config_escape(&url),
        curl_config_escape(body_path.to_string_lossy().as_ref()),
    );
    match auth {
        BitbucketAuth::Bearer(token) => {
            config.push_str(&format!(
                "header = \"Authorization: Bearer {}\"\n",
                curl_config_escape(&token)
            ));
        }
        BitbucketAuth::Basic { username, password } => {
            config.push_str(&format!(
                "user = \"{}:{}\"\n",
                curl_config_escape(&username),
                curl_config_escape(&password)
            ));
        }
    }
    let output = run_curl_config(&config);
    let _ = std::fs::remove_file(&body_path);
    output
}

#[derive(Debug, PartialEq, Eq)]
struct CommandReport {
    success: bool,
    stdout: String,
    stderr: String,
}

fn command_report(output: Result<std::process::Output, std::io::Error>) -> CommandReport {
    match output {
        Ok(output) => CommandReport {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: cap_chars(&String::from_utf8_lossy(&output.stderr), 1200),
        },
        Err(error) => CommandReport {
            success: false,
            stdout: String::new(),
            stderr: error.to_string(),
        },
    }
}

fn github_credentials_present() -> bool {
    std::env::var_os("GITHUB_TOKEN").is_some_and(|value| !value.is_empty())
        || std::env::var_os("GH_TOKEN").is_some_and(|value| !value.is_empty())
        || std::process::Command::new("gh")
            .args(["auth", "status"])
            .output()
            .is_ok_and(|output| output.status.success())
}

#[derive(Debug, PartialEq, Eq)]
enum BitbucketAuth {
    Bearer(String),
    Basic { username: String, password: String },
}

fn bitbucket_auth_config() -> Option<BitbucketAuth> {
    if let Some(token) = nonempty_env("BITBUCKET_TOKEN") {
        return Some(BitbucketAuth::Bearer(token));
    }
    let username = nonempty_env("BITBUCKET_USERNAME")?;
    let password = nonempty_env("BITBUCKET_APP_PASSWORD")?;
    Some(BitbucketAuth::Basic { username, password })
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn run_curl_config(config: &str) -> CommandReport {
    let mut child = match std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail-with-body",
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return CommandReport {
                success: false,
                stdout: String::new(),
                stderr: error.to_string(),
            };
        }
    };
    if let Some(mut stdin) = child.stdin.take()
        && let Err(error) = stdin.write_all(config.as_bytes())
    {
        return CommandReport {
            success: false,
            stdout: String::new(),
            stderr: error.to_string(),
        };
    }
    command_report(child.wait_with_output())
}

fn curl_config_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn github_repo_slug(origin_url: &str) -> Option<String> {
    let mut value = origin_url.trim().trim_end_matches(".git").to_string();
    if let Some(rest) = value.strip_prefix("git@github.com:") {
        value = rest.to_string();
    } else if let Some(rest) = value.strip_prefix("https://github.com/") {
        value = rest.to_string();
    } else if let Some(rest) = value.strip_prefix("http://github.com/") {
        value = rest.to_string();
    } else {
        let rest = value.strip_prefix("ssh://git@github.com/")?;
        value = rest.to_string();
    }
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    Some(format!("{}/{}", parts[0], parts[1]))
}

fn bitbucket_repo_slug(origin_url: &str) -> Option<String> {
    let mut value = origin_url.trim().trim_end_matches(".git").to_string();
    if let Some(rest) = value.strip_prefix("git@bitbucket.org:") {
        value = rest.to_string();
    } else if let Some(rest) = value.strip_prefix("https://bitbucket.org/") {
        value = rest.to_string();
    } else if let Some(rest) = value.strip_prefix("http://bitbucket.org/") {
        value = rest.to_string();
    } else {
        let rest = value.strip_prefix("ssh://git@bitbucket.org/")?;
        value = rest.to_string();
    }
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    Some(format!("{}/{}", parts[0], parts[1]))
}

fn pr_url_from_output(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|token| token.starts_with("https://"))
        .map(str::to_string)
}

fn pr_url_from_bitbucket_response(output: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    value["links"]["html"]["href"].as_str().map(str::to_string)
}

fn failure(reason: &str, detail: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-source-host-live-delivery",
            "status": "LIVE_DELIVERY_FAILED",
            "reason": reason,
            "detail": cap_chars(detail, 1200),
            "delivery_receipt_id": "",
            "policy_decision_id": "",
            "credential_values_recorded": false,
            "network_call_allowed": true,
            "remote_push_allowed": true,
            "pull_request_open_allowed": true,
            "approval_allowed": false,
            "deployment_authority_granted": false,
            "production_write_allowed": false,
        })
        .to_string(),
    )
}

fn refusal(reason: &str) -> RouteResponse {
    refusal_with_receipt(reason, "")
}

// A refusal that carries the receipt id of the escalation/refusal event it was
// paired with, so every refused delivery decision is answered with the receipt
// that proves it landed on the ledger. delivery_receipt_id is the field the
// response schema already carries; an empty id means no receipt was recorded.
fn refusal_with_receipt(reason: &str, delivery_receipt_id: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-source-host-live-delivery","status":"REFUSED","reason":{},"delivery_receipt_id":{},"policy_decision_id":"","credential_values_recorded":false,"network_call_allowed":false,"remote_push_allowed":false,"pull_request_open_allowed":false,"approval_allowed":false,"deployment_authority_granted":false,"production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_string_literal(delivery_receipt_id)
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

fn env_enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn cap_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_aborts_when_the_branch_tip_moved_after_authorization() {
        // The final tip-stability guard: if the source tip no longer matches
        // the commit the authorization approved, the merge aborts and touches
        // nothing. A non-existent repo yields an empty tip, which never equals
        // the expected sha, so the guard fires before any git checkout.
        let report = merge_branch(
            Path::new("/nonexistent/mdx/self-merge/repo"),
            "main",
            "forge/run-branch",
            "abc123def456",
        );
        assert!(!report.success);
        assert!(
            report.stderr.contains("moved after authorization"),
            "a moved tip must abort the merge: {}",
            report.stderr
        );
    }

    #[test]
    fn autonomous_marker_names_the_envelope_scope_and_never_grants_deploy() {
        let marker =
            autonomous_delivery_marker("forge_run_42", "env_docs", "self_merge", "abc123def456");
        assert!(marker.contains("Autonomously delivered by Forge"));
        assert!(marker.contains("`self_merge`"));
        assert!(marker.contains("`env_docs`"));
        assert!(marker.contains("revoke to stop"));
        assert!(marker.contains("fresh-eyes concurrence"));
        assert!(marker.contains("deployment_authority_granted: false"));
        assert!(marker.contains("never a production deploy"));
    }

    #[test]
    fn self_converge_route_names_the_run_and_stays_smoke_safe() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/self-delivery/convergences.json",
            "{}",
            &kernel,
        )
        .expect("route matched")
        .expect("handler ran");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains("name the run to converge"));
    }

    #[test]
    fn self_merge_route_names_the_run_and_stays_smoke_safe() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        // The empty-body smoke path returns a 200 refusal, never a merge.
        let response = route_response("POST", "/forge/self-delivery/merges.json", "{}", &kernel)
            .expect("route matched")
            .expect("handler ran");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains("name the run to merge"));
    }

    #[test]
    fn refuses_without_the_live_gate_and_names_it() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        // The env gate is process-wide and off in tests, so the first
        // refusal is the live gate. The human ship-decision gate has its
        // own test below.
        let response = handle(
            "forge_run_missing",
            r#"{"run_id":"forge_run_missing","target_host":"github"}"#,
            &kernel,
        )
        .expect("response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("MDX_FORGE_SOURCE_HOST_LIVE=1"));
        assert!(response.body.contains(r#""network_call_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""pull_request_open_allowed":false"#)
        );
    }

    #[test]
    fn a_run_without_a_human_ship_decision_has_none_recorded() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let decision =
            latest_ship_decision(&kernel, "forge_run_unratified").expect("ledger readable");
        assert!(
            decision.is_none(),
            "no ship decision may exist for an unratified run"
        );
    }

    #[test]
    fn the_latest_ship_decision_carries_the_ratified_commit() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            for (event, detail) in [
                ("run_started", "accepted"),
                ("check_passed", "run_command cargo build exit=0"),
                (
                    "evidence_appended",
                    "branch=forge/run-r_delivery sha=tip123",
                ),
                ("run_finished", "status=RUN_FINISHED_DONE"),
            ] {
                kernel
                    .record_forge_run_event(ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge",
                        run_id: "r_delivery",
                        event,
                        work_item_id: "w1",
                        detail,
                        turn: 1,
                        input_tokens: 0,
                        output_tokens: 0,
                    })
                    .expect("event records");
            }
            kernel
                .record_forge_run_ship_decision_with_identity(
                    mdx_core::ForgeRunShipDecision {
                        tenant_id: "local_tenant",
                        human_actor_id: "human:dev",
                        run_id: "r_delivery",
                        commit_sha: "tip123",
                        observed_branch_tip: "tip123",
                        reason: "reviewed the diff",
                    },
                    &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                )
                .expect("ship decision records");
        }
        let decision = latest_ship_decision(&kernel, "r_delivery")
            .expect("ledger readable")
            .expect("decision present");
        assert_eq!(decision.commit_sha, "tip123");
    }

    fn self_delivery_event_count(kernel: &Arc<RwLock<MdxKernel>>, run_id: &str) -> usize {
        kernel
            .read()
            .expect("ledger readable")
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .iter()
            .filter(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))
            .filter(|receipt| receipt.payload.contains_key("self_delivery_event"))
            .count()
    }

    #[test]
    fn a_self_delivery_escalation_lands_a_receipt_and_returns_its_id() {
        // Regression: the escalation event passed production_write_allowed as an
        // evidence field, which the recorder reserves, so the whole write was
        // rejected and the escalation left no receipt. It must land and hand
        // back the receipt id it recorded.
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let before = self_delivery_event_count(&kernel, "forge_run_escalated");
        let receipt_id = record_self_delivery_event(
            &kernel,
            "forge_run_escalated",
            "self.delivery.escalated",
            "self_merge_scope_required",
            "env_docs",
            "self_open_pr",
        );
        assert!(
            !receipt_id.is_empty(),
            "the escalation receipt must land and return its id"
        );
        let after = self_delivery_event_count(&kernel, "forge_run_escalated");
        assert_eq!(
            after,
            before + 1,
            "exactly one self-delivery receipt must land on the ledger"
        );
    }

    #[test]
    fn a_refusal_surfaces_the_escalation_receipt_id() {
        let response = refusal_with_receipt("escalated to a human", "receipt_abc123");
        assert_eq!(response.status_for_test(), "200 OK");
        assert!(response.body_text().contains(r#""status":"REFUSED""#));
        assert!(
            response
                .body_text()
                .contains(r#""delivery_receipt_id":"receipt_abc123""#),
            "the refusal must carry the escalation receipt id: {}",
            response.body_text()
        );
    }

    #[test]
    fn github_origin_parsing_is_presence_only() {
        assert_eq!(
            github_repo_slug("git@github.com:acme/service.git").as_deref(),
            Some("acme/service")
        );
        assert_eq!(
            github_repo_slug("https://github.com/acme/service.git").as_deref(),
            Some("acme/service")
        );
        assert_eq!(
            github_repo_slug("ssh://git@github.com/acme/service").as_deref(),
            Some("acme/service")
        );
        assert_eq!(github_repo_slug("https://bitbucket.org/acme/service"), None);
    }

    #[test]
    fn bitbucket_cloud_origin_parsing_is_presence_only() {
        assert_eq!(
            bitbucket_repo_slug("git@bitbucket.org:acme/service.git").as_deref(),
            Some("acme/service")
        );
        assert_eq!(
            bitbucket_repo_slug("https://bitbucket.org/acme/service.git").as_deref(),
            Some("acme/service")
        );
        assert_eq!(
            bitbucket_repo_slug("ssh://git@bitbucket.org/acme/service").as_deref(),
            Some("acme/service")
        );
        assert_eq!(bitbucket_repo_slug("https://github.com/acme/service"), None);
    }

    #[test]
    fn pr_url_is_extracted_without_parsing_credentials() {
        assert_eq!(
            pr_url_from_output("Creating pull request\nhttps://github.com/acme/service/pull/12\n")
                .as_deref(),
            Some("https://github.com/acme/service/pull/12")
        );
        assert_eq!(
            pr_url_from_bitbucket_response(
                r#"{"links":{"html":{"href":"https://bitbucket.org/acme/service/pull-requests/12"}}}"#
            )
            .as_deref(),
            Some("https://bitbucket.org/acme/service/pull-requests/12")
        );
    }

    #[test]
    fn curl_config_escapes_secret_bearing_values_for_stdin_config() {
        assert_eq!(curl_config_escape(r#"a\b"c"#), r#"a\\b\"c"#);
    }

    #[test]
    fn delivery_attempt_records_allowed_forge_run_evidence() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        record_delivery_event(
            &kernel,
            "forge_run_delivery",
            "LIVE_PR_OPENED",
            "github_pr_opened",
            "github",
            "forge/run_delivery",
            "main",
            "https://github.com/acme/service/pull/12",
        );

        let kernel = kernel.read().expect("kernel");
        let receipt = kernel
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| {
                receipt.payload.get("run_id").map(String::as_str) == Some("forge_run_delivery")
            })
            .expect("delivery evidence receipt");
        assert_eq!(
            receipt.payload.get("event").map(String::as_str),
            Some("evidence_appended")
        );
        assert_eq!(
            receipt.payload.get("delivery_status").map(String::as_str),
            Some("LIVE_PR_OPENED")
        );
        assert_eq!(
            receipt.payload.get("pull_request_url").map(String::as_str),
            Some("https://github.com/acme/service/pull/12")
        );
        assert_eq!(
            receipt
                .payload
                .get("production_write_allowed")
                .map(String::as_str),
            Some("false")
        );
    }
}
