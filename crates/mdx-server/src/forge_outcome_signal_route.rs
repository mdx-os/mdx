// Forge outcome signals: the receipt-backed producer edge for the flywheel.
// Runs can emit these automatically when they finish, and the route exposes
// them as both a governed write and a projection. Nothing here activates
// memory, adapts behavior, or opens execution authority.
use crate::RouteResponse;
use crate::forge_flywheel_proof_route::LapMetrics;
use crate::forge_loop_runner::{ForgeRunOutcome, ForgeRunRequest};
use crate::forge_turn_client::TurnClient;
use mdx_core::{ForgeOutcomeSignal, MdxKernel, Receipt, json_string_literal};
use std::sync::{Arc, RwLock};

// Kernel MAX_TEXT_CHARS: a lesson longer than this is refused at the write.
const MAX_LESSON_CHARS: usize = 2000;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/outcome-signals.json" => Some(handle_post(method, body, kernel)),
        "/forge/outcome-signals/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

pub(crate) fn record_run_outcome_signal(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
) {
    // Read stage: assemble ledger evidence under a read lock so run threads
    // and routes are not blocked while the lesson is composed.
    let evidence = {
        let Ok(kernel) = kernel.read() else {
            return;
        };
        if outcome_already_recorded(&kernel, &request.run_id) {
            return;
        }
        assemble_run_evidence(&kernel, request)
    };
    if evidence.source_receipt_id.is_empty() {
        return;
    }
    let deterministic = deterministic_lesson(request, outcome, &evidence);
    // Distill stage: the optional model pass runs with NO kernel lock held.
    let (lesson, lesson_source) =
        distilled_lesson(kernel, request, outcome, &evidence, &deterministic);
    let summary = outcome_summary(outcome);
    let Ok(mut kernel) = kernel.write() else {
        return;
    };
    // The lock was dropped across the distill stage, so a sibling caller may
    // have recorded this run in the meantime: re-verify under the write lock.
    if outcome_already_recorded(&kernel, &request.run_id) {
        return;
    }
    if let Err(error) = kernel.record_forge_outcome_signal(ForgeOutcomeSignal {
        tenant_id: &request.tenant_id,
        actor_id: "agent:forge",
        run_id: &request.run_id,
        source_receipt_id: &evidence.source_receipt_id,
        source_receipt_kind: "forge.run.event",
        disposition: disposition_for(outcome.status),
        summary: &summary,
        capability_ids: "",
        model_or_worker: &evidence.model_or_worker,
        lesson_candidate: &lesson,
        lesson_source,
        message_channel_id: "forge",
    }) {
        // A run that finished but cannot enter the flywheel is itself
        // flywheel evidence; witness the refusal instead of dropping it.
        let detail = format!("outcome signal refused: {}", error.message());
        if kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: &request.tenant_id,
                actor_id: "agent:forge",
                run_id: &request.run_id,
                event: "evidence_appended",
                work_item_id: &request.work_item_id,
                detail: &detail,
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .is_err()
        {
            eprintln!(
                "forge outcome signal for {} dropped: {}",
                request.run_id,
                error.message()
            );
        }
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
        "agent:forge",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_forge_outcome_signal_with_identity(
        ForgeOutcomeSignal {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            run_id: &field("run_id"),
            source_receipt_id: &field("source_receipt_id"),
            source_receipt_kind: &field("source_receipt_kind"),
            disposition: &field("disposition"),
            summary: &field("summary"),
            capability_ids: &field("capability_ids"),
            model_or_worker: &field("model_or_worker"),
            lesson_candidate: &field("lesson_candidate"),
            lesson_source: &field("lesson_source"),
            message_channel_id: &field("message_channel_id"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-outcome-signal-local-post","status":"RECORDED","outcome_signal_id":{},"outcome_signal_receipt_id":{},"policy_decision_id":{},"disposition":{},"projection_route":"/forge/outcome-signals/projection.json","learning_candidate_allowed":true,"message_activity_allowed":true,"active_memory_write_allowed":false,"adaptation_allowed":false,"execution_authority_opened":false,"production_write_allowed":false}}"#,
            json_string_literal(&report.outcome_signal_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(&report.disposition),
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
    let mut items: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("forge.outcome.signal.recorded")
        .iter()
        .map(signal_json)
        .collect();
    items.reverse();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-outcome-signal-local-projection","status":"OK","receipt_kind":"forge.outcome.signal.recorded","writes_route":"/forge/outcome-signals.json","outcome_signal_count":{},"signals":[{}],"learning_candidate_allowed":true,"message_activity_allowed":true,"active_memory_write_allowed":false,"adaptation_allowed":false,"execution_authority_opened":false,"production_write_allowed":false}}"#,
            items.len(),
            items.join(","),
        ),
    ))
}

fn signal_json(receipt: &&Receipt) -> String {
    let value = |key: &str| json_string_literal(pv(receipt, key));
    format!(
        r#"{{"outcome_signal_receipt_id":{},"outcome_signal_id":{},"run_id":{},"source_receipt_id":{},"source_receipt_kind":{},"disposition":{},"summary":{},"capability_ids":{},"model_or_worker":{},"lesson_candidate":{},"lesson_source":{},"message_channel_id":{},"actor_id":{},"learning_candidate_allowed":{},"message_activity_allowed":{},"active_memory_write_allowed":false,"adaptation_allowed":false,"execution_authority_opened":false}}"#,
        json_string_literal(&receipt.receipt_id),
        value("outcome_signal_id"),
        value("run_id"),
        value("source_receipt_id"),
        value("source_receipt_kind"),
        value("disposition"),
        value("summary"),
        value("capability_ids"),
        value("model_or_worker"),
        value("lesson_candidate"),
        value("lesson_source"),
        value("message_channel_id"),
        json_string_literal(receipt.actor_id.as_str()),
        bool_payload(receipt, "learning_candidate_allowed"),
        bool_payload(receipt, "message_activity_allowed"),
    )
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-outcome-signal-local-post","status":"REFUSED","reason":{},"outcome_signal_receipt_id":"","learning_candidate_allowed":false,"message_activity_allowed":false,"active_memory_write_allowed":false,"adaptation_allowed":false,"execution_authority_opened":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn outcome_already_recorded(kernel: &MdxKernel, run_id: &str) -> bool {
    kernel
        .ledger()
        .query()
        .by_kind("forge.outcome.signal.recorded")
        .iter()
        .any(|receipt| pv(receipt, "run_id") == run_id)
}

fn latest_run_finished_receipt(kernel: &MdxKernel, run_id: &str) -> Option<String> {
    latest_run_receipt_matching(kernel, run_id, Some("run_finished"))
}

fn latest_run_event_receipt(kernel: &MdxKernel, run_id: &str) -> Option<String> {
    latest_run_receipt_matching(kernel, run_id, None)
}

fn latest_run_receipt_matching(
    kernel: &MdxKernel,
    run_id: &str,
    event: Option<&str>,
) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            pv(receipt, "run_id") == run_id
                && event
                    .map(|expected| pv(receipt, "event") == expected)
                    .unwrap_or(true)
        })
        .map(|receipt| receipt.receipt_id.clone())
}

