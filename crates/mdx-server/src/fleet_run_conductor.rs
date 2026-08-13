// The fleet conductor: fans a ratified plan out to parallel forge runs
// and witnesses the whole lifecycle as fleet.run.event receipts. It is
// the orchestrator the field's playbooks describe - it plans nothing
// (the plan is already ratified) and writes nothing (each stream's agent
// does the work); it only starts streams, caps concurrency, and folds
// outcomes.
//
// Concurrency model: the conductor no longer spawns threads itself. It
// mints each stream's run id, witnesses the linkage, and SUBMITS the
// stream to the process-wide fleet_executor pool - a bounded set of
// workers shared by EVERY fleet. So however many fleets run at once, the
// real concurrent build count is capped by the pool, and the overflow
// waits in the durable queue (backpressure, not a thread explosion). The
// conductor then drains one outcome per stream off a result channel. One
// stream dying never kills the fleet: the worker catches the panic, the
// lane is marked needs-attention, and the rest keep draining.
use crate::fleet_executor::{StreamJob, StreamOutcome};
use crate::forge_loop_runner::ForgeRunRequest;
use mdx_core::{
    FleetRunEvent, FleetStream, ForgeLongHorizonMissionCheckpoint, GovernedWriteIdentity, MdxKernel,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, RwLock};

const FLEET_LANE_REPAIR_MAX_ATTEMPTS: usize = 2;
const FLEET_PROOF_SUPPORT_REPAIR_MAX_ATTEMPTS: usize = 1;

pub(crate) struct FleetRunPlan {
    pub fleet_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub repo_root: PathBuf,
    pub repo_id: String,
    pub execution_backend: String,
    pub cloud_environment_id: String,
    pub requested_width: u32,
    pub streams: Vec<FleetStream>,
    pub integration_owned_paths: Vec<String>,
    pub full_suite_checks: Vec<String>,
    pub mission_id: String,
    pub mission_milestone_id: String,
}

impl FleetRunPlan {
    pub(crate) fn hosted_execution(&self) -> bool {
        self.execution_backend == "hosted_sandbox"
    }

    fn check_target_dir(&self, local_target: PathBuf) -> Option<String> {
        if self.hosted_execution() {
            Some(format!("hosted://{}", self.cloud_environment_id))
        } else {
            Some(local_target.to_string_lossy().to_string())
        }
    }
}

struct AttentionLane {
    stream_id: String,
    run_id: String,
    status: &'static str,
    branch: String,
}

/// Drive one fleet to completion. Blocking - the route runs this on its
/// own thread. Each stream's forge run records its own receipts; this
/// loop records the fleet-level linkage and rollup.
pub(crate) fn run_fleet(plan: FleetRunPlan, kernel: &Arc<RwLock<MdxKernel>>) {
    run_fleet_seeded(plan, Vec::new(), Vec::new(), kernel);
}

/// The body of a fleet run, with terminal outcomes that already landed
/// before this run. A fresh start seeds nothing; a RESUME after a restart
/// seeds green branches for integration and attention lanes for the rollup.
/// Already-terminal streams are not re-run, while the full ratified stream
/// list remains on the plan for integration and cold review.
pub(crate) fn run_fleet_seeded(
    plan: FleetRunPlan,
    prior_green_branches: Vec<(String, String)>,
    prior_attention_stream_ids: Vec<String>,
    kernel: &Arc<RwLock<MdxKernel>>,
) {
    let total = plan.streams.len();
    let mut done = prior_green_branches.len() + prior_attention_stream_ids.len();
    let mut green = prior_green_branches.len();
    let mut attention = prior_attention_stream_ids.len();
    let completed_stream_ids =
        seeded_completed_stream_ids(&prior_green_branches, &prior_attention_stream_ids);
    // The green streams' actual branches, collected from Forge outcomes for
    // integration. Workspace branch names may carry collision-safe suffixes,
    // so the conductor must never reconstruct them from run ids. Seeded with
    // any branches that already landed before a resume.
    let mut green_branches: Vec<(String, String)> = prior_green_branches;
    let mut attention_lanes: Vec<AttentionLane> = Vec::new();
    // Workers report each stream's terminal status here; the conductor
    // drains exactly `submitted` of them.
    let (result_tx, result_rx) = channel::<StreamOutcome>();
    let mut submitted = 0usize;
    let mut submitted_lanes: Vec<(String, String)> = Vec::new();
    let mut reported_stream_ids: HashSet<String> = HashSet::new();

    for stream in plan
        .streams
        .clone()
        .into_iter()
        .filter(|stream| !completed_stream_ids.contains(&stream.stream_id))
    {
        // Data residency, fail-closed: a stream the planner classified
        // "sensitive" may run ONLY on a sovereign coder that can keep the
        // promise - one with its OWN complete config so the work can never
        // fall back to the default frontier provider. A sovereign LABEL on an
        // unconfigured slot is refused here exactly like a frontier slot,
        // before any model sees the work; the refusal names why so residency
        // is provable on the chain. The label alone does not satisfy the
        // guarantee - the resolved coder must actually be sovereign.
        let builder_casting = {
            let Ok(kernel) = kernel.read() else {
                return;
            };
            stream_builder_casting(&kernel, &plan, &stream)
        };
        let selected_builder_slot = builder_casting.selected_slot_for_execution().to_string();
        if stream
            .data_sensitivity
            .trim()
            .eq_ignore_ascii_case("sensitive")
            && !crate::forge_turn_client::TurnClient::sovereign_slot_ready(&selected_builder_slot)
        {
            let class =
                crate::forge_turn_client::TurnClient::slot_privacy_class(&selected_builder_slot);
            let reason = if class == "sovereign" {
                "data residency refused: this slot is labeled sovereign but is not fully configured, so the work would fall back to a frontier coder - finish the sovereign slot's endpoint, key, and model".to_string()
            } else {
                format!(
                    "data residency refused: sensitive work cannot run on a {class} coder - connect a sovereign coder for this stream"
                )
            };
            record_event(
                kernel,
                &plan,
                "stream_needs_attention",
                &stream.stream_id,
                "",
                &reason,
            );
            done += 1;
            attention += 1;
            continue;
        }
        // Mint the run id and witness the linkage under one brief write
        // lock - the projection's stream_started bracket and the run
        // viewer's run_started both land before the work begins, exactly
        // as before. The heavy work then happens in the pool with no lock
        // held.
        let run_id = {
            let Ok(mut kernel) = kernel.write() else {
                return;
            };
            let run_id = kernel.mint_id("forge_run");
            let _ = kernel.record_fleet_run_event(FleetRunEvent {
                tenant_id: &plan.tenant_id,
                actor_id: "agent:fleet_conductor",
                fleet_id: &plan.fleet_id,
                event: "stream_started",
                stream_id: &stream.stream_id,
                forge_run_id: &run_id,
                detail: &format!("scope_paths={}", stream.write_scope.len()),
            });
            // The stream's forge run opens with its own run_started so the
            // run viewer and ship gate see a complete chain. Fleet streams
            // carry the same repo-intelligence and geometry evidence as a
            // direct run, so wide execution is reviewable at stream level
            // instead of only as a fleet rollup.
            let evidence_fields =
                stream_run_started_evidence_fields(&plan, &stream, &builder_casting);
            let evidence_refs: Vec<(&str, &str)> = evidence_fields
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            let work_item_id = format!("work_item_{}_{}", plan.fleet_id, stream.stream_id);
            let _ = kernel.record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: &plan.tenant_id,
                    actor_id: &plan.actor_id,
                    run_id: &run_id,
                    event: "run_started",
                    work_item_id: &work_item_id,
                    detail: &format!(
                        "fleet={} stream={} repo_root={} language_pack={} workers={}",
                        plan.fleet_id,
                        stream.stream_id,
                        plan.repo_root.display(),
                        crate::forge_repo_profile::profile_repo(&plan.repo_root).language_pack_id,
                        plan.streams.len().max(1)
                    ),
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &GovernedWriteIdentity::local_demo(&plan.actor_id),
                &evidence_refs,
            );
            run_id
        };
        let request = stream_request(&plan, &stream, &run_id, &builder_casting);
        let job = StreamJob {
            request,
            repo_root: plan.repo_root.clone(),
            kernel: Arc::clone(kernel),
            stream_id: stream.stream_id.clone(),
            report: result_tx.clone(),
        };
        match crate::fleet_executor::submit(job) {
            Ok(()) => {
                submitted += 1;
                submitted_lanes.push((stream.stream_id.clone(), run_id));
            }
            Err(_) => {
                // The durable queue is at its depth limit: refuse this
                // stream as bounded backpressure rather than start
                // unbounded work. It surfaces as needs-attention with a
                // clear reason; the operator re-runs it when capacity
                // frees.
                record_event(
                    kernel,
                    &plan,
                    "stream_needs_attention",
                    &stream.stream_id,
                    &run_id,
                    "queue at depth limit - bounded backpressure, re-run when capacity frees",
                );
                done += 1;
                attention += 1;
            }
        }
    }
    // Drop our own sender so the channel can close once every worker clone
    // is gone; we still read an exact count below.
    drop(result_tx);

    // Drain exactly one outcome per successfully-submitted stream, under
    // a fleet deadline. The old bare recv() meant one wedged provider
    // call stalled the whole fleet forever; now the conductor waits at
    // most the remaining deadline and then routes the still-running
    // lanes to attention with a receipt naming the deadline.
    let fleet_deadline_ms = fleet_deadline_ms();
    let drain_started = std::time::Instant::now();
    let mut deadline_hit = false;
    for _ in 0..submitted {
        let remaining = fleet_deadline_ms
            .saturating_sub(drain_started.elapsed().as_millis() as u64)
            .max(1);
        let outcome = match result_rx.recv_timeout(std::time::Duration::from_millis(remaining)) {
            Ok(outcome) => outcome,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                deadline_hit = true;
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        reported_stream_ids.insert(outcome.stream_id.clone());
        done += 1;
        let ok = outcome.outcome.status == "RUN_FINISHED_DONE";
        let branch = outcome.outcome.branch.clone().unwrap_or_default();
        if ok {
            green += 1;
            if !branch.is_empty() {
                green_branches.push((outcome.stream_id.clone(), branch.clone()));
            }
        } else {
            attention += 1;
            attention_lanes.push(AttentionLane {
                stream_id: outcome.stream_id.clone(),
                run_id: outcome.outcome.run_id.clone(),
                status: outcome.outcome.status,
                branch: branch.clone(),
            });
        }
        record_event(
            kernel,
            &plan,
            if ok {
                "stream_finished"
            } else {
                "stream_needs_attention"
            },
            &outcome.stream_id,
            &outcome.outcome.run_id,
            &format!(
                "status={} done={done}/{total} branch={}",
                outcome.outcome.status, branch
            ),
        );
    }

    if deadline_hit {
        // Undrained lanes are still running on worker threads; the fleet
        // stops waiting for them. Each unreported stream goes to
        // attention with the deadline named, so the operator sees exactly
        // which lanes outlived the budget instead of a silent stall.
        let unreported = unreported_submitted_lanes(&submitted_lanes, &reported_stream_ids);
        let undrained = unreported.len();
        attention += undrained;
        done += undrained;
        for (stream_id, run_id) in &unreported {
            record_event(
                kernel,
                &plan,
                "stream_needs_attention",
                stream_id,
                run_id,
                &format!(
                    "lane did not report before the {}ms fleet deadline; routed to attention; done={done}/{total}",
                    fleet_deadline_ms
                ),
            );
        }
        record_event(
            kernel,
            &plan,
            "fleet_deadline_exceeded",
            "conductor",
            "",
            &format!(
                "fleet deadline exceeded after {}ms: {} lane(s) unreported, routed to attention (MDX_FLEET_DEADLINE_MS)",
                fleet_deadline_ms, undrained
            ),
        );
    }

    if attention > 0 {
        let repaired =
            repair_attention_lanes(&plan, &attention_lanes, kernel, &mut green_branches, total);
        if repaired > 0 {
            green += repaired;
            attention = attention.saturating_sub(repaired);
        }
    }

    // The integration phase: the green streams become ONE branch, the
    // integration agent wires the shared registration files, the full
    // suite proves the whole, and a fresh-context reviewer reads the
    // integrated diff cold. The fleet still never ships - the branch
    // waits for the recorded human ship decision.
    let mut fleet_branch_note = String::new();
    let mut integration_landed = false;
    if green == total && attention == 0 && !green_branches.is_empty() {
        record_event(
            kernel,
            &plan,
            "integration_started",
            "",
            "",
            &format!("merging={} order=dependency", green_branches.len()),
        );
        let integration = crate::fleet_integration::integrate_fleet(&plan, &green_branches, kernel);
        match &integration.fleet_branch {
            Some(branch) => {
                fleet_branch_note = format!(" fleet_branch={branch}");
                integration_landed = integration.wired;
                record_event(
                    kernel,
                    &plan,
                    "integration_finished",
                    "",
                    &integration.integration_run_id,
                    &format!(
                        "branch={branch} merged={} wired={} {}",
                        integration.merged_streams, integration.wired, integration.detail
                    ),
                );
                record_event(
                    kernel,
                    &plan,
                    "review_finished",
                    "",
                    "",
                    &integration.review_verdict,
                );
                if crate::fleet_integration::review_verdict_needs_work(&integration.review_verdict)
                {
                    let mut latest_review = integration.review_verdict.clone();
                    for attempt in 1..=FLEET_LANE_REPAIR_MAX_ATTEMPTS {
                        let Some(repair) = crate::fleet_integration::repair_integrated_fleet(
                            &plan,
                            branch,
                            &latest_review,
                            attempt,
                            kernel,
                        ) else {
                            break;
                        };
                        record_event(
                            kernel,
                            &plan,
                            "review_finished",
                            "",
                            &repair.run_id,
                            &repair.review_verdict,
                        );
                        latest_review = repair.review_verdict;
                        if repair.status == "RUN_FINISHED_DONE"
                            && !crate::fleet_integration::review_verdict_needs_work(&latest_review)
                        {
                            break;
                        }
                    }
                }
            }
            None => {
                record_event(
                    kernel,
                    &plan,
                    "integration_finished",
                    "",
                    "",
                    &format!("no branch: {}", integration.detail),
                );
            }
        }
    } else if attention > 0 {
        record_event(
            kernel,
            &plan,
            "integration_finished",
            "",
            "",
            &format!("skipped: streams_need_attention green={green} needs_attention={attention}"),
        );
    }

    record_event(
        kernel,
        &plan,
        "run_finished",
        "",
        "",
        &format!("streams={total} green={green} needs_attention={attention}{fleet_branch_note}"),
    );
    record_mission_checkpoint_from_fleet_outcome(
        kernel,
        &plan,
        total,
        green,
        attention,
        integration_landed,
    );
}

