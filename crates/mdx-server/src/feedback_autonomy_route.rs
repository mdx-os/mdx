use crate::RouteResponse;
use crate::forge_loop_runner::{ForgeRunRequest, run_forge_loop};
use mdx_core::{
    BETA_FEEDBACK_CHANNEL_ID, FEEDBACK_AUTONOMY_ACTOR_ID, FEEDBACK_AUTONOMY_PROJECTION_ROUTE,
    FEEDBACK_AUTONOMY_RECEIPTS, FeedbackAutonomyEvent, FeedbackAutonomyFeedback, ForgeRunEvent,
    GovernedWriteIdentity, MdxKernel, MessageThreadMessage, PagesPublication, WorkItemShape,
    classify_feedback_for_autonomy, feedback_autonomy_feedback, json_string_literal,
};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

struct ProcessedFeedback {
    json: String,
    pending_run: Option<PendingForgeStart>,
}

struct PendingForgeStart {
    request: ForgeRunRequest,
    repo_root: PathBuf,
}

struct ScopeResolution {
    status: &'static str,
    detail: &'static str,
    write_scope: Vec<String>,
    allowed_commands: Vec<String>,
}

struct FeedbackAutonomyEventInput<'a> {
    feedback: &'a FeedbackAutonomyFeedback,
    event: &'a str,
    lane: &'a str,
    detail: &'a str,
    page_ref: &'a str,
    work_item_ref: &'a str,
    forge_run_ref: &'a str,
    source_receipt_id: &'a str,
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/feedback/autonomy-runs.json" => Some(handle_post(method, body, kernel)),
        "/feedback/autonomy-runs/projection.json" => Some(handle_projection(method, kernel)),
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
    let requested_ref = json_string_field(body, "feedback_ref").unwrap_or_default();
    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let feedback = feedback_autonomy_feedback(guard.ledger().entries());
    let mut processed = Vec::new();
    let mut pending_runs = Vec::new();
    let mut skipped = 0usize;
    for item in feedback {
        if !requested_ref.is_empty() && requested_ref != item.feedback_ref {
            continue;
        }
        if guard.feedback_autonomy_already_closed(&item.feedback_ref) {
            skipped += 1;
            continue;
        }
        let outcome = process_feedback(&mut guard, &item)?;
        if let Some(pending) = outcome.pending_run {
            pending_runs.push(pending);
        }
        processed.push(outcome.json);
    }
    let projection = guard.feedback_autonomy_projection();
    drop(guard);
    for pending in pending_runs {
        let kernel_for_thread = Arc::clone(kernel);
        std::thread::Builder::new()
            .name(format!(
                "feedback-autonomy-forge-{}",
                pending.request.run_id
            ))
            .spawn(move || {
                // Catch a loop panic so an autonomous feedback run never aborts
                // the thread or escapes; the boundary snapshot still runs.
                if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_forge_loop(&pending.request, &pending.repo_root, &kernel_for_thread);
                })) {
                    eprintln!(
                        "feedback autonomy run thread panicked, recorded short of finish: {panic:?}"
                    );
                }
                crate::kernel_snapshot::snapshot_at_boundary(&kernel_for_thread);
            })
            .map_err(|error| format!("could not start feedback autonomy Forge run: {error}"))?;
    }
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-feedback-autonomy-local-post","status":"PROCESSED","processed_count":{},"skipped_count":{},"projection_route":{},"processed":[{}],"receipt_chain":[{}],"open_count":{},"production_write_allowed":false}}"#,
            processed.len(),
            skipped,
            json_string_literal(FEEDBACK_AUTONOMY_PROJECTION_ROUTE),
            processed.join(","),
            FEEDBACK_AUTONOMY_RECEIPTS
                .iter()
                .map(|kind| json_string_literal(kind))
                .collect::<Vec<_>>()
                .join(","),
            projection.open_count,
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
    Ok(RouteResponse::json(
        "200 OK",
        render_projection_json(&kernel),
    ))
}

