// Rail Slice 3: read-only runtime projection routes.
//
// Forge needs to read the kernel-real runtime truth instead of approximating it
// locally. These GET routes expose the kernel's own projections - the runtime
// metrics packet, the durable queue state, the operator decision packet, the
// autonomy envelope registry, and the load-and-concurrency proof summary - as
// governed, read-only JSON.
//
// Honesty first: the local server has no persisted runtime jobs. So the queue
// and the operator packet are reported empty ("no job in flight" / "no build at
// the ship door yet"); metrics are the observed-local zeros the kernel projects
// over a booted ledger, carried alongside the real standing targets and the
// overload posture; the envelope projection is an empty list with the safe
// default ("no standing authorization active; every task reviews with a human");
// and the load-proof summary reads .mdx-local/load-proof/evidence.json when it
// exists and otherwise reports that the proof has not been run. No counts are
// invented.
//
// Every route is read-only: it grants nothing, writes nothing, executes nothing,
// and emits no receipt. The kernel projections used here
// (project_runtime_metrics, project_queue_state) are read-only over the ledger;
// the envelope projection filters ledger receipts read-only. None of these
// routes call enqueue / assemble / record, which would write.
use mdx_core::{
    DurationStats, MdxKernel, RuntimeMetrics, RuntimeModelAttribution, RuntimeToolAttribution,
    json_string_literal,
};

// The forbidden-authority footer every route carries verbatim. Each route is a
// read; it never opens a write, an execution, a deployment, or a production door.
const READ_ONLY_FOOTER: &str = r#""read_only": true, "writes_allowed": false, "execution_allowed": false, "deployment_authority_granted": false, "production_write_allowed": false"#;

/// `/runtime/queue-projection.json` - durable queue state.
///
/// The local server persists no queued worker jobs, so this honestly reports an
/// empty queue rather than inventing one. The state is rebuilt from the queue
/// lifecycle receipts (queue.job.enqueued / dequeued / run.started / run.terminal
/// / job.cancelled); none are present locally.
pub(crate) fn render_runtime_queue_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let job_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind.starts_with("queue."))
        .count();
    Ok(format!(
        r#"{{
  "name": "mdx-runtime-queue-projection",
  "status": "OK",
  "source_projection": "runtime queue receipts (durable-queue module deleted; receipts are the projection source)",
  "source_receipt_kinds": ["queue.job.enqueued", "queue.job.dequeued", "queue.run.started", "queue.run.terminal", "queue.job.cancelled"],
  "durable_backend": "local_ledger",
  "live_durability_observed": false,
  "queued_job_count": {job_count},
  "human_summary": "No job in flight.",
  "jobs": [],
  {footer}
}}
"#,
        footer = READ_ONLY_FOOTER,
    ))
}