fn seeded_completed_stream_ids(
    prior_green_branches: &[(String, String)],
    prior_attention_stream_ids: &[String],
) -> HashSet<String> {
    prior_green_branches
        .iter()
        .map(|(stream_id, _)| stream_id.clone())
        .chain(prior_attention_stream_ids.iter().cloned())
        .collect()
}

fn unreported_submitted_lanes(
    submitted_lanes: &[(String, String)],
    reported_stream_ids: &HashSet<String>,
) -> Vec<(String, String)> {
    submitted_lanes
        .iter()
        .filter(|(stream_id, _)| !reported_stream_ids.contains(stream_id))
        .cloned()
        .collect()
}

fn repair_attention_lanes(
    plan: &FleetRunPlan,
    attention_lanes: &[AttentionLane],
    kernel: &Arc<RwLock<MdxKernel>>,
    green_branches: &mut Vec<(String, String)>,
    total: usize,
) -> usize {
    if attention_lanes.is_empty() {
        return 0;
    }
    record_event(
        kernel,
        plan,
        "lane_repair_started",
        "",
        "",
        &format!(
            "targets={} max_attempts={}",
            attention_lanes.len(),
            FLEET_LANE_REPAIR_MAX_ATTEMPTS
        ),
    );
    let mut repaired = 0usize;
    for lane in attention_lanes {
        let Some(stream) = plan
            .streams
            .iter()
            .find(|stream| stream.stream_id == lane.stream_id)
        else {
            continue;
        };
        let fallback_branch = format!("forge/run-{}", lane.run_id);
        let prior_branch =
            if !lane.branch.is_empty() && git_branch_exists(&plan.repo_root, &lane.branch) {
                Some(lane.branch.clone())
            } else if git_branch_exists(&plan.repo_root, &fallback_branch) {
                Some(fallback_branch)
            } else {
                record_event(
                    kernel,
                    plan,
                    "stream_needs_attention",
                    &lane.stream_id,
                    &lane.run_id,
                    &format!(
                        "repair reseeded: no branch recorded for {}, retrying stream from base",
                        lane.status
                    ),
                );
                None
            };
        let mut lane_repaired = false;
        for attempt in 1..=FLEET_LANE_REPAIR_MAX_ATTEMPTS {
            let repair_run_id = {
                let Ok(mut kernel) = kernel.write() else {
                    return repaired;
                };
                let run_id = kernel.mint_id("forge_run");
                let _ = kernel.record_fleet_run_event(FleetRunEvent {
                    tenant_id: &plan.tenant_id,
                    actor_id: "agent:fleet_conductor",
                    fleet_id: &plan.fleet_id,
                    event: "stream_started",
                    stream_id: &stream.stream_id,
                    forge_run_id: &run_id,
                    detail: &format!(
                        "repair_attempt={attempt} {}",
                        prior_branch
                            .as_ref()
                            .map(|branch| format!("revising={branch}"))
                            .unwrap_or_else(|| "reseeding_from_base=true".to_string())
                    ),
                });
                let builder_casting = stream_builder_casting(&kernel, plan, stream);
                let evidence_fields = stream_run_started_evidence_fields_for_lane(
                    plan,
                    stream,
                    &builder_casting,
                    "fleet_lane_repair",
                    "repair_from_governed_fleet_runtime",
                );
                let evidence_refs: Vec<(&str, &str)> = evidence_fields
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str()))
                    .collect();
                let work_item_id = format!(
                    "work_item_{}_{}_repair_{}",
                    plan.fleet_id, stream.stream_id, attempt
                );
                let repair_basis = prior_branch
                    .as_ref()
                    .map(|branch| format!("revising={branch}"))
                    .unwrap_or_else(|| "reseeding_from_base=true".to_string());
                let _ = kernel.record_forge_run_event_with_evidence_fields(
                    mdx_core::ForgeRunEvent {
                        tenant_id: &plan.tenant_id,
                        actor_id: &plan.actor_id,
                        run_id: &run_id,
                        event: "run_started",
                        work_item_id: &work_item_id,
                        detail: &format!(
                            "fleet={} stream={} repair_attempt={} repo_root={} {}",
                            plan.fleet_id,
                            stream.stream_id,
                            attempt,
                            plan.repo_root.display(),
                            repair_basis,
                        ),
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo(&plan.actor_id),
                    &evidence_refs,
                );
                run_id
            };
            let builder_casting = {
                let Ok(kernel) = kernel.read() else {
                    return repaired;
                };
                stream_builder_casting(&kernel, plan, stream)
            };
            let mut request = stream_request(plan, stream, &repair_run_id, &builder_casting);
            request.revise_branch = prior_branch.clone();
            request.max_turns = request.max_turns.clamp(12, 30);
            request.execution_geometry_lane = "fleet_lane_repair".to_string();
            request.semantic_policy_source = "fleet_lane_repair".to_string();
            request.intent = if let Some(branch) = prior_branch.as_ref() {
                format!(
                    "Repair fleet stream {} after prior status {}. Continue on branch {}. Keep the stream write scope, run the stream proof, and finish only when the selected proof passes.\n\n{}",
                    stream.stream_id, lane.status, branch, request.intent
                )
            } else {
                format!(
                    "Repair fleet stream {} after prior status {}. The prior worker produced no branch, so retry this stream from the base repo. Inspect the scoped file and selected proof, make the smallest relevant edit, run the stream proof, and finish only when the selected proof passes. Do not call cannot_proceed unless the repo, scope, or toolchain makes the task impossible after a concrete edit or proof attempt.\n\n{}",
                    stream.stream_id, lane.status, request.intent
                )
            };
            let outcome = crate::fleet_executor::run_blocking(
                request.clone(),
                plan.repo_root.clone(),
                kernel,
                format!(
                    "{}:{}:lane-repair-{attempt}",
                    plan.fleet_id, stream.stream_id
                ),
            );
            crate::forge_outcome_signal_route::record_run_outcome_signal(
                kernel, &request, &outcome,
            );
            let ok = outcome.status == "RUN_FINISHED_DONE";
            record_event(
                kernel,
                plan,
                if ok {
                    "stream_finished"
                } else {
                    "stream_needs_attention"
                },
                &stream.stream_id,
                &repair_run_id,
                &format!(
                    "repair_attempt={attempt} status={} done={}/{}",
                    outcome.status,
                    if ok { total } else { total.saturating_sub(1) },
                    total
                ),
            );
            if ok {
                let merge_branch = request
                    .revise_branch
                    .clone()
                    .or_else(|| outcome.branch.clone())
                    .unwrap_or_else(|| format!("forge/run-{repair_run_id}"));
                green_branches.retain(|(stream_id, _)| stream_id != &stream.stream_id);
                green_branches.push((stream.stream_id.clone(), merge_branch));
                lane_repaired = true;
                repaired += 1;
                break;
            }
        }
        if !lane_repaired
            && let Some(proof_support_scope) = proof_support_repair_scope(stream, &plan.repo_root)
        {
            for attempt in 1..=FLEET_PROOF_SUPPORT_REPAIR_MAX_ATTEMPTS {
                let repair_run_id = {
                    let Ok(mut kernel) = kernel.write() else {
                        return repaired;
                    };
                    let run_id = kernel.mint_id("forge_run");
                    let _ = kernel.record_fleet_run_event(FleetRunEvent {
                            tenant_id: &plan.tenant_id,
                            actor_id: "agent:fleet_conductor",
                            fleet_id: &plan.fleet_id,
                            event: "stream_started",
                            stream_id: &stream.stream_id,
                            forge_run_id: &run_id,
                            detail: &format!(
                                "repair_attempt={} scope_expansion=proof_harness_support support_paths={} {}",
                                FLEET_LANE_REPAIR_MAX_ATTEMPTS + attempt,
                                proof_support_scope.join(","),
                                prior_branch
                                    .as_ref()
                                    .map(|branch| format!("revising={branch}"))
                                    .unwrap_or_else(|| "reseeding_from_base=true".to_string())
                            ),
                        });
                    let builder_casting = stream_builder_casting(&kernel, plan, stream);
                    let mut evidence_fields = stream_run_started_evidence_fields_for_lane(
                        plan,
                        stream,
                        &builder_casting,
                        "fleet_orchestrator_repair",
                        "proof_support_repair_from_governed_fleet_runtime",
                    );
                    evidence_fields.push((
                        "fleet_stream_expanded_write_scope".to_string(),
                        expanded_repair_scope(stream, &proof_support_scope).join("\n"),
                    ));
                    evidence_fields.push((
                        "fleet_stream_scope_expansion_reason".to_string(),
                        "proof_harness_support_repair_after_lane_repair_exhausted".to_string(),
                    ));
                    let evidence_refs: Vec<(&str, &str)> = evidence_fields
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.as_str()))
                        .collect();
                    let work_item_id = format!(
                        "work_item_{}_{}_proof_support_repair_{}",
                        plan.fleet_id, stream.stream_id, attempt
                    );
                    let repair_basis = prior_branch
                        .as_ref()
                        .map(|branch| format!("revising={branch}"))
                        .unwrap_or_else(|| "reseeding_from_base=true".to_string());
                    let _ = kernel.record_forge_run_event_with_evidence_fields(
                            mdx_core::ForgeRunEvent {
                                tenant_id: &plan.tenant_id,
                                actor_id: &plan.actor_id,
                                run_id: &run_id,
                                event: "run_started",
                                work_item_id: &work_item_id,
                                detail: &format!(
                                    "fleet={} stream={} proof_support_repair_attempt={} repo_root={} support_paths={} {}",
                                    plan.fleet_id,
                                    stream.stream_id,
                                    attempt,
                                    plan.repo_root.display(),
                                    proof_support_scope.join(","),
                                    repair_basis,
                                ),
                                turn: 0,
                                input_tokens: 0,
                                output_tokens: 0,
                            },
                            &GovernedWriteIdentity::local_demo(&plan.actor_id),
                            &evidence_refs,
                        );
                    run_id
                };
                let builder_casting = {
                    let Ok(kernel) = kernel.read() else {
                        return repaired;
                    };
                    stream_builder_casting(&kernel, plan, stream)
                };
                let mut request = stream_request(plan, stream, &repair_run_id, &builder_casting);
                let expanded_scope = expanded_repair_scope(stream, &proof_support_scope);
                request.revise_branch = prior_branch.clone();
                request.write_scope = expanded_scope.clone();
                request.max_turns = request.max_turns.clamp(12, 30);
                request.execution_geometry_lane = "fleet_orchestrator_repair".to_string();
                request.semantic_policy_source = "fleet_orchestrator_repair".to_string();
                request.intent = format!(
                    "Repair fleet stream {} after scoped lane repair exhausted with prior status {}.\n\nThis is a governed orchestrator repair, not a normal stream retry. The lane proof appears blocked by proof harness support outside the original stream scope, so the conductor has opened exactly one bounded scope expansion.\n\nYou may edit ONLY these paths:\n{}\n\nKeep product code changes focused on the original stream objective. Use the proof-support paths only to make the selected proof command runnable and honest. Run the selected stream proof and finish done only when it passes. If the selected proof still cannot pass, finish cannot_proceed and name the concrete blocker.\n\nOriginal stream request:\n{}",
                    stream.stream_id,
                    lane.status,
                    expanded_scope.join("\n"),
                    request.intent
                );
                let outcome = crate::fleet_executor::run_blocking(
                    request.clone(),
                    plan.repo_root.clone(),
                    kernel,
                    format!(
                        "{}:{}:proof-support-repair-{attempt}",
                        plan.fleet_id, stream.stream_id
                    ),
                );
                crate::forge_outcome_signal_route::record_run_outcome_signal(
                    kernel, &request, &outcome,
                );
                let ok = outcome.status == "RUN_FINISHED_DONE";
                record_event(
                    kernel,
                    plan,
                    if ok {
                        "stream_finished"
                    } else {
                        "stream_needs_attention"
                    },
                    &stream.stream_id,
                    &repair_run_id,
                    &format!(
                        "proof_support_repair_attempt={attempt} status={} support_paths={} done={}/{}",
                        outcome.status,
                        proof_support_scope.join(","),
                        if ok { total } else { total.saturating_sub(1) },
                        total
                    ),
                );
                if ok {
                    let merge_branch = request
                        .revise_branch
                        .clone()
                        .or_else(|| outcome.branch.clone())
                        .unwrap_or_else(|| format!("forge/run-{repair_run_id}"));
                    green_branches.retain(|(stream_id, _)| stream_id != &stream.stream_id);
                    green_branches.push((stream.stream_id.clone(), merge_branch));
                    lane_repaired = true;
                    repaired += 1;
                    break;
                }
            }
        }
        if !lane_repaired {
            record_event(
                kernel,
                plan,
                "stream_needs_attention",
                &stream.stream_id,
                &lane.run_id,
                &format!(
                    "repair exhausted: attempts={} prior_status={}",
                    FLEET_LANE_REPAIR_MAX_ATTEMPTS, lane.status
                ),
            );
        }
    }
    record_event(
        kernel,
        plan,
        "lane_repair_finished",
        "",
        "",
        &format!("targets={} repaired={}", attention_lanes.len(), repaired),
    );
    repaired
}