fn model_or_worker(kernel: &MdxKernel, run_id: &str) -> String {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .find(|receipt| pv(receipt, "run_id") == run_id && pv(receipt, "event") == "model_called")
        .and_then(|receipt| {
            pv(receipt, "detail")
                .split_whitespace()
                .find_map(|part| part.strip_prefix("model="))
        })
        .unwrap_or("local_forge_worker")
        .to_string()
}

fn outcome_summary(outcome: &ForgeRunOutcome) -> String {
    let branch = outcome.branch.as_deref().unwrap_or("no branch");
    let check_line = if outcome.last_check_passed {
        "checks passed"
    } else {
        "checks not proven green"
    };
    format!(
        "{} after {} turns, {} files changed, {check_line}, {branch}. {}",
        outcome.status,
        outcome.turns_used,
        outcome.files_changed,
        outcome.finish_summary.trim()
    )
}

/// The ledger evidence a lesson is distilled from, assembled under one read
/// lock so the model stage can run without holding the kernel.
struct RunLessonEvidence {
    source_receipt_id: String,
    model_or_worker: String,
    refusal_details: Vec<String>,
    check_passed_count: u64,
    check_failed_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    prior_laps: Vec<LapMetrics>,
}

fn assemble_run_evidence(kernel: &MdxKernel, request: &ForgeRunRequest) -> RunLessonEvidence {
    let source_receipt_id = latest_run_finished_receipt(kernel, &request.run_id)
        .unwrap_or_else(|| latest_run_event_receipt(kernel, &request.run_id).unwrap_or_default());
    let model_or_worker = model_or_worker(kernel, &request.run_id);
    let refusal_details: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| pv(receipt, "run_id") == request.run_id)
        .filter_map(|receipt| {
            let detail = pv(receipt, "detail");
            detail
                .to_ascii_lowercase()
                .contains("refus")
                .then(|| detail.chars().take(240).collect::<String>())
        })
        .collect();
    let mut laps = crate::forge_flywheel_proof_route::fold_laps(kernel);
    let current = laps.remove(&request.run_id);
    let current_finish_index = current
        .as_ref()
        .map(|lap| lap.finish_index)
        .unwrap_or(usize::MAX);
    let mut prior_laps: Vec<LapMetrics> = laps
        .into_values()
        .filter(|lap| {
            !request.work_item_id.is_empty()
                && lap.work_item_id == request.work_item_id
                && lap.finish_index < current_finish_index
        })
        .collect();
    prior_laps.sort_by_key(|lap| lap.finish_index);
    let (check_passed_count, check_failed_count, input_tokens, output_tokens) = current
        .map(|lap| {
            (
                lap.check_passed_count,
                lap.check_failed_count,
                lap.input_tokens,
                lap.output_tokens,
            )
        })
        .unwrap_or_default();
    RunLessonEvidence {
        source_receipt_id,
        model_or_worker,
        refusal_details,
        check_passed_count,
        check_failed_count,
        input_tokens,
        output_tokens,
        prior_laps,
    }
}