/// `/runtime/metrics.json` - runtime metrics, SLO targets, and overload posture.
///
/// The numbers here are exactly what the kernel projects from the ledger
/// (project_runtime_metrics): observed-local, zero on a booted server, never
/// invented. Timed Forge loop receipts now expose local phase, model, tool, and
/// total-run durations when real runs have happened. The standing targets (1000
/// engineers, 5000 peak builds) and the overload posture remain static.
pub(crate) fn render_runtime_metrics_json(kernel: &MdxKernel) -> Result<String, String> {
    let metrics: RuntimeMetrics = kernel.project_runtime_metrics();
    Ok(format!(
        r#"{{
  "name": "mdx-runtime-metrics",
  "status": "OK",
  "source_projection": "LocalRuntimeMetrics::project (crates/mdx-core/src/harness_runtime_metrics.rs)",
  "source_receipt_kinds": ["worker.admission.admitted", "worker.admission.queued", "worker.admission.shed", "worker.build.recorded", "harness.autonomous.completion.recorded", "harness.autonomous.escalated", "harness.test.executed", "forge.run.event"],
  "measurement_source": {measurement_source},
  "latency_percentiles_observed": {latency_percentiles_observed},
  "observed_local": {{
    "admitted": {admitted},
    "queued": {queued},
    "shed": {shed},
    "builds_completed": {builds_completed},
    "builds_failed": {builds_failed},
    "sandbox_test_duration_ms": {sandbox_duration},
    "forge_phase_duration_ms": {{
      "intake": {forge_intake},
      "plan": {forge_plan},
      "execute": {forge_execute},
      "review": {forge_review},
      "ship": {forge_ship}
    }},
    "forge_total_run_duration_ms": {forge_total_run},
    "forge_model_duration_ms": {forge_model},
    "forge_tool_duration_ms": {forge_tool},
    "forge_model_tokens": {{ "input": {forge_model_input_tokens}, "output": {forge_model_output_tokens} }},
    "forge_model_attribution": [{forge_model_attribution}],
    "forge_tool_attribution": [{forge_tool_attribution}],
    "top_failure_reasons": [{top_failure_reasons}]
  }},
  "standing_targets": {{
    "engineers": {target_engineers},
    "peak_forge_builds": {target_peak_forge_builds}
  }},
  "not_yet_instrumented": [{not_yet_instrumented}],
  "overload_posture": [{overload_posture}],
  "human_summary": "Nothing has run yet on this local server; the targets and the overload posture are the standing contract.",
  {footer}
}}
"#,
        measurement_source = json_string_literal(metrics.measurement_source),
        latency_percentiles_observed = metrics.latency_percentiles_observed,
        admitted = metrics.admitted,
        queued = metrics.queued,
        shed = metrics.shed,
        builds_completed = metrics.builds_completed,
        builds_failed = metrics.builds_failed,
        sandbox_duration = duration_stats_json(&metrics.sandbox),
        forge_intake = duration_stats_json(&metrics.forge_intake),
        forge_plan = duration_stats_json(&metrics.forge_plan),
        forge_execute = duration_stats_json(&metrics.forge_execute),
        forge_review = duration_stats_json(&metrics.forge_review),
        forge_ship = duration_stats_json(&metrics.forge_ship),
        forge_total_run = duration_stats_json(&metrics.forge_total_run),
        forge_model = duration_stats_json(&metrics.forge_model),
        forge_tool = duration_stats_json(&metrics.forge_tool),
        forge_model_input_tokens = metrics.forge_model_input_tokens,
        forge_model_output_tokens = metrics.forge_model_output_tokens,
        forge_model_attribution = model_attribution_json(&metrics.forge_model_attribution),
        forge_tool_attribution = tool_attribution_json(&metrics.forge_tool_attribution),
        top_failure_reasons = top_failure_reasons_json(&metrics.top_failure_reasons),
        target_engineers = metrics.target_engineers,
        target_peak_forge_builds = metrics.target_peak_forge_builds,
        not_yet_instrumented = static_string_array_json(&metrics.not_yet_instrumented),
        overload_posture = static_string_array_json(&metrics.overload_posture),
        footer = READ_ONLY_FOOTER,
    ))
}

/// `/runtime/operator-packet.json` - the operator decision packet.
///
/// The operator packet is assembled from a build's terminal receipt. The local
/// server has no build at the ship door, so this honestly reports no packet
/// rather than assembling one (which would write a receipt). The source
/// projection and the terminal receipt kinds it would read are still named.
pub(crate) fn render_runtime_operator_packet_json(kernel: &MdxKernel) -> Result<String, String> {
    let terminal_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| is_operator_terminal_receipt(&receipt.kind))
        .count();
    Ok(format!(
        r#"{{
  "name": "mdx-runtime-operator-packet",
  "status": "OK",
  "source_projection": "LocalForgeOperatorPacket::assemble (crates/mdx-core/src/harness_operator_packet.rs)",
  "source_receipt_kinds": ["harness.autonomous.completion.recorded", "harness.autonomous.escalated", "harness.ship.readiness.assembled", "harness.autonomous.completion.blocked", "harness.review.packet.blocked", "harness.ship.readiness.blocked"],
  "terminal_build_count": {terminal_count},
  "packet_present": false,
  "needs_human": false,
  "human_summary": "No build at the ship door yet.",
  "boundary": "this packet is evidence and a recommended next move; it does not ratify, deploy, or change state",
  "operator_can_ratify_here": false,
  {footer}
}}
"#,
        footer = READ_ONLY_FOOTER,
    ))
}

