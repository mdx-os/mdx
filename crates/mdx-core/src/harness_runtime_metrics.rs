// Slice 4 (Track 1): the runtime metrics and SLO packet.
//
// To run for many engineers the factory must be measurable. This projects the
// metrics it can OBSERVE from the ledger (admission outcomes, build outcomes,
// sandbox durations, top failure reasons)
// and states the standing SLO targets and the overload posture alongside them.
//
// Honesty first: only what the receipts actually contain is reported as
// observed, and it is labeled observed_local. Wall-clock latencies that still
// lack a receipt boundary (queue wait, worker start, CI wait, true review, true
// ship) are listed as defined-but-not-yet-instrumented rather than invented.
// p50/p95/p99 are not claimed for a local sample of this size.
use crate::*;

/// Min / mean / max over a set of observed durations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DurationStats {
    pub count: u32,
    pub min_ms: u64,
    pub mean_ms: u64,
    pub max_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeModelAttribution {
    pub model: String,
    pub calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency: DurationStats,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeToolAttribution {
    pub tool: String,
    pub calls: u32,
    pub latency: DurationStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeMetrics {
    // Admission outcomes (Slice 1).
    pub admitted: u32,
    pub queued: u32,
    pub shed: u32,
    // Build outcomes.
    pub builds_completed: u32,
    pub builds_failed: u32,
    // Autonomous disposition (Slice 5 of the hardening arc / Slice 2 here).
    // Sandbox test execution durations (the one latency the receipts carry).
    pub sandbox: DurationStats,
    // Forge loop phase durations observed from timed forge.run.event receipts.
    pub forge_intake: DurationStats,
    pub forge_plan: DurationStats,
    pub forge_execute: DurationStats,
    pub forge_review: DurationStats,
    pub forge_ship: DurationStats,
    pub forge_total_run: DurationStats,
    pub forge_model: DurationStats,
    pub forge_tool: DurationStats,
    pub forge_model_input_tokens: u64,
    pub forge_model_output_tokens: u64,
    pub forge_model_attribution: Vec<RuntimeModelAttribution>,
    pub forge_tool_attribution: Vec<RuntimeToolAttribution>,
    // Rates, as whole percents, over the dispositions that occurred.
    // The top failure / escalation reasons, most frequent first.
    pub top_failure_reasons: Vec<(String, u32)>,
    // Honesty + standing targets + posture.
    pub measurement_source: &'static str,
    pub latency_percentiles_observed: bool,
    pub target_engineers: u32,
    pub target_peak_forge_builds: u32,
    pub not_yet_instrumented: Vec<&'static str>,
    pub overload_posture: Vec<&'static str>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalRuntimeMetrics;

impl LocalRuntimeMetrics {
    pub fn project<S: StorageProvider>(&self, kernel: &MdxKernel<S>) -> RuntimeMetrics {
        kernel.project_runtime_metrics()
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn project_runtime_metrics(&self) -> RuntimeMetrics {
        let mut m = RuntimeMetrics {
            admitted: 0,
            queued: 0,
            shed: 0,
            builds_completed: 0,
            builds_failed: 0,
            sandbox: DurationStats::default(),
            forge_intake: DurationStats::default(),
            forge_plan: DurationStats::default(),
            forge_execute: DurationStats::default(),
            forge_review: DurationStats::default(),
            forge_ship: DurationStats::default(),
            forge_total_run: DurationStats::default(),
            forge_model: DurationStats::default(),
            forge_tool: DurationStats::default(),
            forge_model_input_tokens: 0,
            forge_model_output_tokens: 0,
            forge_model_attribution: Vec::new(),
            forge_tool_attribution: Vec::new(),
            top_failure_reasons: Vec::new(),
            measurement_source: "observed_local",
            latency_percentiles_observed: false,
            target_engineers: 1000,
            target_peak_forge_builds: 5000,
            not_yet_instrumented: Vec::new(),
            overload_posture: vec![
                "degrades_safely: admission queues then sheds before over-admitting",
                "fairness: per-tenant concurrency cap keeps one tenant from starving the rest",
                "reserve: high-priority work keeps reserved slots under load",
                "sheds_first: low-priority work is shed before the queue is exhausted",
                "human_gated: ratification and deployment never degrade - they stay the human edge",
            ],
        };
        let mut reasons: Vec<(String, u32)> = Vec::new();
        let tally = |reasons: &mut Vec<(String, u32)>, value: &str| {
            if value.is_empty() {
                return;
            }
            if let Some(entry) = reasons.iter_mut().find(|(name, _)| name == value) {
                entry.1 += 1;
            } else {
                reasons.push((value.to_string(), 1));
            }
        };

        for receipt in self.storage.ledger().entries() {
            let get = |k: &str| receipt.payload.get(k).map(String::as_str).unwrap_or("");
            match receipt.kind.as_str() {
                "worker.admission.admitted" => m.admitted += 1,
                "worker.admission.queued" => m.queued += 1,
                "worker.admission.shed" => {
                    m.shed += 1;
                    tally(&mut reasons, get("decision_reason"));
                }
                "worker.build.recorded" => {
                    if get("worker_status").contains("COMPLETED") {
                        m.builds_completed += 1;
                    } else {
                        m.builds_failed += 1;
                    }
                }
                "harness.test.executed" => {
                    let ms: u64 = get("duration_ms").parse().unwrap_or(0);
                    observe_duration(&mut m.sandbox, ms);
                }
                "forge.run.event" => {
                    let event = get("event");
                    let detail = get("detail");
                    let duration = get("duration_ms").parse::<u64>().ok();
                    if event == "model_called" {
                        let tokens_in = get("tokens_in").parse::<u64>().unwrap_or(0);
                        let tokens_out = get("tokens_out").parse::<u64>().unwrap_or(0);
                        m.forge_model_input_tokens += tokens_in;
                        m.forge_model_output_tokens += tokens_out;
                        let model = detail_field(detail, "model=")
                            .unwrap_or("unknown")
                            .to_string();
                        let entry = model_entry(&mut m.forge_model_attribution, &model);
                        entry.calls += 1;
                        entry.input_tokens += tokens_in;
                        entry.output_tokens += tokens_out;
                        if let Some(ms) = duration {
                            observe_duration(&mut m.forge_model, ms);
                            observe_duration(&mut m.forge_plan, ms);
                            observe_duration(&mut entry.latency, ms);
                        }
                    } else if matches!(event, "tool_executed" | "check_passed" | "check_failed") {
                        let tool = tool_name(event, detail);
                        let entry = tool_entry(&mut m.forge_tool_attribution, tool);
                        entry.calls += 1;
                        if let Some(ms) = duration {
                            observe_duration(&mut m.forge_tool, ms);
                            observe_duration(&mut m.forge_execute, ms);
                            observe_duration(&mut entry.latency, ms);
                        }
                    } else if event == "evidence_appended" && detail.contains("phase=intake") {
                        if let Some(ms) = duration {
                            observe_duration(&mut m.forge_intake, ms);
                        }
                    } else if event == "evidence_appended" && detail.contains("phase=review") {
                        if let Some(ms) = duration {
                            observe_duration(&mut m.forge_review, ms);
                        }
                    } else if event == "evidence_appended" && detail.contains("phase=ship") {
                        if let Some(ms) = duration {
                            observe_duration(&mut m.forge_ship, ms);
                        }
                    } else if event == "run_finished"
                        && let Some(ms) = duration
                    {
                        observe_duration(&mut m.forge_total_run, ms);
                    }
                }
                _ => {}
            }
        }
        finish_duration(&mut m.sandbox);
        finish_duration(&mut m.forge_intake);
        finish_duration(&mut m.forge_plan);
        finish_duration(&mut m.forge_execute);
        finish_duration(&mut m.forge_review);
        finish_duration(&mut m.forge_ship);
        finish_duration(&mut m.forge_total_run);
        finish_duration(&mut m.forge_model);
        finish_duration(&mut m.forge_tool);
        for entry in &mut m.forge_model_attribution {
            finish_duration(&mut entry.latency);
        }
        for entry in &mut m.forge_tool_attribution {
            finish_duration(&mut entry.latency);
        }
        // Top reasons first, then alphabetical for a stable order.
        reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        m.top_failure_reasons = reasons;
        m.forge_model_attribution
            .sort_by(|a, b| a.model.cmp(&b.model));
        m.forge_tool_attribution.sort_by(|a, b| a.tool.cmp(&b.tool));
        m.not_yet_instrumented = runtime_gaps(&m);
        m
    }
}

fn observe_duration(stats: &mut DurationStats, ms: u64) {
    stats.count += 1;
    if stats.count == 1 || ms < stats.min_ms {
        stats.min_ms = ms;
    }
    if ms > stats.max_ms {
        stats.max_ms = ms;
    }
    stats.mean_ms = stats.mean_ms.saturating_add(ms);
}

fn finish_duration(stats: &mut DurationStats) {
    if stats.count > 0 {
        stats.mean_ms /= stats.count as u64;
    }
}

fn model_entry<'a>(
    entries: &'a mut Vec<RuntimeModelAttribution>,
    model: &str,
) -> &'a mut RuntimeModelAttribution {
    if let Some(index) = entries.iter().position(|entry| entry.model == model) {
        return &mut entries[index];
    }
    entries.push(RuntimeModelAttribution {
        model: model.to_string(),
        ..RuntimeModelAttribution::default()
    });
    entries.last_mut().expect("entry just pushed")
}

fn tool_entry<'a>(
    entries: &'a mut Vec<RuntimeToolAttribution>,
    tool: &str,
) -> &'a mut RuntimeToolAttribution {
    if let Some(index) = entries.iter().position(|entry| entry.tool == tool) {
        return &mut entries[index];
    }
    entries.push(RuntimeToolAttribution {
        tool: tool.to_string(),
        ..RuntimeToolAttribution::default()
    });
    entries.last_mut().expect("entry just pushed")
}

fn detail_field<'a>(detail: &'a str, key: &str) -> Option<&'a str> {
    let rest = detail.split_once(key)?.1;
    rest.split_whitespace().next()
}

fn tool_name<'a>(event: &str, detail: &'a str) -> &'a str {
    if matches!(event, "check_passed" | "check_failed") {
        return "run_command";
    }
    detail.split_whitespace().next().unwrap_or("unknown")
}

fn runtime_gaps(m: &RuntimeMetrics) -> Vec<&'static str> {
    let mut gaps = vec![
        "queue_wait_ms",
        "worker_start_latency_ms",
        "ci_wait_ms",
        "cost_budget_use",
    ];
    if m.forge_model.count == 0 {
        gaps.push("model_latency_ms");
    }
    if m.forge_total_run.count == 0 {
        gaps.push("total_run_time_ms");
    }
    if m.forge_review.count == 0 {
        gaps.push("review_phase_latency_ms");
    }
    if m.forge_ship.count == 0 {
        gaps.push("ship_phase_latency_ms");
    }
    gaps
}
