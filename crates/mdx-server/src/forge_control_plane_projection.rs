use mdx_core::{MdxKernel, json_string_literal};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn render_forge_control_plane_projection_json(
    kernel: &MdxKernel,
) -> Result<String, String> {
    let plans = fold_plans(kernel);
    let runs = fold_fleet_runs(kernel, &plans);
    let forge_runs = fold_forge_runs(kernel);
    let shipped_runs = shipped_runs(kernel);
    let capacity = crate::fleet_executor::pool_status();
    let needs_you = needs_you_items(&plans, &runs, &forge_runs, &shipped_runs);
    let needs_you_json = needs_you.iter().map(needs_you_json).collect::<Vec<_>>();
    let pipeline = pipeline_counts(&plans, &runs, &forge_runs, &shipped_runs);
    let lane_results = lane_results_json(&runs);
    let fleet_results = fleet_results_json(&runs);
    Ok(format!(
        r#"{{"name":"mdx-forge-control-plane-local-projection","status":"OK","route":"/forge/control-plane/projection.json","receipt_backed":true,"read_only":true,"writes_allowed":false,"production_write_allowed":false,"authority_granted":false,"needs_you_count":{},"needs_you":[{}],"pipeline_counts":{},"capacity_pulse":{{"worker_capacity":{},"active_now":{},"peak_active":{},"queue_depth":{},"queue_depth_limit":{},"submitted_total":{},"completed_total":{},"backpressure_policy":"reserve_workers_then_queue_then_refuse_overflow","production_write_allowed":false}},"fleet_results":[{}],"lane_results":[{}],"source_routes":["/forge/fleet-plans/projection.json","/forge/fleet-runs/projection.json","/forge/runs/projection.json","/forge/fleet-capacity.json"],"operator_boundary":"This projection only folds existing receipts and live capacity facts for the Forge control plane. It does not approve plans, start workers, mark checks passed, ship, deliver, or grant execution authority."}}"#,
        needs_you.len(),
        needs_you_json.join(","),
        pipeline,
        capacity.capacity,
        capacity.active,
        capacity.peak_active,
        capacity.queue_depth,
        capacity.queue_depth_limit,
        capacity.submitted,
        capacity.completed,
        fleet_results,
        lane_results,
    ))
}

#[derive(Clone, Debug, Default)]
struct PlanFact {
    plan_receipt_id: String,
    status: String,
    stream_count: usize,
    requested_width: u32,
    wide_review_required: bool,
    wide_review_status: String,
    wide_review_verdict: String,
    wide_review_confidence: String,
    wide_review_concerns: String,
    streams: BTreeMap<String, StreamFact>,
}

#[derive(Clone, Debug, Default)]
struct StreamFact {
    objective: String,
    builder_slot: String,
    data_sensitivity: String,
    check_count: usize,
}

#[derive(Clone, Debug, Default)]
struct FleetRunFact {
    fleet_id: String,
    started: bool,
    finished: bool,
    final_detail: String,
    integration_state: String,
    integration_detail: String,
    review_verdict: String,
    lane_repair_started_count: u32,
    lane_repair_finished_count: u32,
    lane_repair_target_count: u32,
    lane_repair_repaired_count: u32,
    lane_repair_detail: String,
    latest_receipt_id: String,
    lanes: BTreeMap<String, LaneFact>,
}

#[derive(Clone, Debug, Default)]
struct LaneFact {
    stream_id: String,
    objective: String,
    state: String,
    forge_run_id: String,
    detail: String,
    builder_slot: String,
    data_sensitivity: String,
    check_count: usize,
    checks_passed: u32,
    checks_failed: u32,
    turns: u32,
    tokens_in: u64,
    tokens_out: u64,
}

#[derive(Clone, Debug, Default)]
struct ForgeRunFact {
    status: String,
    detail: String,
    checks_passed: u32,
    checks_failed: u32,
    turns: u32,
    tokens_in: u64,
    tokens_out: u64,
    finish_receipt_id: String,
    latest_check_receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForgeNeedsYouItem {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) line: String,
    pub(crate) href: String,
    pub(crate) subject: String,
    pub(crate) subject_type: String,
    pub(crate) subject_id: String,
    pub(crate) priority: u8,
    pub(crate) action_state: String,
    pub(crate) blocker_codes: Vec<String>,
    pub(crate) evidence_receipt_ids: Vec<String>,
}