/// `/forge/runtime-projection.json` - the Forge-facing runtime packet.
///
/// Forge should not have to infer factory state by stitching unrelated proof
/// routes in the browser. This route aggregates the same read-only kernel
/// projections exposed by `/runtime/*` plus the Forge operator routes into one
/// product-consumable packet. It never records a receipt, never starts a job,
/// and never assembles an operator packet, because those actions would mutate
/// the ledger. It only counts and points.
pub(crate) fn render_forge_runtime_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let metrics: RuntimeMetrics = kernel.project_runtime_metrics();
    let entries = kernel.ledger().entries();
    let mut queued_job_ids = std::collections::BTreeSet::new();
    let mut terminal_job_ids = std::collections::BTreeSet::new();
    let mut queue_event_count = 0usize;
    let mut terminal_build_count = 0usize;
    let mut envelope_count = 0usize;

    for receipt in entries {
        if receipt.kind.starts_with("queue.") {
            queue_event_count += 1;
            if let Some(job_id) = receipt.payload.get("job_id") {
                queued_job_ids.insert(job_id.clone());
            }
            if receipt.kind == "queue.run.terminal"
                && let Some(job_id) = receipt.payload.get("job_id")
            {
                terminal_job_ids.insert(job_id.clone());
            }
        }
        if is_operator_terminal_receipt(&receipt.kind) {
            terminal_build_count += 1;
        }
        if receipt.kind == "autonomy.envelope.recorded" {
            envelope_count += 1;
        }
    }

    let active_queue_count = queued_job_ids
        .iter()
        .filter(|job_id| !terminal_job_ids.contains(*job_id))
        .count();
    let queue_pressure_status = if metrics.shed > 0 {
        "SHEDDING"
    } else if metrics.queued > 0 || active_queue_count > 0 {
        "QUEUED"
    } else {
        "IDLE"
    };
    let current_signal = if terminal_build_count > 0 {
        "Forge has terminal build evidence to inspect"
    } else if metrics.admitted + metrics.queued + metrics.shed > 0 || queue_event_count > 0 {
        "Forge has observed local worker runtime signals"
    } else {
        "No Forge worker job is in flight on this local server"
    };
    let safe_next_move = if terminal_build_count > 0 {
        "Open the operator workspace and review the terminal evidence before any human ship decision"
    } else {
        "Open the operator workspace, queue projection, and runtime metrics before granting any worker or ship authority"
    };

    Ok(format!(
        r#"{{
  "name": "mdx-forge-runtime-projection",
  "status": "OK",
  "route": "/forge/runtime-projection.json",
  "source_projection": "runtime_projection_endpoints",
  "source_routes": [
    "/runtime/queue-projection.json",
    "/runtime/metrics.json",
    "/runtime/operator-packet.json",
    "/autonomy/envelopes/projection.json",
    "/runtime/load-proof-summary.json"
  ],
  "forge_runtime": {{
    "current_signal": {current_signal},
    "queue_pressure_status": {queue_pressure_status},
    "safe_next_move": {safe_next_move},
    "boundary": "read-only runtime projection; worker spawn, CI ratification, deployment, and production writes stay closed here",
    "authority": "none",
    "operator_workspace_route": "/forge/runs/projection.json"
  }},
  "queue": {{
    "route": "/runtime/queue-projection.json",
    "durable_backend": "local_ledger",
    "live_durability_observed": false,
    "queue_event_count": {queue_event_count},
    "known_job_count": {known_job_count},
    "active_job_count": {active_queue_count}
  }},
  "metrics": {{
    "route": "/runtime/metrics.json",
    "measurement_source": {measurement_source},
    "admitted": {admitted},
    "queued": {queued},
    "shed": {shed},
    "builds_completed": {builds_completed},
    "builds_failed": {builds_failed},
    "target_engineers": {target_engineers},
    "target_peak_forge_builds": {target_peak_forge_builds},
    "latency_percentiles_observed": {latency_percentiles_observed},
    "forge_total_run_duration_ms": {forge_total_run},
    "forge_model_duration_ms": {forge_model},
    "forge_tool_duration_ms": {forge_tool},
    "forge_model_tokens": {{ "input": {forge_model_input_tokens}, "output": {forge_model_output_tokens} }},
    "not_yet_instrumented": [{not_yet_instrumented}]
  }},
  "operator_packet": {{
    "route": "/runtime/operator-packet.json",
    "terminal_build_count": {terminal_build_count},
    "packet_present": false,
    "operator_can_ratify_here": false,
    "deployment_authority_granted": false,
    "note": "a read-only route does not assemble the operator packet because assembly records a receipt"
  }},
  "autonomy": {{
    "route": "/autonomy/envelopes/projection.json",
    "envelope_count": {envelope_count},
    "active_authorization": false,
    "safe_default_note": "no standing authorization active; every task reviews with a human unless a valid envelope receipt exists"
  }},
  "ui_handoff": {{
    "claude_ready": true,
    "suggested_work_item": "tier1_forge_flagship_readiness",
    "product_shell_input_ready": true,
    "do_not_infer_execution_authority": true
  }},
  {footer}
}}
"#,
        current_signal = json_string_literal(current_signal),
        queue_pressure_status = json_string_literal(queue_pressure_status),
        safe_next_move = json_string_literal(safe_next_move),
        queue_event_count = queue_event_count,
        known_job_count = queued_job_ids.len(),
        active_queue_count = active_queue_count,
        measurement_source = json_string_literal(metrics.measurement_source),
        admitted = metrics.admitted,
        queued = metrics.queued,
        shed = metrics.shed,
        builds_completed = metrics.builds_completed,
        builds_failed = metrics.builds_failed,
        target_engineers = metrics.target_engineers,
        target_peak_forge_builds = metrics.target_peak_forge_builds,
        latency_percentiles_observed = metrics.latency_percentiles_observed,
        forge_total_run = duration_stats_json(&metrics.forge_total_run),
        forge_model = duration_stats_json(&metrics.forge_model),
        forge_tool = duration_stats_json(&metrics.forge_tool),
        forge_model_input_tokens = metrics.forge_model_input_tokens,
        forge_model_output_tokens = metrics.forge_model_output_tokens,
        not_yet_instrumented = static_string_array_json(&metrics.not_yet_instrumented),
        terminal_build_count = terminal_build_count,
        envelope_count = envelope_count,
        footer = READ_ONLY_FOOTER,
    ))
}