fn proof_support_repair_scope(stream: &FleetStream, repo_root: &Path) -> Option<Vec<String>> {
    if !stream.checks.iter().any(|check| {
        check
            .split_whitespace()
            .any(|part| part == "native-macos-operator-check")
    }) {
        return None;
    }
    let candidates = ["scripts/native-macos-operator-check.sh", "Makefile"];
    let mut support_scope = Vec::new();
    for candidate in candidates {
        if repo_root.join(candidate).exists()
            && !stream.write_scope.iter().any(|entry| entry == candidate)
        {
            support_scope.push(candidate.to_string());
        }
    }
    if support_scope.is_empty() {
        None
    } else {
        Some(support_scope)
    }
}

fn expanded_repair_scope(stream: &FleetStream, proof_support_scope: &[String]) -> Vec<String> {
    let mut scope = stream.write_scope.clone();
    for entry in proof_support_scope {
        if !scope.iter().any(|existing| existing == entry) {
            scope.push(entry.clone());
        }
    }
    scope
}

fn git_branch_exists(repo_root: &Path, branch: &str) -> bool {
    std::process::Command::new("git")
        .arg("rev-parse")
        .arg("--verify")
        .arg(branch)
        .current_dir(repo_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn record_mission_checkpoint_from_fleet_outcome(
    kernel: &Arc<RwLock<MdxKernel>>,
    plan: &FleetRunPlan,
    total: usize,
    green: usize,
    attention: usize,
    integration_landed: bool,
) {
    if plan.mission_id.trim().is_empty() || plan.mission_milestone_id.trim().is_empty() {
        return;
    }
    let completed = total > 0 && green == total && attention == 0 && integration_landed;
    let checkpoint_event = if completed {
        "milestone_completed"
    } else {
        "milestone_blocked"
    };
    let validation_status = if completed {
        "fleet_passed"
    } else if attention > 0 {
        "fleet_needs_attention"
    } else {
        "fleet_integration_not_landed"
    };
    let summary = format!(
        "Fleet {} ended with streams={} green={} needs_attention={} integration_landed={}",
        plan.fleet_id, total, green, attention, integration_landed
    );
    let Ok(mut kernel) = kernel.write() else {
        return;
    };
    if let Err(error) = kernel.record_forge_long_horizon_mission_checkpoint_local(
        ForgeLongHorizonMissionCheckpoint {
            tenant_id: &plan.tenant_id,
            actor_id: "agent:fleet_conductor",
            actor_role: "operator",
            mission_id: &plan.mission_id,
            milestone_id: &plan.mission_milestone_id,
            checkpoint_event,
            summary: &summary,
            validation_status,
            related_run_id: "",
            related_fleet_id: &plan.fleet_id,
            steering_note: "",
        },
    ) {
        eprintln!(
            "fleet {} mission checkpoint bridge failed: {}",
            plan.fleet_id,
            error.message()
        );
    }
}

/// Durable execution, MDx-native: on boot, re-drive any fleet that
/// started but never recorded run_finished - the kernel was killed mid
/// run. The receipt trail IS the checkpoint. We re-run only the streams
/// that never reached a terminal event (interrupted or never-started),
/// seed the integration phase with the branches that already landed
/// green, retain the full ratified stream contract for integration, and
/// close the fleet. A fleet that already recorded run_finished is left
/// alone. Spawns one conductor thread per resumed fleet and returns; it
/// never blocks boot.
///
/// Opt-in via MDX_FLEET_RESUME=1: a boot must never silently re-execute
/// build work, and one stream that reliably crashes the process can never
/// wedge the kernel in a resume loop. Disabled is a no-op.
pub(crate) fn resume_unfinished_fleets(kernel: &Arc<RwLock<MdxKernel>>) {
    if std::env::var("MDX_FLEET_RESUME").ok().as_deref() != Some("1") {
        return;
    }
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct FleetState {
        started: bool,
        finished: bool,
        tenant_id: String,
        actor_id: String,
        repo_id: String,
        repo_root: String,
        execution_backend: String,
        cloud_environment_id: String,
        requested_width: u32,
        mission_id: String,
        mission_milestone_id: String,
        // stream_id -> latest terminal kind ("done" or "attention"); absent
        // means interrupted or never started.
        terminal: BTreeMap<String, bool>,
        // stream_id -> branch of the green run, for the integration seed.
        green: BTreeMap<String, String>,
    }

    struct Resumable {
        plan: FleetRunPlan,
        prior_green: Vec<(String, String)>,
        prior_attention: Vec<String>,
        unfinished: usize,
    }

    let resumables: Vec<Resumable> = {
        let kernel = kernel.read().unwrap_or_else(|p| p.into_inner());
        let mut fleets: BTreeMap<String, FleetState> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        for receipt in kernel.ledger().query().by_kind("fleet.run.event").iter() {
            let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            let fleet_id = get("fleet_id");
            if fleet_id.is_empty() {
                continue;
            }
            if !fleets.contains_key(&fleet_id) {
                order.push(fleet_id.clone());
            }
            let state = fleets.entry(fleet_id).or_default();
            let event = get("event");
            let stream_id = get("stream_id");
            match event.as_str() {
                "run_started" => {
                    state.started = true;
                    // tenant/actor live on the receipt; repo_root in the detail.
                    state.tenant_id = receipt.tenant_id.as_str().to_string();
                    state.actor_id = receipt.actor_id.as_str().to_string();
                    state.repo_id = detail_field(&get("detail"), "repo_id");
                    if state.repo_id == "mdx" {
                        state.repo_id.clear();
                    }
                    state.repo_root = detail_field(&get("detail"), "repo_root");
                    state.execution_backend = detail_field(&get("detail"), "execution_backend");
                    state.cloud_environment_id =
                        detail_field(&get("detail"), "cloud_environment_id");
                    state.requested_width = detail_field(&get("detail"), "requested_width")
                        .parse()
                        .unwrap_or(0);
                    state.mission_id = detail_field(&get("detail"), "mission_id");
                    state.mission_milestone_id =
                        detail_field(&get("detail"), "mission_milestone_id");
                }
                "run_finished" => state.finished = true,
                "stream_finished" => {
                    state.terminal.insert(stream_id.clone(), true);
                    let run_id = get("forge_run_id");
                    if !run_id.is_empty() {
                        let branch = detail_field(&get("detail"), "branch");
                        state.green.insert(
                            stream_id,
                            if branch.is_empty() {
                                format!("forge/run-{run_id}")
                            } else {
                                branch
                            },
                        );
                    }
                }
                "stream_needs_attention" => {
                    state.terminal.insert(stream_id, false);
                }
                _ => {}
            }
        }

        let mut resumables = Vec::new();
        for fleet_id in order {
            let state = &fleets[&fleet_id];
            if !state.started || state.finished {
                continue;
            }
            if !kernel.fleet_plan_ratified(&fleet_id) {
                continue;
            }
            let Some(plan_fields) = kernel.latest_fleet_plan(&fleet_id) else {
                continue;
            };
            let all_streams = mdx_core::fleet_plan_streams(&plan_fields);
            // Re-run only streams that never reached a terminal event.
            let unfinished_count = all_streams
                .iter()
                .filter(|stream| !state.terminal.contains_key(&stream.stream_id))
                .count();
            let prior_green: Vec<(String, String)> = state
                .green
                .iter()
                .map(|(stream_id, branch)| (stream_id.clone(), branch.clone()))
                .collect();
            let prior_attention: Vec<String> = state
                .terminal
                .iter()
                .filter(|(_, green)| !**green)
                .map(|(stream_id, _)| stream_id.clone())
                .collect();
            let repo_root = if state.repo_root.is_empty() {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .to_string_lossy()
                    .to_string()
            } else {
                state.repo_root.clone()
            };
            let integration_owned_paths = lines_field(&plan_fields, "integration_owned_paths");
            let full_suite_checks = lines_field(&plan_fields, "full_suite_checks");
            let requested_width = plan_fields
                .get("requested_width")
                .and_then(|value| value.parse::<u32>().ok())
                .or_else(|| (state.requested_width > 0).then_some(state.requested_width))
                .unwrap_or(0);
            resumables.push(Resumable {
                plan: FleetRunPlan {
                    fleet_id: fleet_id.clone(),
                    tenant_id: if state.tenant_id.is_empty() {
                        "local_tenant".to_string()
                    } else {
                        state.tenant_id.clone()
                    },
                    actor_id: if state.actor_id.is_empty() {
                        "local_user".to_string()
                    } else {
                        state.actor_id.clone()
                    },
                    repo_root: PathBuf::from(repo_root),
                    repo_id: state.repo_id.clone(),
                    execution_backend: if state.execution_backend == "hosted_sandbox" {
                        "hosted_sandbox".to_string()
                    } else {
                        "local".to_string()
                    },
                    cloud_environment_id: state.cloud_environment_id.clone(),
                    requested_width: requested_width
                        .max(unfinished_count.max(prior_green.len()) as u32)
                        .max(1),
                    // Integration and cold review need the full ratified stream
                    // contract even when restart recovery only re-runs a subset.
                    // The prior terminal seeds below suppress already-finished
                    // lanes without erasing their scopes or objectives.
                    streams: all_streams,
                    integration_owned_paths,
                    full_suite_checks,
                    mission_id: state.mission_id.clone(),
                    mission_milestone_id: state.mission_milestone_id.clone(),
                },
                prior_green,
                prior_attention,
                unfinished: unfinished_count,
            });
        }
        resumables
    };

    for resumable in resumables {
        let fleet_id = resumable.plan.fleet_id.clone();
        eprintln!(
            "mdx-server resuming interrupted fleet {fleet_id}: {} stream(s) to re-run, {} already green",
            resumable.unfinished,
            resumable.prior_green.len()
        );
        // Mark the resume on the trail before re-running, so the projection
        // and an operator can see it was picked back up, not silently
        // restarted.
        record_event(
            kernel,
            &resumable.plan,
            "run_resumed",
            "",
            "",
            &format!(
                "re_running={} prior_green={}",
                resumable.unfinished,
                resumable.prior_green.len()
            ),
        );
        let kernel_for_fleet = Arc::clone(kernel);
        let _ = std::thread::Builder::new()
            .name(format!("fleet-resume-{fleet_id}"))
            .spawn(move || {
                if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_fleet_seeded(
                        resumable.plan,
                        resumable.prior_green,
                        resumable.prior_attention,
                        &kernel_for_fleet,
                    );
                })) {
                    eprintln!("resumed fleet conductor panicked: {panic:?}");
                }
            });
    }
}