pub(crate) fn forge_needs_you_items(kernel: &MdxKernel) -> Vec<ForgeNeedsYouItem> {
    let plans = fold_plans(kernel);
    let runs = fold_fleet_runs(kernel, &plans);
    let forge_runs = fold_forge_runs(kernel);
    let shipped_runs = shipped_runs(kernel);
    needs_you_items(&plans, &runs, &forge_runs, &shipped_runs)
}

fn fold_plans(kernel: &MdxKernel) -> BTreeMap<String, PlanFact> {
    let mut plans = BTreeMap::<String, PlanFact>::new();
    for receipt in kernel.ledger().query().by_kind("fleet.plan.drafted").iter() {
        let fleet_id = payload(receipt, "fleet_id");
        if fleet_id.is_empty() {
            continue;
        }
        let streams = mdx_core::fleet_plan_streams(&receipt.payload)
            .into_iter()
            .map(|stream| {
                (
                    stream.stream_id.clone(),
                    StreamFact {
                        objective: stream.objective,
                        builder_slot: stream.builder_slot,
                        data_sensitivity: if stream.data_sensitivity.trim().is_empty() {
                            "internal".to_string()
                        } else {
                            stream.data_sensitivity
                        },
                        check_count: stream
                            .checks
                            .iter()
                            .filter(|check| !check.trim().is_empty())
                            .count(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let requested_width = receipt
            .payload
            .get("requested_width")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(streams.len() as u32)
            .max(1);
        plans.insert(
            fleet_id,
            PlanFact {
                plan_receipt_id: receipt.receipt_id.clone(),
                status: "drafted".to_string(),
                stream_count: streams.len(),
                requested_width,
                wide_review_required: payload(receipt, "wide_plan_review_required") == "true",
                wide_review_status: payload(receipt, "wide_plan_review_status"),
                wide_review_verdict: payload(receipt, "wide_plan_review_verdict"),
                wide_review_confidence: payload(receipt, "wide_plan_review_confidence"),
                wide_review_concerns: payload(receipt, "wide_plan_review_concerns"),
                streams,
            },
        );
    }
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("fleet.plan.ratified")
        .iter()
    {
        if let Some(plan) = plans.get_mut(&payload(receipt, "fleet_id")) {
            plan.status = "ratified".to_string();
        }
    }
    plans
}

fn fold_fleet_runs(
    kernel: &MdxKernel,
    plans: &BTreeMap<String, PlanFact>,
) -> BTreeMap<String, FleetRunFact> {
    let mut runs = BTreeMap::<String, FleetRunFact>::new();
    for receipt in kernel.ledger().query().by_kind("fleet.run.event").iter() {
        let fleet_id = payload(receipt, "fleet_id");
        if fleet_id.is_empty() {
            continue;
        }
        let event = payload(receipt, "event");
        let stream_id = payload(receipt, "stream_id");
        let detail = payload(receipt, "detail");
        let run = runs
            .entry(fleet_id.clone())
            .or_insert_with(|| FleetRunFact {
                fleet_id: fleet_id.clone(),
                ..Default::default()
            });
        run.latest_receipt_id = receipt.receipt_id.clone();
        match event.as_str() {
            "run_started" => run.started = true,
            "run_finished" => {
                run.finished = true;
                run.final_detail = detail;
            }
            "integration_started" => {
                run.integration_state = "working".to_string();
                run.integration_detail = detail;
            }
            "integration_finished" => {
                run.integration_state = if detail.starts_with("no branch") {
                    "did_not_land".to_string()
                } else if detail.starts_with("skipped:") {
                    "skipped".to_string()
                } else if detail.contains("wired=false") {
                    "needs_attention".to_string()
                } else {
                    "done".to_string()
                };
                run.integration_detail = detail;
            }
            "review_finished" => run.review_verdict = detail,
            "lane_repair_started" => {
                run.lane_repair_started_count += 1;
                run.lane_repair_target_count = detail_field(&detail, "targets")
                    .parse()
                    .unwrap_or(run.lane_repair_target_count);
                run.lane_repair_detail = detail;
            }
            "lane_repair_finished" => {
                run.lane_repair_finished_count += 1;
                run.lane_repair_target_count = detail_field(&detail, "targets")
                    .parse()
                    .unwrap_or(run.lane_repair_target_count);
                run.lane_repair_repaired_count = detail_field(&detail, "repaired")
                    .parse()
                    .unwrap_or(run.lane_repair_repaired_count);
                run.lane_repair_detail = detail;
            }
            "stream_started" | "stream_finished" | "stream_needs_attention" => {
                let state = match event.as_str() {
                    "stream_started" => "working",
                    "stream_finished" => "done",
                    _ => "needs_attention",
                };
                let stream = plans
                    .get(&fleet_id)
                    .and_then(|plan| plan.streams.get(&stream_id))
                    .cloned()
                    .unwrap_or_default();
                let lane = run
                    .lanes
                    .entry(stream_id.clone())
                    .or_insert_with(|| LaneFact {
                        stream_id: stream_id.clone(),
                        objective: stream.objective.clone(),
                        builder_slot: stream.builder_slot.clone(),
                        data_sensitivity: stream.data_sensitivity.clone(),
                        check_count: stream.check_count,
                        ..Default::default()
                    });
                lane.state = state.to_string();
                lane.detail = detail;
                let forge_run_id = payload(receipt, "forge_run_id");
                if !forge_run_id.is_empty() {
                    lane.forge_run_id = forge_run_id;
                }
            }
            _ => {}
        }
    }
    let forge_runs = fold_forge_runs(kernel);
    for run in runs.values_mut() {
        for lane in run.lanes.values_mut() {
            if let Some(forge_run) = forge_runs.get(&lane.forge_run_id) {
                lane.checks_passed = forge_run.checks_passed;
                lane.checks_failed = forge_run.checks_failed;
                lane.turns = forge_run.turns;
                lane.tokens_in = forge_run.tokens_in;
                lane.tokens_out = forge_run.tokens_out;
            }
        }
    }
    runs
}

fn fold_forge_runs(kernel: &MdxKernel) -> BTreeMap<String, ForgeRunFact> {
    let mut runs = BTreeMap::<String, ForgeRunFact>::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        let run_id = payload(receipt, "run_id");
        if run_id.is_empty() {
            continue;
        }
        let run = runs.entry(run_id).or_default();
        let event = payload(receipt, "event");
        match event.as_str() {
            "run_started" if run.status.is_empty() => run.status = "running".to_string(),
            "check_passed" => {
                run.checks_passed += 1;
                run.latest_check_receipt_id = receipt.receipt_id.clone();
            }
            "check_failed" => {
                run.checks_failed += 1;
                run.latest_check_receipt_id = receipt.receipt_id.clone();
            }
            "turn_executed" | "model_called" | "tool_executed" => {
                run.turns = run.turns.max(payload(receipt, "turn").parse().unwrap_or(0));
            }
            "run_finished" => {
                run.finish_receipt_id = receipt.receipt_id.clone();
                run.detail = payload(receipt, "detail");
                run.status = if run.detail.contains("status=done")
                    || run.detail.contains("RUN_FINISHED_DONE")
                {
                    "done".to_string()
                } else if run.detail.contains("status=cannot_proceed") {
                    "cannot_proceed".to_string()
                } else if run.detail.contains("status=budget_exhausted") {
                    "budget_exhausted".to_string()
                } else if run.detail.contains("status=stopped") {
                    "stopped".to_string()
                } else if run.detail.contains("status=errored") {
                    "errored".to_string()
                } else {
                    "finished".to_string()
                };
            }
            _ => {}
        }
        run.tokens_in += payload(receipt, "tokens_in").parse::<u64>().unwrap_or(0);
        run.tokens_out += payload(receipt, "tokens_out").parse::<u64>().unwrap_or(0);
    }
    runs
}

fn shipped_runs(kernel: &MdxKernel) -> BTreeSet<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.ship.decided")
        .iter()
        .filter_map(|receipt| receipt.payload.get("run_id").cloned())
        .collect()
}

fn needs_you_items(
    plans: &BTreeMap<String, PlanFact>,
    fleet_runs: &BTreeMap<String, FleetRunFact>,
    forge_runs: &BTreeMap<String, ForgeRunFact>,
    shipped_runs: &BTreeSet<String>,
) -> Vec<ForgeNeedsYouItem> {
    let mut items = Vec::new();
    for (fleet_id, plan) in plans {
        let wide_review_missing = plan.wide_review_required
            && (plan.wide_review_status != "recorded" || plan.wide_review_verdict != "ready");
        if wide_review_missing {
            items.push(needs_you_item(NeedsYouItemSpec {
                kind: "wide_plan_review",
                label: "Review fleet plan",
                line: "Plan review needs attention before workers scale out.",
                href: "/forge/fleets",
                subject_type: "fleet",
                subject_id: fleet_id,
                priority: 20,
                action_state: "review_required",
                blocker_codes: &[],
                evidence_receipt_ids: &[&plan.plan_receipt_id],
                subject: &format!(
                    "fleet_id={fleet_id} width={} streams={} confidence={} receipt={} concerns={}",
                    plan.requested_width,
                    plan.stream_count,
                    plan.wide_review_confidence,
                    plan.plan_receipt_id,
                    plan.wide_review_concerns
                ),
            }));
        }
        if plan.status != "ratified" {
            let blocker_codes = if wide_review_missing {
                vec!["missing_proof"]
            } else {
                Vec::new()
            };
            items.push(needs_you_item(NeedsYouItemSpec {
                kind: "ratify_plan",
                label: "Approve fleet split",
                line: "Forge has a plan, but workers do not start until you approve the split.",
                href: "/forge/fleets",
                subject_type: "fleet",
                subject_id: fleet_id,
                priority: 30,
                action_state: if blocker_codes.is_empty() {
                    "review_required"
                } else {
                    "blocked"
                },
                blocker_codes: &blocker_codes,
                evidence_receipt_ids: &[&plan.plan_receipt_id],
                subject: &format!(
                    "fleet_id={fleet_id} width={} streams={} receipt={}",
                    plan.requested_width, plan.stream_count, plan.plan_receipt_id
                ),
            }));
        }
    }
    for run in fleet_runs.values() {
        if run.needs_principal_attention() {
            items.push(needs_you_item(NeedsYouItemSpec {
                kind: "fleet_repair_escalation",
                label: "Unblock fleet",
                line: "A fleet used its repair budget or integration still needs attention.",
                href: "/forge/fleets",
                subject_type: "fleet",
                subject_id: &run.fleet_id,
                priority: 10,
                action_state: "review_required",
                blocker_codes: &[],
                evidence_receipt_ids: &[&run.latest_receipt_id],
                subject: &format!("fleet_id={}", run.fleet_id),
            }));
        }
    }
    for (run_id, run) in forge_runs {
        if run.status == "done" && run.checks_passed > 0 && !shipped_runs.contains(run_id) {
            items.push(needs_you_item(NeedsYouItemSpec {
                kind: "ship_call",
                label: "Review finished run",
                line: "A run has a passed check and is waiting for your ship or revision call.",
                href: "/forge/review",
                subject_type: "run",
                subject_id: run_id,
                priority: 40,
                action_state: "review_required",
                blocker_codes: &[],
                evidence_receipt_ids: &[&run.latest_check_receipt_id, &run.finish_receipt_id],
                subject: &format!("run_id={run_id}"),
            }));
        }
    }
    items.sort_by_key(|item| item.priority);
    items
}

fn pipeline_counts(
    plans: &BTreeMap<String, PlanFact>,
    fleet_runs: &BTreeMap<String, FleetRunFact>,
    forge_runs: &BTreeMap<String, ForgeRunFact>,
    shipped_runs: &BTreeSet<String>,
) -> String {
    let plan_count = plans.len();
    let plan_review_needed = plans
        .values()
        .filter(|plan| {
            plan.wide_review_required
                && (plan.wide_review_status != "recorded" || plan.wide_review_verdict != "ready")
        })
        .count();
    let approval_needed = plans
        .values()
        .filter(|plan| plan.status != "ratified")
        .count();
    let building = fleet_runs
        .values()
        .filter(|run| run.started && !run.finished)
        .count()
        + forge_runs
            .values()
            .filter(|run| run.status == "running")
            .count();
    let repairing = fleet_runs
        .values()
        .filter(|run| run.lane_repair_started_count > run.lane_repair_finished_count)
        .count();
    let needs_attention = fleet_runs
        .values()
        .filter(|run| run.needs_principal_attention())
        .count();
    let review_needed = forge_runs
        .iter()
        .filter(|(run_id, run)| {
            run.status == "done" && run.checks_passed > 0 && !shipped_runs.contains(*run_id)
        })
        .count();
    format!(
        r#"{{"plan_count":{},"plan_review_needed":{},"approval_needed":{},"building":{},"repairing":{},"needs_attention":{},"review_needed":{},"ship_needed":{},"source":"receipt_fold","grants_execution_authority":false}}"#,
        plan_count,
        plan_review_needed,
        approval_needed,
        building,
        repairing,
        needs_attention,
        review_needed,
        review_needed,
    )
}

fn fleet_results_json(runs: &BTreeMap<String, FleetRunFact>) -> String {
    runs.values()
        .map(|run| {
            let lane_total = run.lanes.len();
            let lanes_clean = run
                .lanes
                .values()
                .filter(|lane| lane.state == "done")
                .count();
            let lanes_needing_attention = run
                .lanes
                .values()
                .filter(|lane| lane.state == "needs_attention")
                .count();
            let checks_passed: u32 = run.lanes.values().map(|lane| lane.checks_passed).sum();
            let checks_failed: u32 = run.lanes.values().map(|lane| lane.checks_failed).sum();
            let turns: u32 = run.lanes.values().map(|lane| lane.turns).sum();
            let tokens_in: u64 = run.lanes.values().map(|lane| lane.tokens_in).sum();
            let tokens_out: u64 = run.lanes.values().map(|lane| lane.tokens_out).sum();
            let status = if run.needs_principal_attention() {
                "needs_you"
            } else if run.finished && lane_total > 0 && lanes_clean == lane_total {
                "clean"
            } else if run.started && !run.finished {
                "running"
            } else if run.finished {
                "finished_with_notes"
            } else {
                "not_started"
            };
            format!(
                r#"{{"fleet_id":{},"label":"Fleet result","status":{},"lanes_clean":{},"lane_count":{},"lanes_needing_attention":{},"checks_passed":{},"checks_failed":{},"repair_attempts_started":{},"repair_attempts_finished":{},"repair_targets":{},"repair_targets_repaired":{},"integration_state":{},"review_verdict":{},"turns":{},"tokens_in":{},"tokens_out":{},"summary":{},"grants_execution_authority":false}}"#,
                json_string_literal(&run.fleet_id),
                json_string_literal(status),
                lanes_clean,
                lane_total,
                lanes_needing_attention,
                checks_passed,
                checks_failed,
                run.lane_repair_started_count,
                run.lane_repair_finished_count,
                run.lane_repair_target_count,
                run.lane_repair_repaired_count,
                json_string_literal(&run.integration_state),
                json_string_literal(&run.review_verdict),
                turns,
                tokens_in,
                tokens_out,
                json_string_literal(&format!(
                    "{lanes_clean} of {lane_total} lanes clean; {} checks passed, {} failed; {} repairs finished",
                    checks_passed, checks_failed, run.lane_repair_finished_count
                )),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn lane_results_json(runs: &BTreeMap<String, FleetRunFact>) -> String {
    runs.values()
        .flat_map(|run| {
            run.lanes.values().map(|lane| {
                // Check counters preserve the full red-to-green trail. The terminal
                // lane receipt owns current attention so a repaired lane stays clean.
                let status = if lane.state == "needs_attention" {
                    "needs_attention"
                } else if lane.state == "done" {
                    "clean"
                } else if lane.state == "working" {
                    "running"
                } else {
                    "not_started"
                };
                format!(
                    r#"{{"fleet_id":{},"stream_id":{},"label":"Lane result","status":{},"objective":{},"forge_run_id":{},"builder_slot":{},"data_sensitivity":{},"declared_check_count":{},"checks_passed":{},"checks_failed":{},"turns":{},"tokens_in":{},"tokens_out":{},"detail":{},"grants_execution_authority":false}}"#,
                    json_string_literal(&run.fleet_id),
                    json_string_literal(&lane.stream_id),
                    json_string_literal(status),
                    json_string_literal(&lane.objective),
                    json_string_literal(&lane.forge_run_id),
                    json_string_literal(if lane.builder_slot.trim().is_empty() {
                        "default"
                    } else {
                        &lane.builder_slot
                    }),
                    json_string_literal(if lane.data_sensitivity.trim().is_empty() {
                        "internal"
                    } else {
                        &lane.data_sensitivity
                    }),
                    lane.check_count,
                    lane.checks_passed,
                    lane.checks_failed,
                    lane.turns,
                    lane.tokens_in,
                    lane.tokens_out,
                    json_string_literal(&lane.detail),
                )
            })
        })
        .collect::<Vec<_>>()
        .join(",")
}

impl FleetRunFact {
    fn needs_principal_attention(&self) -> bool {
        self.lanes
            .values()
            .any(|lane| lane.state == "needs_attention")
            || self.integration_state == "needs_attention"
            || self.lane_repair_started_count > self.lane_repair_finished_count
            || (self.lane_repair_target_count > 0
                && self.lane_repair_finished_count >= 2
                && self.lane_repair_repaired_count < self.lane_repair_target_count)
    }
}

fn needs_you_json(item: &ForgeNeedsYouItem) -> String {
    let blockers = item
        .blocker_codes
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",");
    let evidence = item
        .evidence_receipt_ids
        .iter()
        .filter(|value| !value.is_empty())
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"id":{},"kind":{},"label":{},"line":{},"href":{},"subject":{},"subject_ref":{{"type":{},"id":{}}},"priority":{},"primary_action":{{"kind":{},"label":{},"method":"GET","route":{},"state":{},"blocker_codes":[{}],"evidence_receipt_ids":[{}],"grants_execution_authority":false}},"authority_granted":false}}"#,
        json_string_literal(&item.id),
        json_string_literal(&item.kind),
        json_string_literal(&item.label),
        json_string_literal(&item.line),
        json_string_literal(&item.href),
        json_string_literal(&item.subject),
        json_string_literal(&item.subject_type),
        json_string_literal(&item.subject_id),
        item.priority,
        json_string_literal(&item.kind),
        json_string_literal(&item.label),
        json_string_literal(&item.href),
        json_string_literal(&item.action_state),
        blockers,
        evidence,
    )
}

struct NeedsYouItemSpec<'a> {
    kind: &'a str,
    label: &'a str,
    line: &'a str,
    href: &'a str,
    subject_type: &'a str,
    subject_id: &'a str,
    priority: u8,
    action_state: &'a str,
    blocker_codes: &'a [&'a str],
    evidence_receipt_ids: &'a [&'a str],
    subject: &'a str,
}

fn needs_you_item(spec: NeedsYouItemSpec<'_>) -> ForgeNeedsYouItem {
    ForgeNeedsYouItem {
        id: format!("{}:{}:{}", spec.kind, spec.subject_type, spec.subject_id),
        kind: spec.kind.to_string(),
        label: spec.label.to_string(),
        line: spec.line.to_string(),
        href: spec.href.to_string(),
        subject: spec.subject.to_string(),
        subject_type: spec.subject_type.to_string(),
        subject_id: spec.subject_id.to_string(),
        priority: spec.priority,
        action_state: spec.action_state.to_string(),
        blocker_codes: spec
            .blocker_codes
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        evidence_receipt_ids: spec
            .evidence_receipt_ids
            .iter()
            .filter(|value| !value.is_empty())
            .map(|value| (*value).to_string())
            .collect(),
    }
}

fn payload(receipt: &mdx_core::Receipt, key: &str) -> String {
    receipt.payload.get(key).cloned().unwrap_or_default()
}

fn detail_field(detail: &str, key: &str) -> String {
    let needle = format!("{key}=");
    detail
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&needle))
        .unwrap_or("")
        .to_string()
}