/// The guaranteed floor: a 2-4 sentence lesson composed only from evidence
/// the ledger already holds. Never exceeds the kernel text limit.
fn deterministic_lesson(
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
    evidence: &RunLessonEvidence,
) -> String {
    let mut sentences: Vec<String> = Vec::new();
    if let Some(prior) = evidence.prior_laps.last() {
        sentences.push(format!(
            "Attempt {} on {}: the prior lap ({}) ended {} with {} files changed and {} failed checks.",
            evidence.prior_laps.len() + 1,
            request.work_item_id,
            prior.run_id,
            prior.status,
            prior.files_changed,
            prior.check_failed_count,
        ));
    }
    let check_line = if outcome.check_runs == 0 {
        "ran no checks"
    } else if outcome.last_check_passed {
        "proved the selected check green"
    } else {
        "did not prove the selected check green"
    };
    let intent = lesson_intent_line(&request.intent);
    sentences.push(match disposition_for(outcome.status) {
        "no_change" => format!(
            "This run refused to change code for \"{intent}\" after {} turns and {check_line}.",
            outcome.turns_used,
        ),
        "completed" => format!(
            "This run finished \"{intent}\" after {} turns, changed {} files, and {check_line}.",
            outcome.turns_used, outcome.files_changed,
        ),
        _ => format!(
            "This run ended {} on \"{intent}\" after {} turns with {} files changed and {check_line}.",
            outcome.status, outcome.turns_used, outcome.files_changed,
        ),
    });
    if let Some(refusal) = evidence.refusal_details.last() {
        sentences.push(format!("Run evidence recorded: {refusal}."));
    } else if !outcome.finish_summary.trim().is_empty() {
        // Carry the run's own diagnosis and fix (the richest signal) into the
        // deterministic lesson for completed-with-changes runs too, not only
        // when no files changed. This is what makes the offline or no-provider
        // lesson actionable ("changed > to >= in discount.js") rather than a
        // bare count of turns and files.
        let account: String = outcome.finish_summary.trim().chars().take(280).collect();
        sentences.push(format!("The run's own account: {account}."));
    }
    if let Some(prior) = evidence.prior_laps.last() {
        if prior.check_failed_count > 0 && outcome.last_check_passed && outcome.files_changed > 0 {
            sentences.push(
                "The check flip against the prior lap suggests the earlier failure was in the patch, not the approach.".to_string(),
            );
        } else if outcome.files_changed == 0 && prior.files_changed == 0 {
            sentences.push(
                "Repeated laps without a change on this work item point at the intent or fix location, not the worker.".to_string(),
            );
        }
    } else if sentences.len() < 2 {
        sentences.push(format!(
            "Evidence for this run: {} with {} checks passed, {} failed, {} input and {} output tokens.",
            evidence.model_or_worker,
            evidence.check_passed_count,
            evidence.check_failed_count,
            evidence.input_tokens,
            evidence.output_tokens,
        ));
    }
    sentences.truncate(4);
    bounded_lesson_text(&sentences.join(" "))
}