fn process_feedback(
    kernel: &mut MdxKernel,
    feedback: &FeedbackAutonomyFeedback,
) -> Result<ProcessedFeedback, String> {
    let identity = GovernedWriteIdentity::local_demo(FEEDBACK_AUTONOMY_ACTOR_ID);
    let (lane, reason) =
        classify_feedback_for_autonomy(&feedback.note, &feedback.category, &feedback.surface);
    let scope = resolve_scope(feedback, lane);
    let suffix = stable_suffix(&feedback.feedback_ref);
    let thread_id = format!("thread_beta_feedback_{suffix}");
    let assessment_page = format!("feedback_assessment_{suffix}");
    let assessment_revision = format!("rev_feedback_assessment_{suffix}_001");
    let closeout_page = format!("feedback_closeout_{suffix}");
    let closeout_revision = format!("rev_feedback_closeout_{suffix}_001");
    let work_item_id = format!("work_item_feedback_{suffix}");
    let title = summarize_feedback(&feedback.note);
    let approval = if lane == "autopilot_allowed" {
        "auto"
    } else if lane == "autopilot_with_review" {
        "brief_first"
    } else {
        "plan_first"
    };

    let ack = kernel
        .save_message_thread_message_local_with_identity(
            MessageThreadMessage {
                tenant_id: "local_tenant",
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                thread_id: &thread_id,
                channel_id: BETA_FEEDBACK_CHANNEL_ID,
                message_id: &format!("feedback_ack_{suffix}"),
                content_type: "text",
                body: "Captured. MDx is assessing whether this should become governed work.",
                content_payload: "Captured. MDx is assessing whether this should become governed work.",
                source_receipt_id: &feedback.feedback_ref,
                outbox_topic: "message.beta_feedback.local",
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    record_event(
        kernel,
        &identity,
        FeedbackAutonomyEventInput {
            feedback,
            event: "acknowledged",
            lane,
            detail: "posted acknowledgement in #beta-feedback",
            page_ref: "",
            work_item_ref: "",
            forge_run_ref: "",
            source_receipt_id: &ack.message_receipt_id,
        },
    )?;

    record_event(
        kernel,
        &identity,
        FeedbackAutonomyEventInput {
            feedback,
            event: "assessed",
            lane,
            detail: reason,
            page_ref: "",
            work_item_ref: "",
            forge_run_ref: "",
            source_receipt_id: &feedback.feedback_ref,
        },
    )?;

    let assessment_body = assessment_body(feedback, lane, reason, &work_item_id);
    let assessment_body_ref = crate::pages_body_store::store_published_body(
        &assessment_page,
        &assessment_revision,
        &assessment_body,
    )
    .ok_or_else(|| "feedback assessment body could not be stored".to_string())?;
    let assessment = kernel
        .save_pages_publication_local_with_identity(
            PagesPublication {
                tenant_id: "local_tenant",
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                document_id: &assessment_page,
                title: &format!("Feedback assessment: {title}"),
                body_ref: &assessment_body_ref,
                source_receipt_id: &feedback.feedback_ref,
                revision_id: &assessment_revision,
                page_type: "signal",
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    record_event(
        kernel,
        &identity,
        FeedbackAutonomyEventInput {
            feedback,
            event: "assessment_published",
            lane,
            detail: "published assessment page",
            page_ref: &assessment.document_id,
            work_item_ref: "",
            forge_run_ref: "",
            source_receipt_id: &assessment.publication_receipt_id,
        },
    )?;

    let work = kernel
        .save_work_item_local_with_identity(
            WorkItemShape {
                tenant_id: "local_tenant",
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                work_item_id: &work_item_id,
                title: &title,
                description: &format!(
                    "Feedback autonomy lane: {lane}. Assessment: {reason}. Source feedback: {}. Assessment page: {assessment_page}.",
                    feedback.feedback_ref
                ),
                bet_id: "",
                owner_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                approval,
                page_ref: &assessment_page,
                build_ref: "",
            },
            &identity,
        )
        .map_err(|error| error.message())?;

    let pending_run = if lane == "autopilot_allowed" && !scope.write_scope.is_empty() {
        let run_id = kernel.mint_id("forge_run");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                run_id: &run_id,
                event: "run_started",
                work_item_id: &work.record_id,
                detail: &format!(
                    "feedback_autonomy: scope={} commands={}",
                    scope.write_scope.join("|"),
                    scope.allowed_commands.len()
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .map_err(|error| error.message())?;
        Some(PendingForgeStart {
            request: ForgeRunRequest {
                run_id,
                tenant_id: "local_tenant".to_string(),
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID.to_string(),
                work_item_id: work.record_id.clone(),
                intent: forge_intent(feedback, &assessment_page, &work.record_id, &scope),
                allowed_commands: scope.allowed_commands.clone(),
                max_turns: 12,
                revise_branch: None,
                resume: false,
                write_scope: scope.write_scope.clone(),
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
            },
            repo_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        })
    } else {
        None
    };
    let forge_run_ref = pending_run
        .as_ref()
        .map(|pending| pending.request.run_id.as_str())
        .unwrap_or("");
    let forge_detail_line = forge_detail(lane, &scope);

    record_event(
        kernel,
        &identity,
        FeedbackAutonomyEventInput {
            feedback,
            event: "forge_requested",
            lane,
            detail: &forge_detail_line,
            page_ref: &assessment_page,
            work_item_ref: &work.record_id,
            forge_run_ref,
            source_receipt_id: &work.receipt_id,
        },
    )?;

    record_event(
        kernel,
        &identity,
        FeedbackAutonomyEventInput {
            feedback,
            event: "evidence_attached",
            lane,
            detail: scope.detail,
            page_ref: &assessment_page,
            work_item_ref: &work.record_id,
            forge_run_ref,
            source_receipt_id: &work.receipt_id,
        },
    )?;

    let closeout_body = closeout_body(
        feedback,
        lane,
        reason,
        &assessment_page,
        &work.record_id,
        &forge_detail_line,
    );
    let closeout_body_ref = crate::pages_body_store::store_published_body(
        &closeout_page,
        &closeout_revision,
        &closeout_body,
    )
    .ok_or_else(|| "feedback closeout body could not be stored".to_string())?;
    let closeout = kernel
        .save_pages_publication_local_with_identity(
            PagesPublication {
                tenant_id: "local_tenant",
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                document_id: &closeout_page,
                title: &format!("Feedback closeout: {title}"),
                body_ref: &closeout_body_ref,
                source_receipt_id: &work.receipt_id,
                revision_id: &closeout_revision,
                page_type: "changelog",
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    record_event(
        kernel,
        &identity,
        FeedbackAutonomyEventInput {
            feedback,
            event: "closeout_published",
            lane,
            detail: "published closeout page for the feedback loop",
            page_ref: &closeout.document_id,
            work_item_ref: &work.record_id,
            forge_run_ref,
            source_receipt_id: &closeout.publication_receipt_id,
        },
    )?;

    let final_message = format!(
        "Assessment complete. Lane: {lane}. Work: {}. Assessment: {}. Closeout: {}.",
        work.record_id, assessment.document_id, closeout.document_id
    );
    let reply = kernel
        .save_message_thread_message_local_with_identity(
            MessageThreadMessage {
                tenant_id: "local_tenant",
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                thread_id: &thread_id,
                channel_id: BETA_FEEDBACK_CHANNEL_ID,
                message_id: &format!("feedback_close_{suffix}"),
                content_type: "text",
                body: &final_message,
                content_payload: &final_message,
                source_receipt_id: &closeout.publication_receipt_id,
                outbox_topic: "message.beta_feedback.local",
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    record_event(
        kernel,
        &identity,
        FeedbackAutonomyEventInput {
            feedback,
            event: "closed_loop_replied",
            lane,
            detail: "posted close-loop reply in #beta-feedback",
            page_ref: &closeout.document_id,
            work_item_ref: &work.record_id,
            forge_run_ref,
            source_receipt_id: &reply.message_receipt_id,
        },
    )?;

    Ok(ProcessedFeedback {
        json: format!(
            r#"{{"feedback_ref":{},"lane":{},"lifecycle":"closed_loop_replied","assessment_page_ref":{},"closeout_page_ref":{},"work_item_ref":{},"forge_run_ref":{},"scope_status":{},"write_scope":[{}],"forge_detail":{}}}"#,
            json_string_literal(&feedback.feedback_ref),
            json_string_literal(lane),
            json_string_literal(&assessment.document_id),
            json_string_literal(&closeout.document_id),
            json_string_literal(&work.record_id),
            json_string_literal(forge_run_ref),
            json_string_literal(scope.status),
            scope
                .write_scope
                .iter()
                .map(|path| json_string_literal(path))
                .collect::<Vec<_>>()
                .join(","),
            json_string_literal(&forge_detail_line),
        ),
        pending_run,
    })
}

fn record_event(
    kernel: &mut MdxKernel,
    identity: &GovernedWriteIdentity,
    input: FeedbackAutonomyEventInput<'_>,
) -> Result<(), String> {
    kernel
        .record_feedback_autonomy_event_with_identity(
            FeedbackAutonomyEvent {
                tenant_id: "local_tenant",
                actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                feedback_ref: &input.feedback.feedback_ref,
                event: input.event,
                lane: input.lane,
                detail: input.detail,
                page_ref: input.page_ref,
                work_item_ref: input.work_item_ref,
                forge_run_ref: input.forge_run_ref,
                source_receipt_id: input.source_receipt_id,
            },
            identity,
        )
        .map(|_| ())
        .map_err(|error| error.message())
}

fn assessment_body(
    feedback: &FeedbackAutonomyFeedback,
    lane: &str,
    reason: &str,
    work_item_id: &str,
) -> String {
    format!(
        "# Feedback assessment\n\nSource feedback: {}\nSurface: {}\nRoute: {}\nCategory: {}\n\nSignal\n{}\n\nLane: {}\nReason: {}\n\nDecision\nShape work item {} and route execution through the governed Forge handoff.",
        feedback.feedback_ref,
        feedback.surface,
        feedback.route,
        feedback.category,
        feedback.note,
        lane,
        reason,
        work_item_id,
    )
}

fn closeout_body(
    feedback: &FeedbackAutonomyFeedback,
    lane: &str,
    reason: &str,
    assessment_page: &str,
    work_item_id: &str,
    forge_detail: &str,
) -> String {
    format!(
        "# Feedback closeout\n\nFeedback: {}\nAssessment: {}\nWork item: {}\nLane: {}\n\nOutcome\n{}\n\nForge\n{}\n\nReason\n{}",
        feedback.feedback_ref,
        assessment_page,
        work_item_id,
        lane,
        feedback.note,
        forge_detail,
        reason
    )
}

fn forge_detail(lane: &str, scope: &ScopeResolution) -> String {
    match lane {
        "autopilot_allowed" if scope.status == "scope_resolved" => {
            format!(
                "scoped Forge run started with write scope: {}",
                scope.write_scope.join(", ")
            )
        }
        "autopilot_allowed" => {
            "autopilot allowed by rubric, but held because no deterministic write scope was resolved"
                .to_string()
        }
        "autopilot_with_review" => {
            "held for brief review before Forge because the change touches boundary or product scope"
                .to_string()
        }
        "needs_human" => "held for human attention before Forge".to_string(),
        "batch_later" => "queued for later batching before Forge".to_string(),
        _ => "not sent to Forge".to_string(),
    }
}

fn resolve_scope(feedback: &FeedbackAutonomyFeedback, lane: &str) -> ScopeResolution {
    // Fail-closed FIRST, before the note-based scope match. A telemetry-origin
    // capture carries an UNTRUSTED app-reported route in its note; the note match
    // below (feedback + copy/typo/label) must never resolve a real write scope for
    // it. Telemetry signals never get a deterministic scope; they are held for a
    // person. Defense in depth with the classifier short-circuit.
    if feedback.surface == mdx_core::APP_HEALTH_SELF_HEAL_SURFACE {
        return ScopeResolution {
            status: "scope_missing",
            detail: "telemetry-origin signals never resolve a deterministic write scope",
            write_scope: Vec::new(),
            allowed_commands: default_allowed_commands(),
        };
    }
    if lane != "autopilot_allowed" {
        return ScopeResolution {
            status: "not_applicable",
            detail: "scope not requested for this autonomy lane",
            write_scope: Vec::new(),
            allowed_commands: default_allowed_commands(),
        };
    }
    let lower = feedback.note.to_ascii_lowercase();
    if lower.contains("feedback")
        && ["copy", "typo", "label", "confusing"]
            .iter()
            .any(|needle| lower.contains(needle) || feedback.category == "confusing")
    {
        return ScopeResolution {
            status: "scope_resolved",
            detail: "resolved safe feedback-button copy scope",
            write_scope: vec!["packages/ui/components/FeedbackButton.svelte".to_string()],
            allowed_commands: default_allowed_commands(),
        };
    }
    match feedback.surface.as_str() {
        "product" => ScopeResolution {
            status: "scope_resolved",
            detail: "resolved safe Product surface copy scope",
            write_scope: vec![
                "apps/mdx-host/src/routes/product-direction/+page.svelte".to_string(),
            ],
            allowed_commands: default_allowed_commands(),
        },
        "message" => ScopeResolution {
            status: "scope_resolved",
            detail: "resolved safe Message surface copy scope",
            write_scope: vec!["apps/mdx-host/src/routes/message/+page.svelte".to_string()],
            allowed_commands: default_allowed_commands(),
        },
        "twin" => ScopeResolution {
            status: "scope_resolved",
            detail: "resolved safe Twin host surface copy scope",
            write_scope: vec!["apps/mdx-host/src/routes/twin/+page.svelte".to_string()],
            allowed_commands: default_allowed_commands(),
        },
        _ => ScopeResolution {
            status: "scope_missing",
            detail: "no deterministic write scope for this safe feedback yet",
            write_scope: Vec::new(),
            allowed_commands: default_allowed_commands(),
        },
    }
}

fn default_allowed_commands() -> Vec<String> {
    vec![
        "make feedback-autonomy-check".to_string(),
        "make beta-feedback-contract-check".to_string(),
        "make local-http-smoke".to_string(),
    ]
}

fn forge_intent(
    feedback: &FeedbackAutonomyFeedback,
    assessment_page: &str,
    work_item_id: &str,
    scope: &ScopeResolution,
) -> String {
    format!(
        "Autonomous beta-feedback fix. Feedback {} on surface {} category {} says: {}. Assessment page: {}. Work item: {}. Keep the change inside the declared write scope only: {}. Run the allowed checks and finish with evidence.",
        feedback.feedback_ref,
        feedback.surface,
        feedback.category,
        feedback.note,
        assessment_page,
        work_item_id,
        scope.write_scope.join(", ")
    )
}

fn render_projection_json(kernel: &MdxKernel) -> String {
    let projection = kernel.feedback_autonomy_projection();
    let rows = projection
        .rows
        .iter()
        .map(|row| {
            format!(
                r#"{{"feedback_ref":{},"surface":{},"route":{},"category":{},"lane":{},"lifecycle":{},"assessment_page_ref":{},"closeout_page_ref":{},"work_item_ref":{},"forge_run_ref":{},"acknowledged":{},"assessed":{},"assessment_published":{},"work_shaped":{},"forge_requested":{},"evidence_attached":{},"closeout_published":{},"closed_loop_replied":{}}}"#,
                json_string_literal(&row.feedback_ref),
                json_string_literal(&row.surface),
                json_string_literal(&row.route),
                json_string_literal(&row.category),
                json_string_literal(&row.lane),
                json_string_literal(&row.lifecycle),
                json_string_literal(&row.assessment_page_ref),
                json_string_literal(&row.closeout_page_ref),
                json_string_literal(&row.work_item_ref),
                json_string_literal(&row.forge_run_ref),
                row.acknowledged,
                row.assessed,
                row.assessment_published,
                row.work_shaped,
                row.forge_requested,
                row.evidence_attached,
                row.closeout_published,
                row.closed_loop_replied,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"name":"mdx-feedback-autonomy-local-projection","total_feedback":{},"processed_count":{},"open_count":{},"channel_id":{},"rows":[{}],"production_write_allowed":false}}"#,
        projection.total_feedback,
        projection.processed_count,
        projection.open_count,
        json_string_literal(BETA_FEEDBACK_CHANNEL_ID),
        rows,
    )
}

fn summarize_feedback(note: &str) -> String {
    let trimmed = note.trim();
    let summary = if trimmed.is_empty() {
        "Beta feedback follow-up".to_string()
    } else {
        trimmed.chars().take(90).collect::<String>()
    };
    if summary.chars().count() < trimmed.chars().count() {
        format!("{summary}...")
    } else {
        summary
    }
}

fn stable_suffix(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_ascii_lowercase()
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
    use mdx_core::FeedbackAutonomyFeedback;

    fn telemetry_feedback(note: &str) -> FeedbackAutonomyFeedback {
        FeedbackAutonomyFeedback {
            feedback_ref: "feedback_telemetry".to_string(),
            tenant_id: "tenant_local".to_string(),
            surface: "telemetry".to_string(),
            route: "/feedback/copy-fix.json".to_string(),
            actor_id: "agent:app_health".to_string(),
            session_ref: "app_health_self_heal_scan".to_string(),
            category: "bug".to_string(),
            note: note.to_string(),
            captured_at: "2026-07-04T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolve_scope_holds_telemetry_even_when_lane_is_autopilot() {
        // The exploit note ("feedback ... copy") would resolve a real write scope
        // on a normal surface. On the telemetry surface the short-circuit wins even
        // if a caller passes an autopilot_allowed lane: no deterministic scope.
        let feedback =
            telemetry_feedback("route /feedback/copy-fix.json returned status 404 in 3 sessions");
        let scope = resolve_scope(&feedback, "autopilot_allowed");
        assert_eq!(scope.status, "scope_missing");
        assert!(scope.write_scope.is_empty());
    }
}