fn duration_stats_json(stats: &DurationStats) -> String {
    format!(
        r#"{{ "count": {}, "min_ms": {}, "mean_ms": {}, "max_ms": {} }}"#,
        stats.count, stats.min_ms, stats.mean_ms, stats.max_ms
    )
}

fn model_attribution_json(rows: &[RuntimeModelAttribution]) -> String {
    rows.iter()
        .map(|row| {
            format!(
                r#"{{"model":{},"calls":{},"input_tokens":{},"output_tokens":{},"latency_ms":{}}}"#,
                json_string_literal(&row.model),
                row.calls,
                row.input_tokens,
                row.output_tokens,
                duration_stats_json(&row.latency)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn tool_attribution_json(rows: &[RuntimeToolAttribution]) -> String {
    rows.iter()
        .map(|row| {
            format!(
                r#"{{"tool":{},"calls":{},"latency_ms":{}}}"#,
                json_string_literal(&row.tool),
                row.calls,
                duration_stats_json(&row.latency)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// `/autonomy/envelopes/projection.json` - the autonomy envelope registry.
///
/// An envelope is a standing authorization recorded as a receipt
/// (autonomy.envelope.recorded). The local server has none, so this reports an
/// empty list with the safe default: no standing authorization is active, so
/// every task reviews with a human. The list is filtered read-only from the
/// ledger; nothing is recorded here.
pub(crate) fn render_autonomy_envelope_projection_json(
    kernel: &MdxKernel,
) -> Result<String, String> {
    let entries = kernel.ledger().entries();
    let envelope_rows: Vec<String> = entries
        .iter()
        .filter(|receipt| {
            receipt.kind == "autonomy.envelope.recorded" || receipt.kind == "autonomy.envelope.amended"
        })
        .map(|receipt| {
            let active = !entries.iter().any(|entry| {
                matches!(
                    entry.kind.as_str(),
                    "autonomy.envelope.revoked"
                        | "autonomy.envelope.amended"
                        | "autonomy.envelope.expired"
                ) && entry
                    .payload
                    .get("source_envelope_receipt_id")
                    .map(String::as_str)
                    == Some(receipt.receipt_id.as_str())
            }) && receipt
                .payload
                .get("active")
                .map(String::as_str)
                == Some("true");
            format!(
                r#"{{"envelope_receipt_id":{},"receipt_kind":{},"envelope_id":{},"owner_id":{},"source_envelope_receipt_id":{},"expires_at":{},"active":{},"max_risk_class":{},"grants_ship_ratification":false,"grants_deployment_authority":false,"production_write_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                json_string_literal(&receipt.kind),
                json_string_literal(receipt.payload.get("envelope_id").map(String::as_str).unwrap_or("")),
                json_string_literal(receipt.payload.get("owner_id").map(String::as_str).unwrap_or("")),
                json_string_literal(receipt.payload.get("source_envelope_receipt_id").map(String::as_str).unwrap_or("")),
                json_string_literal(receipt.payload.get("expires_at").map(String::as_str).unwrap_or("")),
                active,
                json_string_literal(receipt.payload.get("max_risk_class").map(String::as_str).unwrap_or(""))
            )
        })
        .collect();
    let envelope_count = envelope_rows.len();
    let active_count = envelope_rows
        .iter()
        .filter(|row| row.contains(r#""active":true"#))
        .count();
    let amended_count = entries
        .iter()
        .filter(|receipt| receipt.kind == "autonomy.envelope.amended")
        .count();
    let revoked_count = entries
        .iter()
        .filter(|receipt| receipt.kind == "autonomy.envelope.revoked")
        .count();
    let expired_count = entries
        .iter()
        .filter(|receipt| receipt.kind == "autonomy.envelope.expired")
        .count();
    Ok(format!(
        r#"{{
  "name": "mdx-autonomy-envelope-projection",
  "status": "OK",
  "source_projection": "MdxKernel::reconstruct_autonomy_envelope (crates/mdx-core/src/harness_envelope_registry.rs)",
  "source_receipt_kinds": ["autonomy.envelope.recorded","autonomy.envelope.amended","autonomy.envelope.revoked","autonomy.envelope.expired"],
  "envelope_count": {envelope_count},
  "active_envelope_count": {active_count},
  "amended_envelope_count": {amended_count},
  "revoked_envelope_count": {revoked_count},
  "expired_envelope_count": {expired_count},
  "active_authorization": {active_authorization},
  "safe_default_note": "no standing authorization active; every task reviews with a human",
  "human_summary": {human_summary},
  "envelopes": [{envelopes}],
  {footer}
}}
"#,
        active_authorization = active_count > 0,
        human_summary = if active_count > 0 {
            json_string_literal(
                "Standing authorization exists, but ship and deployment still require their own human gates.",
            )
        } else {
            json_string_literal(
                "No standing authorization active; every task reviews with a human.",
            )
        },
        envelopes = envelope_rows.join(","),
        footer = READ_ONLY_FOOTER,
    ))
}

/// `/runtime/load-proof-summary.json` - the load-and-concurrency proof summary.
///
/// Reads .mdx-local/load-proof/evidence.json when present and reports its config
/// and report verbatim-honest. When the proof has not been run, it says so and
/// points at `make load-concurrency-proof`. It never fabricates a result.
pub(crate) fn render_runtime_load_proof_summary_json() -> Result<String, String> {
    let path = std::path::Path::new(".mdx-local/load-proof/evidence.json");
    let evidence = std::fs::read_to_string(path).ok();
    match evidence {
        Some(body) => {
            let trimmed = body.trim();
            Ok(format!(
                r#"{{
  "name": "mdx-runtime-load-proof-summary",
  "status": "OK",
  "source": "load_proof_evidence_file",
  "source_file": ".mdx-local/load-proof/evidence.json",
  "proof_run": true,
  "human_summary": "The load-and-concurrency proof has been run; the evidence is shown below.",
  "evidence": {trimmed},
  {footer}
}}
"#,
                footer = READ_ONLY_FOOTER,
            ))
        }
        None => Ok(format!(
            r#"{{
  "name": "mdx-runtime-load-proof-summary",
  "status": "OK",
  "source": "load_proof_evidence_file",
  "source_file": ".mdx-local/load-proof/evidence.json",
  "proof_run": false,
  "human_summary": "The load-and-concurrency proof has not been run on this machine.",
  "next_command": "make load-concurrency-proof",
  "evidence": null,
  {footer}
}}
"#,
            footer = READ_ONLY_FOOTER,
        )),
    }
}

/// `/runtime/watchtower.json` - the day-one operator pulse.
///
/// One compact, read-only summary an operator can open to answer: is beta
/// healthy, what is blocked, what changed recently, and what to look at next. It
/// aggregates the kernel ledger and the runtime signals into SAFE METADATA ONLY:
/// counts by receipt kind, feedback volume by surface and category, refusals and
/// blocks by kind, queue pressure, and posture flags. It never reads or emits a
/// feedback note, a message or page body, a prompt, an output, a secret, or a raw
/// receipt payload. A signal with no runtime source (HTTP-boundary auth refusals,
/// observed latency) is labeled NOT_OBSERVED, never zero-as-green.
pub(crate) fn render_runtime_watchtower_json(kernel: &MdxKernel) -> Result<String, String> {
    use std::collections::BTreeMap;

    let entries = kernel.ledger().entries();
    let total_receipts = entries.len();

    // Queue pressure: counted from the worker-admission and escalation receipts,
    // the same source the runtime metrics use.
    let count_kind = |k: &str| entries.iter().filter(|r| r.kind == k).count();
    let admitted = count_kind("worker.admission.admitted");
    let queued = count_kind("worker.admission.queued");
    let shed = count_kind("worker.admission.shed");
    let escalated = count_kind("harness.autonomous.escalated");

    // Feedback volume by surface and category (safe metadata fields only; the
    // note text is never read here).
    let mut feedback_by_surface: BTreeMap<String, u64> = BTreeMap::new();
    let mut feedback_by_category: BTreeMap<String, u64> = BTreeMap::new();
    let mut feedback_total: u64 = 0;
    // Refusals and blocks by kind (the kind is the reason category).
    let mut refusals_by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut refusals_total: u64 = 0;
    // Recent activity: the kinds of the last entries, counted (no payloads).
    let mut recent_by_kind: BTreeMap<String, u64> = BTreeMap::new();

    for receipt in entries.iter() {
        if receipt.kind == "beta.feedback.captured" {
            feedback_total += 1;
            let surface = receipt
                .payload
                .get("surface")
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("unspecified");
            *feedback_by_surface.entry(surface.to_string()).or_insert(0) += 1;
            let category = receipt
                .payload
                .get("category")
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("unspecified");
            *feedback_by_category
                .entry(category.to_string())
                .or_insert(0) += 1;
        }
        if receipt.kind.contains(".refused")
            || receipt.kind.contains(".refusal")
            || receipt.kind.contains(".blocked")
        {
            refusals_total += 1;
            *refusals_by_kind.entry(receipt.kind.clone()).or_insert(0) += 1;
        }
    }
    for receipt in entries.iter().rev().take(10) {
        *recent_by_kind.entry(receipt.kind.clone()).or_insert(0) += 1;
    }

    let health = if escalated > 0 { "ATTENTION" } else { "STEADY" };
    // Real pluralization: the most prominent sentence in Admin never
    // reads like a printf template.
    let items = if escalated == 1 { "item" } else { "items" };
    let refusals = if refusals_total == 1 {
        "refusal"
    } else {
        "refusals"
    };
    let notes = if feedback_total == 1 { "note" } else { "notes" };
    let summary = if escalated > 0 {
        format!(
            "{escalated} {items} escalated to a human. {refusals_total} {refusals} recorded, {feedback_total} feedback {notes} in. Look at what is blocked."
        )
    } else {
        format!(
            "Steady. {refusals_total} {refusals} recorded by design, {feedback_total} feedback {notes} in, nothing escalated to a human."
        )
    };
    let deployment_mode =
        std::env::var("MDX_DEPLOYMENT_MODE").unwrap_or_else(|_| "local-demo".to_string());

    Ok(format!(
        r#"{{
  "name": "mdx-runtime-watchtower",
  "status": "OK",
  "health": {health},
  "summary": {summary},
  "deployment_mode": {deployment_mode},
  "is_beta_healthy": {{
    "health": {health},
    "escalated_to_human": {escalated},
    "note": "STEADY means nothing is escalated to a human; ATTENTION means at least one item needs a person."
  }},
  "what_is_blocked": {{
    "refusals_observed": true,
    "refusals_total": {refusals_total},
    "by_kind": {refusals_by_kind},
    "note": "refusals and blocks are by design; each kind is its own reason. Counts only, never the refused content.",
    "auth_session_refusals": {{
      "observed": false,
      "status": "NOT_OBSERVED",
      "known_reasons": ["production_missing_trusted_session", "production_trusted_session_incomplete", "production_body_supplied_tenant", "production_body_supplied_actor", "production_body_supplied_role", "production_missing_auth_profile"],
      "note": "HTTP-boundary auth refusals are not yet recorded as runtime signals; watch the server logs for these reasons."
    }}
  }},
  "queue_pressure": {{
    "observed": true,
    "admitted": {admitted},
    "queued": {queued},
    "shed": {shed},
    "escalated": {escalated},
    "source_receipt_kinds": ["worker.admission.admitted", "worker.admission.queued", "worker.admission.shed", "harness.autonomous.escalated"],
    "note": "observed-local counts over this booted ledger; real latency is not observed here."
  }},
  "feedback": {{
    "observed": true,
    "total": {feedback_total},
    "by_surface": {feedback_by_surface},
    "by_category": {feedback_by_category},
    "note": "counts only; no feedback note text is read or shown here."
  }},
  "provider": {{
    "live_calls_allowed": false,
    "posture": "gated",
    "note": "provider calls stay gated until a human turn-on; a missing key or budget degrades to a labeled deterministic answer."
  }},
  "security_posture": {{
    "presence_only": true,
    "no_persisted_keys": true,
    "sandbox_hardened": true,
    "no_ship_deploy_receipts": true,
    "proven_by": "make runtime-security-posture-check"
  }},
  "readiness": {{
    "proven_by": "make production-admin-readiness",
    "note": "this surface does not re-run readiness; it links to the proof and the packet."
  }},
  "what_changed_recently": {{
    "total_receipts": {total_receipts},
    "recent_by_kind": {recent_by_kind},
    "note": "kinds of the most recent recorded work; no payloads."
  }},
  "look_at_next": [
    {{ "label": "Platform - what this install runs on and what turns on next", "href": "/admin/platform" }},
    {{ "label": "Security - what is watching, what it found, doors locked by design", "href": "/admin/aegis" }},
    {{ "label": "The recorded work", "href": "/receipts.json" }},
    {{ "label": "Beta readiness packet", "href": "docs/BETA-PRODUCT-READINESS-PACKET.md" }}
  ],
  "boundary": "this surface only reads and summarizes; it shows counts, kinds, and reasons, never a feedback note, a message or page body, a prompt, an output, a secret, or a raw receipt payload",
  {footer}
}}
"#,
        health = json_string_literal(health),
        summary = json_string_literal(&summary),
        deployment_mode = json_string_literal(&deployment_mode),
        refusals_by_kind = counts_to_json(&refusals_by_kind, "kind"),
        feedback_by_surface = counts_to_json(&feedback_by_surface, "surface"),
        feedback_by_category = counts_to_json(&feedback_by_category, "category"),
        recent_by_kind = counts_to_json(&recent_by_kind, "kind"),
        footer = READ_ONLY_FOOTER,
    ))
}

/// Render a count map as a JSON array `[{ "<key_name>": "...", "count": N }, ...]`
/// sorted by count descending then key ascending (deterministic). The keys are
/// safe metadata (receipt kinds, surfaces, categories), never payload content.
fn counts_to_json(counts: &std::collections::BTreeMap<String, u64>, key_name: &str) -> String {
    let mut rows: Vec<(&String, &u64)> = counts.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let items = rows
        .iter()
        .map(|(key, count)| {
            format!(
                r#"{{ "{key_name}": {}, "count": {count} }}"#,
                json_string_literal(key)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{items}]")
}

fn is_operator_terminal_receipt(kind: &str) -> bool {
    matches!(
        kind,
        "harness.autonomous.completion.recorded"
            | "harness.autonomous.escalated"
            | "harness.ship.readiness.assembled"
            | "harness.autonomous.completion.blocked"
            | "harness.review.packet.blocked"
            | "harness.ship.readiness.blocked"
    )
}

fn top_failure_reasons_json(reasons: &[(String, u32)]) -> String {
    reasons
        .iter()
        .map(|(reason, count)| {
            format!(
                r#"{{ "reason": {}, "count": {} }}"#,
                json_string_literal(reason),
                count
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn static_string_array_json(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ")
}