fn lesson_intent_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
}

fn bounded_lesson_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_LESSON_CHARS {
        trimmed.to_string()
    } else {
        trimmed.chars().take(MAX_LESSON_CHARS).collect()
    }
}

/// The optional model stage: ask a reviewer-class model for a sharper
/// distillation of the assembled evidence, falling back to the deterministic
/// lesson on any provider error or claim-guard failure. No kernel lock is
/// held across the completion call.
fn distilled_lesson(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
    evidence: &RunLessonEvidence,
    deterministic: &str,
) -> (String, &'static str) {
    let fallback = || (deterministic.to_string(), "deterministic");
    let Some(client) = lesson_distill_client(kernel, &request.tenant_id) else {
        return fallback();
    };
    let fact_source = lesson_fact_source(request, outcome, evidence, deterministic);
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "You distill one lesson from Forge run evidence so the next run on similar work starts smarter. Output only the lesson: 2 to 4 plain factual sentences. First state what the run proved or refused and why. Then end with one imperative sentence telling the next run what to do or check first (name the root-cause location and the change if the evidence shows them). Cite only evidence shown to you; never invent receipt ids, files, checks, or outcomes. No em dashes or en dashes."
        }),
        serde_json::json!({
            "role": "user",
            "content": fact_source
        }),
    ];
    let started = std::time::Instant::now();
    let candidate = match client.complete(&messages) {
        Ok(turn) => {
            if let Ok(mut guard) = kernel.write() {
                let _ = client.record_route_outcome(
                    &mut guard,
                    &request.tenant_id,
                    "agent:forge-lesson-distiller",
                    &turn,
                    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    "forge_advisor_runtime",
                );
            }
            turn.text.trim().to_string()
        }
        Err(_) => {
            if let Ok(mut guard) = kernel.write() {
                let _ = crate::model_fabric_service::persist_pending_model_runtime_evidence(
                    &mut guard,
                    &request.tenant_id,
                    "agent:forge-lesson-distiller",
                );
            }
            return fallback();
        }
    };
    if lesson_claims_safe(kernel, &candidate) {
        (bounded_lesson_text(&candidate), "model_distilled")
    } else {
        fallback()
    }
}

fn lesson_distill_client(kernel: &Arc<RwLock<MdxKernel>>, tenant_id: &str) -> Option<TurnClient> {
    let mut guard = kernel.write().ok()?;
    match TurnClient::routed_for_role(
        &mut guard,
        tenant_id,
        "agent:forge-lesson-distiller",
        "forge-lesson-distill",
        "advisor",
        "",
    ) {
        Ok(Some(client)) => Some(client),
        Ok(None) => TurnClient::for_role(&guard, tenant_id, "reviewer")
            .or_else(|| TurnClient::for_role(&guard, tenant_id, "planner")),
        Err(_) => None,
    }
}