/// Pull `key=value` out of a space-separated detail string (the fleet
/// run_started detail is `streams=N repo_root=PATH`). Returns the value up
/// to the next space, or empty.
fn detail_field(detail: &str, key: &str) -> String {
    let needle = format!("{key}=");
    detail
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&needle))
        .unwrap_or("")
        .to_string()
}

/// Split a newline-separated plan field into trimmed, non-empty lines.
fn lines_field(plan_fields: &std::collections::BTreeMap<String, String>, key: &str) -> Vec<String> {
    plan_fields
        .get(key)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// One stream's delegation packet, built the way the field's lessons
/// demand: a self-sufficient objective, explicit boundaries (its write
/// scope, enforced), its own contract, and every NEIGHBOR's contract so
/// it codes against decided seams instead of guessing.
fn stream_request(
    plan: &FleetRunPlan,
    stream: &FleetStream,
    run_id: &str,
    builder_casting: &crate::forge_model_scorecard_route::BuilderCastingEvidence,
) -> ForgeRunRequest {
    let repo_profile = crate::forge_repo_profile::profile_repo(&plan.repo_root);
    let semantic_session = crate::forge_semantic_query_route::semantic_session_evidence(
        &plan.repo_root,
        &repo_profile,
    );
    let neighbors: Vec<String> = plan
        .streams
        .iter()
        .filter(|other| other.stream_id != stream.stream_id)
        .map(|other| {
            format!(
                "- {} (owns {}): {}",
                other.stream_id,
                other.write_scope.join(", "),
                if other.interface_contract.is_empty() {
                    other.objective.as_str()
                } else {
                    other.interface_contract.as_str()
                }
            )
        })
        .collect();
    // The verification protocol must be EXPLICIT, especially when no
    // check commands are allowed - the live proof showed streams burning
    // their whole budget searching for test frameworks that do not
    // exist, exactly the failure mode Anthropic documented. Tell the
    // agent precisely what done looks like.
    let verification = if stream.checks.is_empty() {
        "Verification protocol: NO check commands are allowed in this stream. Do not search for test frameworks, linters, or build tools - there are none for you here. Verify by re-reading your edited file; the moment it matches the objective, call finish with outcome done. Editing and finishing promptly is the whole job."
            .to_string()
    } else {
        format!(
            "Verification protocol: run exactly these allowed commands to prove your work, nothing else:\n{}\nIf one of these selected proof commands is red before editing, repair the failing lane proof inside your stream write scope; that red proof is your first proof signal.",
            stream.checks.join("\n")
        )
    };
    let semantic_strategy = fleet_stream_semantic_strategy(stream);
    let selected_checks = if stream.checks.is_empty() {
        repo_profile.suggested_checks.join(",")
    } else {
        stream.checks.join(",")
    };
    let classification = mdx_core::classify_forge_work(&stream.objective, &selected_checks);
    let repo_protocol = format!(
        "Repo intelligence contract: this is a {} repo using language pack {}. Before editing, orient with semantic_query against the files and symbols in your write scope using your required semantic operations; prefer repo-native idioms from that pack. Semantic session: {} status={} indexed_files={} symbols={} related_tests={} grants_authority=false. Use artifact filters when reviewing your own diff: {}. Repo proof guidance: {}. Semantic tools: {}. Toolchain readiness: {}.",
        repo_profile.primary_language,
        repo_profile.language_pack_id,
        semantic_session.session_id,
        semantic_session.fallback_index_status,
        semantic_session.indexed_file_count,
        semantic_session.indexed_symbol_count,
        semantic_session.related_test_anchor_count,
        list_or_none_static(&repo_profile.artifact_patterns),
        repo_profile.proof_plan_summary,
        list_or_none_string(&repo_profile.semantic_tool_readiness),
        list_or_none_string(&repo_profile.toolchain_readiness),
    );
    let intent = format!(
        "Fleet stream {}: {}\n\nYou are one stream of a fleet build - other agents are building the neighboring pieces RIGHT NOW in the same repository.\n\nFleet semantic strategy: {} - {} {}\nRequired semantic operations: {}.\nProof bias: {}.\n\nBuilder casting evidence: status={}, selected_slot={}, recommended_profile={}, basis={}, grants_authority=false.\n\n{}\n\n{}\n\nYour interface contract (provide exactly this):\n{}\n\nYour write scope - the ONLY files you may edit or create:\n{}\n\n{}\n\nYour neighbors and their contracts (code against these; their files are off limits and the system will refuse such edits):\n{}\n\nShared registration files ({}) belong to the integration phase. If your piece needs a registration or export there, put a clearly-marked note in a comment at the top of your main file instead of editing them.\n\nIf the objective cannot be met inside your write scope, call finish with outcome cannot_proceed immediately and say why - do not spend turns exploring around the wall.",
        stream.stream_id,
        stream.objective.lines().next().unwrap_or(""),
        semantic_strategy.id,
        semantic_strategy.summary,
        semantic_strategy.instructions,
        semantic_strategy.required_semantic_operations.join(", "),
        semantic_strategy.proof_bias,
        builder_casting.status.as_str(),
        if builder_casting.selected_builder_slot.is_empty() {
            "default"
        } else {
            builder_casting.selected_builder_slot.as_str()
        },
        if builder_casting.recommended_model_profile_id.is_empty() {
            "none"
        } else {
            builder_casting.recommended_model_profile_id.as_str()
        },
        builder_casting.basis.as_str(),
        stream.objective,
        repo_protocol,
        if stream.interface_contract.is_empty() {
            "none declared"
        } else {
            &stream.interface_contract
        },
        stream.write_scope.join("\n"),
        verification,
        if neighbors.is_empty() {
            "none - you are the only stream".to_string()
        } else {
            neighbors.join("\n")
        },
        plan.integration_owned_paths.join(", "),
    );
    ForgeRunRequest {
        run_id: run_id.to_string(),
        tenant_id: plan.tenant_id.clone(),
        actor_id: plan.actor_id.clone(),
        work_item_id: format!("work_item_{}_{}", plan.fleet_id, stream.stream_id),
        intent,
        allowed_commands: stream.checks.clone(),
        max_turns: stream.max_turns,
        revise_branch: None,
        resume: false,
        write_scope: stream.write_scope.clone(),
        // Absolute, under the LIVE repo (not the throwaway worktree), so
        // the cache survives the run and warms the stream's next attempt.
        check_target_dir: plan.check_target_dir(
            plan.repo_root
                .join(".mdx-local/fleet-targets")
                .join(&stream.stream_id),
        ),
        // The planner's casting decision for this stream: which coder runs
        // it, by difficulty and cost. Empty falls through to the default
        // builder; an unknown slot degrades safely to it at run time.
        builder_slot: builder_casting.selected_builder_slot.clone(),
        work_complexity_tier: classification.complexity_tier.to_string(),
        semantic_policy_required_operations: semantic_strategy
            .required_semantic_operations
            .iter()
            .map(|operation| (*operation).to_string())
            .collect(),
        semantic_policy_source: "fleet_stream_semantic_strategy".to_string(),
        execution_geometry_requested_workers: plan.requested_width.max(1),
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "fleet_stream".to_string(),
        execution_geometry_route: "/forge/fleet-runs.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: 0,
        max_runtime_ms: 0,
        envelope_id: String::new(),
        plan_only: false,
        reasoning_effort: String::new(),
    }
}

fn list_or_none_static(items: &[&str]) -> String {
    if items.is_empty() {
        "none detected".to_string()
    } else {
        items.join(", ")
    }
}

fn list_or_none_string(items: &[String]) -> String {
    if items.is_empty() {
        "none detected".to_string()
    } else {
        items.join(", ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FleetStreamSemanticStrategy {
    id: &'static str,
    summary: &'static str,
    instructions: &'static str,
    required_semantic_operations: &'static [&'static str],
    proof_bias: &'static str,
}

fn fleet_stream_semantic_strategy(stream: &FleetStream) -> FleetStreamSemanticStrategy {
    let objective = stream.objective.to_ascii_lowercase();
    let scope = stream.write_scope.join("\n").to_ascii_lowercase();
    let contract = stream.interface_contract.to_ascii_lowercase();
    if objective.contains("test")
        || scope.contains("/test")
        || scope.contains("tests/")
        || scope.contains("spec")
    {
        FleetStreamSemanticStrategy {
            id: "stream_test_proof",
            summary: "prove behavior from the fleet edge",
            instructions: "Start from the checks and related tests, then make the smallest proof-oriented change inside this stream.",
            required_semantic_operations: &[
                "related_tests",
                "references",
                "dependency_map",
                "diagnostics",
            ],
            proof_bias: "red_green_or_related_test_first",
        }
    } else if objective.contains("refactor")
        || objective.contains("api")
        || contract.contains("api")
        || contract.contains("interface")
    {
        FleetStreamSemanticStrategy {
            id: "stream_contract_guard",
            summary: "preserve interfaces while improving the local design",
            instructions: "Map definitions and references before changing public contracts, and keep compatibility visible for the integrator.",
            required_semantic_operations: &[
                "file_outline",
                "definition",
                "references",
                "diagnostics",
            ],
            proof_bias: "interface_compatibility_and_behavioral_check",
        }
    } else if objective.contains("security")
        || objective.contains("performance")
        || objective.contains("edge")
        || objective.contains("failure")
    {
        FleetStreamSemanticStrategy {
            id: "stream_risk_probe",
            summary: "hunt failure modes before repairing",
            instructions: "Trace dependencies and references first, then repair the riskiest local path with focused proof.",
            required_semantic_operations: &[
                "dependency_map",
                "references",
                "related_tests",
                "diagnostics",
            ],
            proof_bias: "edge_case_security_performance_risk_review",
        }
    } else {
        FleetStreamSemanticStrategy {
            id: "stream_minimal_safe_patch",
            summary: "smallest correct stream-local implementation",
            instructions: "Anchor the target symbol, map dependencies, then land the smallest stream-local diff that satisfies the contract.",
            required_semantic_operations: &[
                "symbol_graph",
                "definition",
                "dependency_map",
                "related_tests",
            ],
            proof_bias: "minimal_diff_with_behavioral_check",
        }
    }
}

fn stream_builder_casting(
    kernel: &MdxKernel,
    plan: &FleetRunPlan,
    stream: &FleetStream,
) -> crate::forge_model_scorecard_route::BuilderCastingEvidence {
    let repo_profile = crate::forge_repo_profile::profile_repo(&plan.repo_root);
    let selected_checks = if stream.checks.is_empty() {
        repo_profile.suggested_checks.join(",")
    } else {
        stream.checks.join(",")
    };
    let classification = mdx_core::classify_forge_work(&stream.objective, &selected_checks);
    crate::forge_model_scorecard_route::builder_casting_evidence(
        kernel,
        repo_profile.language_pack_id,
        &classification.task_class,
        &classification.complexity_tier,
        &stream.builder_slot,
    )
}

fn stream_run_started_evidence_fields(
    plan: &FleetRunPlan,
    stream: &FleetStream,
    builder_casting: &crate::forge_model_scorecard_route::BuilderCastingEvidence,
) -> Vec<(String, String)> {
    stream_run_started_evidence_fields_for_lane(
        plan,
        stream,
        builder_casting,
        "fleet_stream",
        "stream_from_governed_fleet_plan",
    )
}

fn stream_run_started_evidence_fields_for_lane(
    plan: &FleetRunPlan,
    stream: &FleetStream,
    builder_casting: &crate::forge_model_scorecard_route::BuilderCastingEvidence,
    execution_geometry_lane: &str,
    execution_geometry_reason: &str,
) -> Vec<(String, String)> {
    let repo_profile = crate::forge_repo_profile::profile_repo(&plan.repo_root);
    let semantic_session = crate::forge_semantic_query_route::semantic_session_evidence(
        &plan.repo_root,
        &repo_profile,
    );
    let semantic_strategy = fleet_stream_semantic_strategy(stream);
    let selected_checks = if stream.checks.is_empty() {
        repo_profile.suggested_checks.join("\n")
    } else {
        stream.checks.join("\n")
    };
    vec![
        ("fleet_id".to_string(), plan.fleet_id.clone()),
        ("repo_id".to_string(), plan.repo_id.clone()),
        (
            "execution_backend_kind".to_string(),
            plan.execution_backend.clone(),
        ),
        (
            "execution_target_kind".to_string(),
            if plan.hosted_execution() {
                "mdx_cloud".to_string()
            } else {
                "paired_host".to_string()
            },
        ),
        (
            "execution_target_id".to_string(),
            if plan.hosted_execution() {
                plan.cloud_environment_id.clone()
            } else {
                "local_host".to_string()
            },
        ),
        (
            "cloud_environment_id".to_string(),
            plan.cloud_environment_id.clone(),
        ),
        ("fleet_stream_id".to_string(), stream.stream_id.clone()),
        (
            "fleet_stream_write_scope".to_string(),
            stream.write_scope.join("\n"),
        ),
        (
            "fleet_stream_interface_contract".to_string(),
            stream.interface_contract.clone(),
        ),
        (
            "fleet_stream_builder_slot".to_string(),
            stream.builder_slot.clone(),
        ),
        (
            "fleet_stream_selected_builder_slot".to_string(),
            builder_casting.selected_builder_slot.clone(),
        ),
        (
            "fleet_stream_data_sensitivity".to_string(),
            stream.data_sensitivity.clone(),
        ),
        (
            "fleet_stream_semantic_strategy_id".to_string(),
            semantic_strategy.id.to_string(),
        ),
        (
            "fleet_stream_semantic_strategy_summary".to_string(),
            semantic_strategy.summary.to_string(),
        ),
        (
            "fleet_stream_required_semantic_operations".to_string(),
            semantic_strategy.required_semantic_operations.join("\n"),
        ),
        (
            "fleet_stream_proof_bias".to_string(),
            semantic_strategy.proof_bias.to_string(),
        ),
        (
            "builder_casting_status".to_string(),
            builder_casting.status.clone(),
        ),
        (
            "builder_casting_requested_slot".to_string(),
            builder_casting.requested_builder_slot.clone(),
        ),
        (
            "builder_casting_selected_slot".to_string(),
            builder_casting.selected_builder_slot.clone(),
        ),
        (
            "builder_casting_selected_model_profile_id".to_string(),
            builder_casting.selected_model_profile_id.clone(),
        ),
        (
            "builder_casting_selected_provider_family".to_string(),
            builder_casting.selected_provider_family.clone(),
        ),
        (
            "builder_casting_selected_model_id".to_string(),
            builder_casting.selected_model_id.clone(),
        ),
        (
            "builder_casting_recommended_slot".to_string(),
            builder_casting.recommended_builder_slot.clone(),
        ),
        (
            "builder_casting_recommended_model_profile_id".to_string(),
            builder_casting.recommended_model_profile_id.clone(),
        ),
        (
            "builder_casting_recommended_provider_family".to_string(),
            builder_casting.recommended_provider_family.clone(),
        ),
        (
            "builder_casting_recommended_model_id".to_string(),
            builder_casting.recommended_model_id.clone(),
        ),
        (
            "builder_casting_basis".to_string(),
            builder_casting.basis.clone(),
        ),
        (
            "builder_casting_matching_eval_score_count".to_string(),
            builder_casting.matching_eval_score_count.to_string(),
        ),
        (
            "builder_casting_accepted_eval_score_count".to_string(),
            builder_casting.accepted_eval_score_count.to_string(),
        ),
        (
            "builder_casting_matching_run_count".to_string(),
            builder_casting.matching_run_count.to_string(),
        ),
        (
            "builder_casting_done_rate_pct".to_string(),
            builder_casting.done_rate_pct.to_string(),
        ),
        (
            "builder_casting_requested_slot_matches_evidence".to_string(),
            builder_casting.requested_slot_matches_evidence.to_string(),
        ),
        (
            "builder_casting_grants_execution_authority".to_string(),
            "false".to_string(),
        ),
        (
            "repo_root".to_string(),
            plan.repo_root.display().to_string(),
        ),
        (
            "repo_primary_language".to_string(),
            repo_profile.primary_language.to_string(),
        ),
        (
            "language_pack_id".to_string(),
            repo_profile.language_pack_id.to_string(),
        ),
        (
            "detected_language_packs".to_string(),
            repo_profile.detected_language_packs.join("\n"),
        ),
        (
            "repo_profile_suggested_checks".to_string(),
            repo_profile.suggested_checks.join("\n"),
        ),
        (
            "repo_profile_artifact_patterns".to_string(),
            repo_profile.artifact_patterns.join("\n"),
        ),
        (
            "repo_profile_semantic_intelligence".to_string(),
            repo_profile.semantic_intelligence.join("\n"),
        ),
        (
            "repo_profile_semantic_tool_readiness".to_string(),
            repo_profile.semantic_tool_readiness.join("\n"),
        ),
        (
            "repo_profile_semantic_session_id".to_string(),
            semantic_session.session_id,
        ),
        (
            "repo_profile_semantic_fallback_index_status".to_string(),
            semantic_session.fallback_index_status.to_string(),
        ),
        (
            "repo_profile_semantic_source_file_count".to_string(),
            semantic_session.source_file_count.to_string(),
        ),
        (
            "repo_profile_semantic_indexed_file_count".to_string(),
            semantic_session.indexed_file_count.to_string(),
        ),
        (
            "repo_profile_semantic_indexed_symbol_count".to_string(),
            semantic_session.indexed_symbol_count.to_string(),
        ),
        (
            "repo_profile_semantic_related_test_anchor_count".to_string(),
            semantic_session.related_test_anchor_count.to_string(),
        ),
        (
            "repo_profile_semantic_session_grants_execution_authority".to_string(),
            semantic_session.grants_execution_authority.to_string(),
        ),
        (
            "repo_profile_toolchain_readiness".to_string(),
            repo_profile.toolchain_readiness.join("\n"),
        ),
        (
            "repo_profile_proof_plan_status".to_string(),
            repo_profile.proof_plan_status.to_string(),
        ),
        (
            "repo_profile_proof_plan_summary".to_string(),
            repo_profile.proof_plan_summary,
        ),
        ("selected_checks".to_string(), selected_checks),
        (
            "selected_checks_source".to_string(),
            if stream.checks.is_empty() {
                "repo_profile_suggested_checks"
            } else {
                "fleet_stream_checks"
            }
            .to_string(),
        ),
        (
            "principal_orientation_gate_required".to_string(),
            "true".to_string(),
        ),
        (
            "principal_orientation_gate_tool".to_string(),
            "semantic_query".to_string(),
        ),
        (
            "principal_orientation_gate_grants_authority".to_string(),
            "false".to_string(),
        ),
        (
            "execution_geometry_requested_workers".to_string(),
            plan.requested_width.max(1).to_string(),
        ),
        (
            "execution_geometry_effective_workers".to_string(),
            "1".to_string(),
        ),
        (
            "execution_geometry_lane".to_string(),
            execution_geometry_lane.to_string(),
        ),
        (
            "execution_geometry_route".to_string(),
            "/forge/fleet-runs.json".to_string(),
        ),
        (
            "execution_geometry_reason".to_string(),
            execution_geometry_reason.to_string(),
        ),
        (
            "execution_geometry_fleet_required".to_string(),
            "false".to_string(),
        ),
        (
            "execution_geometry_grants_execution_authority".to_string(),
            "false".to_string(),
        ),
    ]
}

/// The fleet wall-clock ceiling. Recorded numbers are enforced numbers:
/// a fleet that cannot finish inside the deadline reports attention
/// lanes instead of blocking its conductor thread forever.
fn fleet_deadline_ms() -> u64 {
    std::env::var("MDX_FLEET_DEADLINE_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(90 * 60 * 1000)
}

fn record_event(
    kernel: &Arc<RwLock<MdxKernel>>,
    plan: &FleetRunPlan,
    event: &str,
    stream_id: &str,
    forge_run_id: &str,
    detail: &str,
) {
    if let Ok(mut kernel) = kernel.write() {
        let _ = kernel.record_fleet_run_event(FleetRunEvent {
            tenant_id: &plan.tenant_id,
            actor_id: "agent:fleet_conductor",
            fleet_id: &plan.fleet_id,
            event,
            stream_id,
            forge_run_id,
            detail,
        });
    }
    // Terminal moments survive a restart: each stream outcome and the
    // fleet rollup snapshot the chain (mid-run receipts ride the next
    // boundary - a restart kills the threads anyway).
    if matches!(
        event,
        "stream_finished"
            | "stream_needs_attention"
            | "lane_repair_finished"
            | "integration_finished"
            | "run_finished"
    ) {
        crate::kernel_snapshot::snapshot_at_boundary(kernel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn recovery_seeds_terminal_lanes_without_erasing_full_stream_contract() {
        let streams = ["s1_readme", "s2_architecture", "s3_pending"]
            .into_iter()
            .map(|stream_id| FleetStream {
                stream_id: stream_id.to_string(),
                objective: format!("Complete {stream_id}"),
                write_scope: vec![format!("{stream_id}.md")],
                interface_contract: "Keep docs consistent".to_string(),
                depends_on: Vec::new(),
                checks: vec!["cargo test".to_string()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            })
            .collect::<Vec<_>>();
        let prior_green = vec![("s1_readme".to_string(), "forge/run-readme".to_string())];
        let prior_attention = vec!["s2_architecture".to_string()];

        let completed = seeded_completed_stream_ids(&prior_green, &prior_attention);
        let pending = streams
            .iter()
            .filter(|stream| !completed.contains(&stream.stream_id))
            .map(|stream| stream.stream_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(streams.len(), 3, "integration retains all ratified streams");
        assert_eq!(pending, vec!["s3_pending"]);
    }

    #[test]
    fn fleet_deadline_identifies_each_unreported_submitted_lane() {
        let submitted = vec![
            ("s1_green".to_string(), "forge_run_1".to_string()),
            ("s2_stuck".to_string(), "forge_run_2".to_string()),
            ("s3_stuck".to_string(), "forge_run_3".to_string()),
        ];
        let reported = HashSet::from(["s1_green".to_string()]);

        assert_eq!(
            unreported_submitted_lanes(&submitted, &reported),
            vec![
                ("s2_stuck".to_string(), "forge_run_2".to_string()),
                ("s3_stuck".to_string(), "forge_run_3".to_string()),
            ]
        );
    }

    #[test]
    fn fleet_terminal_outcome_records_mission_checkpoint() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            kernel
                .admit_forge_long_horizon_mission_local(
                    mdx_core::ForgeLongHorizonMissionAdmission {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        actor_role: "owner",
                        mission_id: "mission_fleet_scale",
                        goal: "Execute a fleet milestone",
                        non_goals: "none",
                        constraints: "stay local",
                        done_when: "fleet proof passes",
                        allowed_write_scope: "src/**",
                        blocked_paths: "target/**",
                        validation_commands: "cargo test",
                        model_policy: "configured builders only",
                        provider_allowlist: "xai,anthropic,openai",
                        fleet_width: 4,
                        max_runtime_ms: 600_000,
                        max_cost_cents: 10_000,
                        checkpoint_cadence_minutes: 15,
                    },
                )
                .expect("mission admitted");
        }
        let plan = FleetRunPlan {
            fleet_id: "fleet_scale_001".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:eng".to_string(),
            repo_root: PathBuf::from("."),
            repo_id: String::new(),
            execution_backend: "local".to_string(),
            cloud_environment_id: String::new(),
            requested_width: 2,
            streams: Vec::new(),
            integration_owned_paths: Vec::new(),
            full_suite_checks: Vec::new(),
            mission_id: "mission_fleet_scale".to_string(),
            mission_milestone_id: "mission_milestone_01".to_string(),
        };

        record_mission_checkpoint_from_fleet_outcome(&kernel, &plan, 2, 2, 0, true);

        let dashboard = kernel
            .read()
            .expect("kernel")
            .project_forge_long_horizon_missions();
        let mission = dashboard
            .packets
            .iter()
            .find(|mission| mission.mission_id == "mission_fleet_scale")
            .expect("mission");
        assert_eq!(mission.completed_milestone_count, 1);
        assert_eq!(mission.related_fleet_ids, vec!["fleet_scale_001"]);
        assert_eq!(mission.milestones[0].related_fleet_id, "fleet_scale_001");
        assert_eq!(mission.milestones[0].validation_status, "fleet_passed");
    }

    #[test]
    fn stream_request_names_repo_intelligence_contract_for_parallel_workers() {
        let repo = temp_repo("fleet_stream_swift");
        std::fs::write(repo.join("Package.swift"), "// swift package\n").expect("package");
        std::fs::create_dir_all(repo.join("Sources/Canary")).expect("sources");
        std::fs::write(
            repo.join("Sources/Canary/Slugger.swift"),
            "struct Slugger {}\n",
        )
        .expect("source");
        let stream = FleetStream {
            stream_id: "s1_slugger".to_string(),
            objective: "Implement slug generation.".to_string(),
            write_scope: vec!["Sources/Canary/Slugger.swift".to_string()],
            interface_contract: "Keep Slugger API stable.".to_string(),
            depends_on: Vec::new(),
            checks: vec!["swift test".to_string()],
            max_turns: 20,
            builder_slot: String::new(),
            data_sensitivity: "internal".to_string(),
        };
        let mut plan = FleetRunPlan {
            fleet_id: "fleet_test".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:eng".to_string(),
            repo_root: repo,
            repo_id: "github_test".to_string(),
            execution_backend: "local".to_string(),
            cloud_environment_id: String::new(),
            requested_width: 8,
            streams: vec![stream.clone()],
            integration_owned_paths: vec!["Package.swift".to_string()],
            full_suite_checks: vec!["swift test".to_string()],
            mission_id: String::new(),
            mission_milestone_id: String::new(),
        };

        let builder_casting = crate::forge_model_scorecard_route::BuilderCastingEvidence {
            status: "INSUFFICIENT_EVIDENCE".to_string(),
            basis: "no_matching_language_task_model_evidence_yet".to_string(),
            ..Default::default()
        };
        let request = stream_request(&plan, &stream, "forge_run_1", &builder_casting);

        assert!(request.intent.contains("language pack swift-spm"));
        assert!(
            request
                .intent
                .contains("Fleet semantic strategy: stream_contract_guard")
        );
        assert!(request.intent.contains(
            "Required semantic operations: file_outline, definition, references, diagnostics."
        ));
        assert!(
            request
                .intent
                .contains("Proof bias: interface_compatibility_and_behavioral_check.")
        );
        assert!(
            request
                .intent
                .contains("Builder casting evidence: status=INSUFFICIENT_EVIDENCE")
        );
        assert!(
            request
                .intent
                .contains("using your required semantic operations")
        );
        assert!(request.intent.contains(".build/**"));
        assert!(request.intent.contains("swift test"));
        assert_eq!(request.allowed_commands, vec!["swift test"]);
        assert_eq!(request.execution_geometry_requested_workers, 8);
        assert_eq!(request.execution_geometry_effective_workers, 1);
        assert_eq!(request.execution_geometry_lane, "fleet_stream");
        assert_eq!(request.execution_geometry_route, "/forge/fleet-runs.json");

        plan.execution_backend = "hosted_sandbox".to_string();
        plan.cloud_environment_id = "env_cloud_beta".to_string();
        let hosted_request = stream_request(&plan, &stream, "forge_run_hosted_1", &builder_casting);
        assert_eq!(
            hosted_request.check_target_dir.as_deref(),
            Some("hosted://env_cloud_beta")
        );
        let evidence = stream_run_started_evidence_fields(&plan, &stream, &builder_casting);
        let get = |key: &str| {
            evidence
                .iter()
                .find(|(field, _)| field == key)
                .map(|(_, value)| value.as_str())
                .unwrap_or("")
        };
        assert_eq!(get("execution_backend_kind"), "hosted_sandbox");
        assert_eq!(get("execution_target_kind"), "mdx_cloud");
        assert_eq!(get("execution_target_id"), "env_cloud_beta");
        assert_eq!(get("cloud_environment_id"), "env_cloud_beta");
    }

    #[test]
    fn fleet_stream_run_started_evidence_is_repo_intelligent_and_geometric() {
        let repo = temp_repo("fleet_stream_evidence_java");
        std::fs::write(repo.join("pom.xml"), "<project></project>\n").expect("pom");
        let stream = FleetStream {
            stream_id: "s1_service".to_string(),
            objective: "Refactor service boundary.".to_string(),
            write_scope: vec!["src/main/java/app/Service.java".to_string()],
            interface_contract: "Keep public Service API stable.".to_string(),
            depends_on: Vec::new(),
            checks: vec!["mvn test".to_string()],
            max_turns: 22,
            builder_slot: "SONNET".to_string(),
            data_sensitivity: "internal".to_string(),
        };
        let plan = FleetRunPlan {
            fleet_id: "fleet_java".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:eng".to_string(),
            repo_root: repo,
            repo_id: "github_test".to_string(),
            execution_backend: "local".to_string(),
            cloud_environment_id: String::new(),
            requested_width: 8,
            streams: vec![
                stream.clone(),
                FleetStream {
                    stream_id: "s2_tests".to_string(),
                    objective: "Add regression tests.".to_string(),
                    write_scope: vec!["src/test/java/app/ServiceTest.java".to_string()],
                    interface_contract: "Verify Service API.".to_string(),
                    depends_on: vec!["s1_service".to_string()],
                    checks: vec!["mvn test".to_string()],
                    max_turns: 22,
                    builder_slot: String::new(),
                    data_sensitivity: "internal".to_string(),
                },
            ],
            integration_owned_paths: vec!["pom.xml".to_string()],
            full_suite_checks: vec!["mvn test".to_string()],
            mission_id: String::new(),
            mission_milestone_id: String::new(),
        };

        let builder_casting = crate::forge_model_scorecard_route::BuilderCastingEvidence {
            status: "LOCAL_RUN_EVIDENCE_REQUESTED_SLOT_OVERRIDES_EVIDENCE".to_string(),
            requested_builder_slot: "SONNET".to_string(),
            selected_builder_slot: "SONNET".to_string(),
            recommended_builder_slot: "OPUS".to_string(),
            recommended_model_profile_id: "codex_anthropic_responses_profile".to_string(),
            recommended_provider_family: "anthropic".to_string(),
            recommended_model_id: "claude-opus-4-8".to_string(),
            basis: "local_run_track_record".to_string(),
            matching_run_count: 3,
            done_rate_pct: 67,
            requested_slot_matches_evidence: false,
            ..Default::default()
        };
        let evidence = stream_run_started_evidence_fields(&plan, &stream, &builder_casting);
        let get = |key: &str| {
            evidence
                .iter()
                .find(|(field, _)| field == key)
                .map(|(_, value)| value.as_str())
                .unwrap_or("")
        };

        assert_eq!(get("fleet_id"), "fleet_java");
        assert_eq!(get("fleet_stream_id"), "s1_service");
        assert_eq!(get("language_pack_id"), "java-maven");
        assert_eq!(get("selected_checks_source"), "fleet_stream_checks");
        assert_eq!(get("selected_checks"), "mvn test");
        assert_eq!(
            get("fleet_stream_semantic_strategy_id"),
            "stream_contract_guard"
        );
        assert_eq!(
            get("fleet_stream_required_semantic_operations"),
            "file_outline\ndefinition\nreferences\ndiagnostics"
        );
        assert_eq!(
            get("fleet_stream_proof_bias"),
            "interface_compatibility_and_behavioral_check"
        );
        assert_eq!(get("fleet_stream_builder_slot"), "SONNET");
        assert_eq!(get("fleet_stream_selected_builder_slot"), "SONNET");
        assert_eq!(
            get("builder_casting_status"),
            "LOCAL_RUN_EVIDENCE_REQUESTED_SLOT_OVERRIDES_EVIDENCE"
        );
        assert_eq!(get("builder_casting_recommended_slot"), "OPUS");
        assert_eq!(get("builder_casting_matching_run_count"), "3");
        assert_eq!(get("builder_casting_done_rate_pct"), "67");
        assert_eq!(
            get("builder_casting_requested_slot_matches_evidence"),
            "false"
        );
        assert_eq!(get("execution_geometry_requested_workers"), "8");
        assert_eq!(get("execution_geometry_effective_workers"), "1");
        assert_eq!(get("execution_geometry_lane"), "fleet_stream");
        assert_eq!(get("execution_geometry_route"), "/forge/fleet-runs.json");
        assert_eq!(
            get("execution_geometry_reason"),
            "stream_from_governed_fleet_plan"
        );
        assert_eq!(get("principal_orientation_gate_tool"), "semantic_query");
    }

    #[test]
    fn lane_repair_run_started_evidence_reports_repair_geometry() {
        let repo = temp_repo("fleet_lane_repair_evidence_node");
        std::fs::write(repo.join("package.json"), r#"{"type":"module"}"#).expect("package");
        let stream = FleetStream {
            stream_id: "s1_export".to_string(),
            objective: "Implement export lane.".to_string(),
            write_scope: vec!["src/export.js".to_string()],
            interface_contract: "Export summarize.".to_string(),
            depends_on: Vec::new(),
            checks: vec!["npm test".to_string()],
            max_turns: 22,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        };
        let plan = FleetRunPlan {
            fleet_id: "fleet_node_repair".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:eng".to_string(),
            repo_root: repo,
            repo_id: "github_test".to_string(),
            execution_backend: "local".to_string(),
            cloud_environment_id: String::new(),
            requested_width: 16,
            streams: vec![stream.clone()],
            integration_owned_paths: Vec::new(),
            full_suite_checks: vec!["npm test".to_string()],
            mission_id: String::new(),
            mission_milestone_id: String::new(),
        };
        let builder_casting = crate::forge_model_scorecard_route::BuilderCastingEvidence {
            status: "LOCAL_RUN_EVIDENCE_SLOT_READY".to_string(),
            selected_builder_slot: "GROK".to_string(),
            recommended_builder_slot: "GROK".to_string(),
            recommended_model_profile_id: "codex_xai_grok_profile".to_string(),
            recommended_provider_family: "xai".to_string(),
            recommended_model_id: "grok-4-3".to_string(),
            basis: "local_run_track_record".to_string(),
            matching_run_count: 8,
            done_rate_pct: 88,
            ..Default::default()
        };
        let evidence = stream_run_started_evidence_fields_for_lane(
            &plan,
            &stream,
            &builder_casting,
            "fleet_lane_repair",
            "repair_from_governed_fleet_runtime",
        );
        let get = |key: &str| {
            evidence
                .iter()
                .find(|(field, _)| field == key)
                .map(|(_, value)| value.as_str())
                .unwrap_or("")
        };

        assert_eq!(get("fleet_id"), "fleet_node_repair");
        assert_eq!(get("fleet_stream_id"), "s1_export");
        assert_eq!(get("language_pack_id"), "node");
        assert_eq!(get("execution_geometry_requested_workers"), "16");
        assert_eq!(get("execution_geometry_effective_workers"), "1");
        assert_eq!(get("execution_geometry_lane"), "fleet_lane_repair");
        assert_eq!(
            get("execution_geometry_reason"),
            "repair_from_governed_fleet_runtime"
        );
        assert_eq!(get("builder_casting_selected_slot"), "GROK");
        assert_eq!(get("principal_orientation_gate_tool"), "semantic_query");
    }

    #[test]
    fn native_macos_check_can_open_bounded_proof_support_repair_scope() {
        let repo = temp_repo("fleet_native_proof_support_scope");
        std::fs::create_dir_all(repo.join("scripts")).expect("scripts");
        std::fs::write(
            repo.join("scripts/native-macos-operator-check.sh"),
            "#!/bin/sh\n",
        )
        .expect("native check");
        std::fs::write(
            repo.join("Makefile"),
            "native-macos-operator-check:\n\ttrue\n",
        )
        .expect("makefile");
        let stream = FleetStream {
            stream_id: "s1_native".to_string(),
            objective: "Fix native inspector state.".to_string(),
            write_scope: vec![
                "apps/mdx-operator-macos/Sources/MDxOperator/Views/InspectorView.swift".to_string(),
            ],
            interface_contract: String::new(),
            depends_on: Vec::new(),
            checks: vec!["make native-macos-operator-check".to_string()],
            max_turns: 18,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        };

        let support = proof_support_repair_scope(&stream, &repo).expect("support scope");
        assert_eq!(
            support,
            vec![
                "scripts/native-macos-operator-check.sh".to_string(),
                "Makefile".to_string()
            ]
        );
        let expanded = expanded_repair_scope(&stream, &support);
        assert!(expanded.contains(
            &"apps/mdx-operator-macos/Sources/MDxOperator/Views/InspectorView.swift".to_string()
        ));
        assert!(expanded.contains(&"scripts/native-macos-operator-check.sh".to_string()));
        assert!(expanded.contains(&"Makefile".to_string()));
    }

    #[test]
    fn ordinary_stream_check_does_not_open_proof_support_repair_scope() {
        let repo = temp_repo("fleet_no_proof_support_scope");
        std::fs::create_dir_all(repo.join("scripts")).expect("scripts");
        std::fs::write(
            repo.join("scripts/native-macos-operator-check.sh"),
            "#!/bin/sh\n",
        )
        .expect("native check");
        std::fs::write(
            repo.join("Makefile"),
            "native-macos-operator-check:\n\ttrue\n",
        )
        .expect("makefile");
        let stream = FleetStream {
            stream_id: "s1_node".to_string(),
            objective: "Fix node export.".to_string(),
            write_scope: vec!["src/export.js".to_string()],
            interface_contract: String::new(),
            depends_on: Vec::new(),
            checks: vec!["npm test".to_string()],
            max_turns: 18,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        };

        assert!(proof_support_repair_scope(&stream, &repo).is_none());
    }

    fn temp_repo(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("mdx_{label}_{}_{}", std::process::id(), nonce));
        std::fs::create_dir_all(dir.join(".git")).expect("git");
        dir
    }
}