/// A bounded digest of what the run actually did, mined from its durable
/// transcript: the decisions the model narrated and the commands it ran. This
/// turns lesson distillation from "counts and a status" into "here is the path
/// the run took", so the candidate lesson can be concrete about what worked or
/// failed instead of generic. Empty when the run kept no transcript (older
/// runs, or a persist that never landed), so distillation degrades exactly to
/// its prior ledger-only behavior.
fn transcript_digest(run_id: &str) -> String {
    let Some(messages) = crate::forge_transcript_store::load_transcript_by_run(run_id) else {
        return String::new();
    };
    let mut decisions: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    for message in &messages {
        if message["role"].as_str() != Some("assistant") {
            continue;
        }
        if let Some(text) = message["content"].as_str() {
            let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !line.is_empty() && line.chars().count() > 12 {
                decisions.push(line.chars().take(240).collect::<String>());
            }
        }
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                let name = call["function"]["name"].as_str().unwrap_or("");
                let args_raw = call["function"]["arguments"].as_str().unwrap_or("{}");
                let args: serde_json::Value =
                    serde_json::from_str(args_raw).unwrap_or(serde_json::Value::Null);
                if name == "run_command"
                    && let Some(command) = args["command"].as_str()
                    && !commands.iter().any(|existing| existing == command)
                {
                    commands.push(command.to_string());
                }
            }
        }
    }
    // Keep the most recent decisions - the ones nearest the outcome carry the
    // most signal - and cap the whole digest so the fact source stays lean.
    let mut recent_decisions: Vec<String> = decisions.into_iter().rev().take(6).collect();
    recent_decisions.reverse();
    let decisions_block = if recent_decisions.is_empty() {
        "- none recorded".to_string()
    } else {
        recent_decisions
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let commands_block = if commands.is_empty() {
        "- none recorded".to_string()
    } else {
        commands
            .iter()
            .take(10)
            .map(|command| format!("- {command}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let digest = format!(
        "What the run decided (recent narration):\n{decisions_block}\nCommands it ran:\n{commands_block}"
    );
    digest.chars().take(3000).collect()
}

fn lesson_fact_source(
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
    evidence: &RunLessonEvidence,
    deterministic: &str,
) -> String {
    let prior_lines: Vec<String> = evidence
        .prior_laps
        .iter()
        .map(|lap| {
            format!(
                "- {} ended {} with {} files changed, {} checks passed, {} failed",
                lap.run_id,
                lap.status,
                lap.files_changed,
                lap.check_passed_count,
                lap.check_failed_count,
            )
        })
        .collect();
    let digest = transcript_digest(&request.run_id);
    let transcript_section = if digest.is_empty() {
        "Run transcript digest: none (this run kept no transcript)".to_string()
    } else {
        format!("Run transcript digest (what the run actually did):\n{digest}")
    };
    format!(
        "Intent: {}\nWork item: {}\nStatus: {}\nTurns: {}\nFiles changed: {}\nCheck runs: {} (last selected proof passed: {})\nChecks passed/failed this run: {}/{}\nTokens in/out: {}/{}\nModel or worker: {}\nFinish summary: {}\nRefusal evidence:\n{}\nPrior laps on the same work item:\n{}\n{transcript_section}\nDeterministic lesson (your floor; sharpen it, same facts only):\n{}",
        lesson_intent_line(&request.intent),
        request.work_item_id,
        outcome.status,
        outcome.turns_used,
        outcome.files_changed,
        outcome.check_runs,
        outcome.last_check_passed,
        evidence.check_passed_count,
        evidence.check_failed_count,
        evidence.input_tokens,
        evidence.output_tokens,
        evidence.model_or_worker,
        lesson_intent_line(&outcome.finish_summary),
        if evidence.refusal_details.is_empty() {
            "- none".to_string()
        } else {
            evidence
                .refusal_details
                .iter()
                .map(|detail| format!("- {detail}"))
                .collect::<Vec<_>>()
                .join("\n")
        },
        if prior_lines.is_empty() {
            "- none".to_string()
        } else {
            prior_lines.join("\n")
        },
        deterministic,
    )
}

/// Claim guard: a distilled lesson is rejected when it is empty, too long,
/// uses banned dashes, or cites a receipt id the ledger does not hold.
fn lesson_claims_safe(kernel: &Arc<RwLock<MdxKernel>>, candidate: &str) -> bool {
    let trimmed = candidate.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_LESSON_CHARS {
        return false;
    }
    if trimmed.contains('\u{2014}') || trimmed.contains('\u{2013}') {
        return false;
    }
    let Ok(kernel) = kernel.read() else {
        return false;
    };
    trimmed
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| token.contains("receipt_"))
        .all(|token| kernel.ledger().query().by_id(token).is_some())
}

fn disposition_for(status: &str) -> &'static str {
    match status {
        "RUN_FINISHED_DONE" => "completed",
        "RUN_FINISHED_NO_CHANGE" => "no_change",
        "RUN_STOPPED" => "stopped",
        "RUN_BUDGET_EXHAUSTED" => "budget_exhausted",
        "RUN_FINISHED_CANNOT_PROCEED" => "blocked",
        _ if status.contains("ERROR") || status.contains("FAILED") => "failed",
        _ => "blocked",
    }
}

fn pv<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn bool_payload(receipt: &Receipt, key: &str) -> &'static str {
    if pv(receipt, key) == "true" {
        "true"
    } else {
        "false"
    }
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
    use mdx_core::ForgeRunEvent;

    #[test]
    fn transcript_digest_mines_decisions_and_commands_from_the_stored_run() {
        let run_id = "forge_run_lesson_digest_test";
        let messages = vec![
            serde_json::json!({ "role": "system", "content": "You are a builder." }),
            serde_json::json!({ "role": "user", "content": "Fix the failing test." }),
            serde_json::json!({
                "role": "assistant",
                "content": "The failure is a boundary check; I will add a guard for the empty slice.",
                "tool_calls": [{
                    "id": "c1", "type": "function",
                    "function": { "name": "run_command", "arguments": "{\"command\":\"cargo test -p mdx-core\"}" }
                }]
            }),
            crate::forge_turn_client::tool_result_message("c1", "test passed"),
        ];
        crate::forge_transcript_store::store_transcript(run_id, &messages).expect("stored");

        let digest = transcript_digest(run_id);
        assert!(
            digest.contains("boundary check"),
            "the run's narrated decision is mined into the digest"
        );
        assert!(
            digest.contains("cargo test -p mdx-core"),
            "the commands the run ran are mined into the digest"
        );

        let _ = std::fs::remove_file(format!(
            "{}/{}.json",
            crate::forge_transcript_store::TRANSCRIPTS_DIR,
            run_id
        ));
    }

    #[test]
    fn transcript_digest_is_empty_without_a_stored_transcript() {
        assert!(transcript_digest("forge_run_lesson_no_transcript").is_empty());
    }

    fn test_request(run_id: &str, work_item_id: &str) -> ForgeRunRequest {
        ForgeRunRequest {
            run_id: run_id.to_string(),
            tenant_id: "local_tenant".to_string(),
            actor_id: "human:eng".to_string(),
            work_item_id: work_item_id.to_string(),
            intent: "test".to_string(),
            allowed_commands: Vec::new(),
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        }
    }

    #[test]
    fn records_run_outcome_once_from_finished_run() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = test_request("forge_run_test", "work_item_1");
        {
            let mut k = kernel.write().expect("kernel");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_run_test",
                event: "run_finished",
                work_item_id: "work_item_1",
                detail: "status=RUN_FINISHED_DONE turns=3 files_changed=1",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("event");
        }
        let outcome = ForgeRunOutcome {
            run_id: "forge_run_test".to_string(),
            status: "RUN_FINISHED_DONE",
            turns_used: 3,
            files_changed: 1,
            check_runs: 1,
            check_duration_ms: 12,
            branch: Some("forge/run-test".to_string()),
            commit_sha: Some("abc123".to_string()),
            finish_summary: "done".to_string(),
            last_check_passed: true,
        };
        record_run_outcome_signal(&kernel, &request, &outcome);
        record_run_outcome_signal(&kernel, &request, &outcome);
        let k = kernel.read().expect("kernel");
        assert_eq!(
            k.ledger()
                .query()
                .by_kind("forge.outcome.signal.recorded")
                .len(),
            1
        );
    }

    #[test]
    fn no_change_run_outcome_records_no_change_disposition() {
        assert_eq!(disposition_for("RUN_FINISHED_NO_CHANGE"), "no_change");
    }

    #[test]
    fn every_disposition_round_trips_through_the_kernel_enum() {
        // Drift guard: a disposition the route emits but the kernel refuses
        // silently drops that outcome class from the flywheel.
        let statuses = [
            "RUN_FINISHED_DONE",
            "RUN_FINISHED_NO_CHANGE",
            "RUN_STOPPED",
            "RUN_BUDGET_EXHAUSTED",
            "RUN_FINISHED_CANNOT_PROCEED",
            "RUN_ERROR_MODEL",
            "RUN_FAILED_CHECKS",
            "RUN_SOMETHING_UNEXPECTED",
        ];
        for status in statuses {
            let disposition = disposition_for(status);
            assert!(
                mdx_core::FORGE_OUTCOME_DISPOSITIONS.contains(&disposition),
                "disposition_for({status}) = {disposition} is not a kernel disposition"
            );
        }
    }

    #[test]
    fn no_change_outcome_signal_is_accepted_by_the_kernel() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = test_request("forge_run_no_change", "work_item_1");
        {
            let mut k = kernel.write().expect("kernel");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_run_no_change",
                event: "run_finished",
                work_item_id: "work_item_1",
                detail: "status=RUN_FINISHED_NO_CHANGE turns=2 files_changed=0",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("event");
        }
        let outcome = ForgeRunOutcome {
            run_id: "forge_run_no_change".to_string(),
            status: "RUN_FINISHED_NO_CHANGE",
            turns_used: 2,
            files_changed: 0,
            check_runs: 1,
            check_duration_ms: 8,
            branch: None,
            commit_sha: None,
            finish_summary: "refused a wrong fix".to_string(),
            last_check_passed: true,
        };
        record_run_outcome_signal(&kernel, &request, &outcome);
        let k = kernel.read().expect("kernel");
        let signals = k
            .ledger()
            .query()
            .by_kind("forge.outcome.signal.recorded")
            .iter()
            .filter(|receipt| {
                receipt.payload.get("run_id").map(String::as_str) == Some("forge_run_no_change")
            })
            .count();
        assert_eq!(
            signals, 1,
            "no_change outcomes must enter the flywheel, not vanish"
        );
    }

    #[test]
    fn no_change_lesson_names_the_refusal_reason_from_run_evidence() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = test_request("forge_run_refusal", "work_item_refusal");
        {
            let mut k = kernel.write().expect("kernel");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_run_refusal",
                event: "evidence_appended",
                work_item_id: "work_item_refusal",
                detail: "finish refused missing post-change proof",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("refusal event");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_run_refusal",
                event: "run_finished",
                work_item_id: "work_item_refusal",
                detail: "status=RUN_FINISHED_NO_CHANGE turns=2 files_changed=0",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("event");
        }
        let outcome = ForgeRunOutcome {
            run_id: "forge_run_refusal".to_string(),
            status: "RUN_FINISHED_NO_CHANGE",
            turns_used: 2,
            files_changed: 0,
            check_runs: 1,
            check_duration_ms: 8,
            branch: None,
            commit_sha: None,
            finish_summary: "refused a wrong fix".to_string(),
            last_check_passed: true,
        };
        // Content is asserted on the deterministic stage directly so the test
        // stays hermetic when a live model endpoint is reachable locally.
        let lesson = {
            let k = kernel.read().expect("kernel");
            let evidence = assemble_run_evidence(&k, &request);
            deterministic_lesson(&request, &outcome, &evidence)
        };
        assert!(
            lesson.contains("refused to change code"),
            "lesson must state the refusal: {lesson}"
        );
        assert!(
            lesson.contains("finish refused missing post-change proof"),
            "lesson must cite the recorded refusal reason: {lesson}"
        );
        assert!(
            !lesson.contains("Candidate lesson:"),
            "canned template text must be gone: {lesson}"
        );
        record_run_outcome_signal(&kernel, &request, &outcome);
        let k = kernel.read().expect("kernel");
        let receipt = k
            .ledger()
            .query()
            .by_kind("forge.outcome.signal.recorded")
            .into_iter()
            .find(|receipt| pv(receipt, "run_id") == "forge_run_refusal")
            .expect("signal recorded");
        let recorded_lesson = pv(receipt, "lesson_candidate");
        assert!(!recorded_lesson.is_empty());
        assert!(recorded_lesson.chars().count() <= MAX_LESSON_CHARS);
        assert!(
            mdx_core::FORGE_OUTCOME_LESSON_SOURCES.contains(&pv(receipt, "lesson_source")),
            "lesson_source must be stamped on the receipt"
        );
    }

    #[test]
    fn second_lap_lesson_cites_the_prior_lap_and_the_check_flip() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = test_request("forge_lap_2", "work_item_shared");
        {
            let mut k = kernel.write().expect("kernel");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_lap_1",
                event: "check_failed",
                work_item_id: "work_item_shared",
                detail: "run_command cargo test exit=1",
                turn: 4,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lap one check failed");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_lap_1",
                event: "run_finished",
                work_item_id: "work_item_shared",
                detail: "status=RUN_FAILED_CHECKS turns=5 files_changed=2",
                turn: 5,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lap one finished");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_lap_2",
                event: "run_finished",
                work_item_id: "work_item_shared",
                detail: "status=RUN_FINISHED_DONE turns=3 files_changed=3",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lap two finished");
        }
        let outcome = ForgeRunOutcome {
            run_id: "forge_lap_2".to_string(),
            status: "RUN_FINISHED_DONE",
            turns_used: 3,
            files_changed: 3,
            check_runs: 1,
            check_duration_ms: 10,
            branch: Some("forge/run-lap-2".to_string()),
            commit_sha: Some("def456".to_string()),
            finish_summary: "done".to_string(),
            last_check_passed: true,
        };
        // Deterministic stage asserted directly: hermetic against any live
        // local model endpoint.
        let lesson = {
            let k = kernel.read().expect("kernel");
            let evidence = assemble_run_evidence(&k, &request);
            deterministic_lesson(&request, &outcome, &evidence)
        };
        assert!(
            lesson.contains("Attempt 2 on work_item_shared"),
            "lesson must frame the lap count: {lesson}"
        );
        assert!(
            lesson.contains("forge_lap_1"),
            "lesson must cite the prior lap: {lesson}"
        );
        assert!(
            lesson.contains("check flip"),
            "lesson must read the failed-to-green flip: {lesson}"
        );
    }

    #[test]
    fn deterministic_lesson_never_exceeds_the_kernel_text_limit() {
        let request = test_request("forge_run_long", &"x".repeat(6000));
        let outcome = ForgeRunOutcome {
            run_id: "forge_run_long".to_string(),
            status: "RUN_FINISHED_DONE",
            turns_used: 9,
            files_changed: 4,
            check_runs: 2,
            check_duration_ms: 20,
            branch: None,
            commit_sha: None,
            finish_summary: "y".repeat(6000),
            last_check_passed: true,
        };
        let evidence = RunLessonEvidence {
            source_receipt_id: "receipt_1".to_string(),
            model_or_worker: "local_forge_worker".to_string(),
            refusal_details: vec!["z".repeat(240)],
            check_passed_count: 2,
            check_failed_count: 0,
            input_tokens: 100,
            output_tokens: 50,
            prior_laps: vec![LapMetrics {
                run_id: "r".repeat(4000),
                work_item_id: "x".repeat(6000),
                status: "RUN_FAILED_CHECKS".to_string(),
                files_changed: 1,
                check_failed_count: 3,
                ..LapMetrics::default()
            }],
        };
        let lesson = deterministic_lesson(&request, &outcome, &evidence);
        assert!(
            lesson.chars().count() <= MAX_LESSON_CHARS,
            "lesson is {} chars",
            lesson.chars().count()
        );
        assert!(!lesson.is_empty());
    }

    #[test]
    fn claim_guard_rejects_fabricated_receipt_ids() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let real_receipt_id = {
            let mut k = kernel.write().expect("kernel");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                run_id: "forge_run_guard",
                event: "run_finished",
                work_item_id: "work_item_guard",
                detail: "status=RUN_FINISHED_DONE turns=1 files_changed=1",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("event")
            .receipt_id
        };
        assert!(
            !lesson_claims_safe(
                &kernel,
                "The fix was proven by receipt_999999_fabricated on the ledger."
            ),
            "a fabricated receipt id must fail the guard"
        );
        assert!(
            lesson_claims_safe(
                &kernel,
                &format!("The fix was proven by {real_receipt_id} on the ledger.")
            ),
            "a real receipt id must pass the guard"
        );
        assert!(
            !lesson_claims_safe(&kernel, "A lesson \u{2014} with a banned dash."),
            "em dashes must fail the guard"
        );
        assert!(
            !lesson_claims_safe(&kernel, ""),
            "an empty lesson must fail the guard"
        );
    }
}
