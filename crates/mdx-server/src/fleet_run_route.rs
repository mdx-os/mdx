// The fleet fan-out over HTTP. POST starts the run for a RATIFIED plan -
// the kernel refuses unratified or already-run fleets, the start is
// witnessed synchronously (the response cites the receipt), and the
// conductor thread takes it from there. GET folds the fleet's event
// receipts into lanes: one per stream, with its underlying forge run id
// so the run viewer's trail, diff, and ship verbs all apply unchanged.
use crate::RouteResponse;
use crate::fleet_run_conductor::{FleetRunPlan, run_fleet};
use mdx_core::{FleetRunEvent, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

#[cfg(test)]
thread_local! {
    static SKIP_FLEET_CONDUCTOR_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) struct SkipFleetConductorGuard {
    previous: bool,
}

#[cfg(test)]
impl Drop for SkipFleetConductorGuard {
    fn drop(&mut self) {
        SKIP_FLEET_CONDUCTOR_FOR_TEST.set(self.previous);
    }
}

#[cfg(test)]
pub(crate) fn skip_fleet_conductor_for_test() -> SkipFleetConductorGuard {
    let previous = SKIP_FLEET_CONDUCTOR_FOR_TEST.replace(true);
    SkipFleetConductorGuard { previous }
}

const WIDE_FLEET_REVIEW_REQUIRED_WIDTH: u32 = 8;
const FLEET_RUNTIME_MAX_REPAIR_ATTEMPTS: u32 = 2;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/fleet-runs.json" => Some(handle_start(method, body, kernel)),
        "/forge/fleet-runs/projection.json" => Some(handle_projection(method, kernel)),
        "/forge/fleet-capacity.json" => Some(handle_capacity(method)),
        _ => None,
    }
}

/// A live read of the shared worker pool: the global concurrency bound,
/// how many streams are building right now, the high-water mark, and how
/// deep the receipt-recoverable queue is backed up. This is the operator's proof that
/// hundreds of queued streams drain through a bounded budget instead of
/// exploding the machine - the enacted form of the scale-orchestration
/// model.
fn handle_capacity(method: &str) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let status = crate::fleet_executor::pool_status();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-capacity","worker_capacity":{},"active_now":{},"peak_active":{},"queue_depth":{},"queue_depth_limit":{},"submitted_total":{},"completed_total":{},"queue_durability":"receipt_recoverable_single_conductor","restart_recovery_enabled":{},"backpressure_policy":"reserve_workers_then_queue_then_refuse_overflow","production_write_allowed":false}}"#,
            status.capacity,
            status.active,
            status.peak_active,
            status.queue_depth,
            status.queue_depth_limit,
            status.submitted,
            status.completed,
            std::env::var("MDX_FLEET_RESUME").ok().as_deref() == Some("1"),
        ),
    ))
}

fn handle_start(
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
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let fleet_id = parsed["fleet_id"].as_str().unwrap_or("").trim().to_string();
    if fleet_id.is_empty() {
        return Ok(refusal("name the fleet to run"));
    }
    let requested_repo_id = parsed["repo_id"].as_str().unwrap_or("").trim().to_string();
    let cloud_environment_id = parsed["cloud_environment_id"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    let hosted_execution = match hosted_execution_requested(
        parsed["execution_backend"].as_str(),
        &cloud_environment_id,
    ) {
        Ok(hosted) => hosted,
        Err(reason) => return Ok(refusal(reason)),
    };
    let execution_backend = if hosted_execution {
        "hosted_sandbox"
    } else {
        "local"
    };
    let mission_attachment = MissionAttachment::from_body(&parsed);

    let validated_repo_id = {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let Some(plan_fields) = guard.latest_fleet_plan(&fleet_id) else {
            return Ok(refusal(&format!(
                "no plan drafted for {fleet_id} - draft one first"
            )));
        };
        if !guard.fleet_plan_ratified(&fleet_id) {
            return Ok(refusal(&format!(
                "the plan for {fleet_id} must be ratified before its execution target is prepared"
            )));
        }
        if guard.fleet_run_started(&fleet_id) {
            return Ok(refusal(&format!("the fleet {fleet_id} already started")));
        }
        let streams = mdx_core::fleet_plan_streams(&plan_fields);
        let requested_width = plan_fields
            .get("requested_width")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(streams.len() as u32)
            .max(1);
        if let Err(reason) = validate_wide_plan_review(&plan_fields, requested_width) {
            return Ok(refusal(&reason));
        }
        if let Err(reason) = mission_attachment.validate(&guard) {
            return Ok(refusal(&reason));
        }
        fleet_run_repo_id(&requested_repo_id, &plan_fields)
    };

    let (prepared_repo_id, prepared_repo_root) = match prepare_fleet_execution_target(
        kernel,
        &resolved.tenant_id,
        &validated_repo_id,
        hosted_execution,
        &cloud_environment_id,
    ) {
        Ok(target) => target,
        Err(reason) => return Ok(refusal(&reason)),
    };

    // Everything the conductor needs is read and witnessed under one
    // lock: the ratified plan, the start receipt (the kernel refuses
    // unratified or already-run fleets here), and the repo root.
    let (plan, started_receipt_id, resolved_repo_id) = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let Some(plan_fields) = kernel.latest_fleet_plan(&fleet_id) else {
            return Ok(refusal(&format!(
                "no plan drafted for {fleet_id} - draft one first"
            )));
        };
        let streams = mdx_core::fleet_plan_streams(&plan_fields);
        let requested_width = plan_fields
            .get("requested_width")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(streams.len() as u32)
            .max(1);
        if let Err(reason) = validate_wide_plan_review(&plan_fields, requested_width) {
            return Ok(refusal(&reason));
        }
        if let Err(reason) = mission_attachment.validate(&kernel) {
            return Ok(refusal(&reason));
        }
        let resolved_repo_id = fleet_run_repo_id(&requested_repo_id, &plan_fields);
        if resolved_repo_id != prepared_repo_id {
            return Ok(refusal(
                "the fleet plan changed while its execution target was being prepared; start it again",
            ));
        }
        let repo_root = prepared_repo_root.clone();
        let report = match kernel.record_fleet_run_event_with_identity(
            FleetRunEvent {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                fleet_id: &fleet_id,
                event: "run_started",
                stream_id: "",
                forge_run_id: "",
                detail: &format!(
                    "streams={} requested_width={} repo_id={} repo_root={repo_root} execution_backend={} cloud_environment_id={} mission_id={} mission_milestone_id={}",
                    streams.len(),
                    requested_width,
                    if resolved_repo_id.is_empty() {
                        "mdx"
                    } else {
                        &resolved_repo_id
                    },
                    execution_backend,
                    cloud_environment_id,
                    mission_attachment.mission_id,
                    mission_attachment.milestone_id
                ),
            },
            &resolved.identity,
        ) {
            Ok(report) => report,
            Err(error) => return Ok(refusal(&error.message())),
        };
        let integration_owned_paths = plan_fields
            .get("integration_owned_paths")
            .map(|value| {
                value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let full_suite_checks = plan_fields
            .get("full_suite_checks")
            .map(|value| {
                value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        (
            FleetRunPlan {
                fleet_id: fleet_id.clone(),
                tenant_id: resolved.tenant_id.clone(),
                actor_id: resolved.actor_id.clone(),
                repo_root: std::path::PathBuf::from(repo_root),
                repo_id: resolved_repo_id.clone(),
                execution_backend: execution_backend.to_string(),
                cloud_environment_id: cloud_environment_id.clone(),
                requested_width,
                streams,
                integration_owned_paths,
                full_suite_checks,
                mission_id: mission_attachment.mission_id.clone(),
                mission_milestone_id: mission_attachment.milestone_id.clone(),
            },
            report.receipt_id,
            resolved_repo_id,
        )
    };

    let stream_count = plan.streams.len();
    #[cfg(test)]
    let skip_conductor = SKIP_FLEET_CONDUCTOR_FOR_TEST.get();
    #[cfg(not(test))]
    let skip_conductor = false;
    if !skip_conductor {
        let kernel_for_thread = Arc::clone(kernel);
        std::thread::Builder::new()
            .name(format!("fleet-conductor-{fleet_id}"))
            .spawn(move || {
                // Catch a conductor panic so a fleet never aborts the conductor
                // thread or escapes to poison the kernel - per-stream panics are
                // already reaped on join inside run_fleet.
                if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_fleet(plan, &kernel_for_thread);
                })) {
                    eprintln!("fleet conductor thread panicked: {panic:?}");
                }
            })
            .map_err(|error| format!("could not start the fleet conductor: {error}"))?;
    }

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":{},"status":"FLEET_STARTED","auth_session_status":{},"fleet_id":{},"repo_id":{},"execution_backend":{},"cloud_environment_id":{},"stream_count":{},"run_started_receipt_id":{},"mission":{{"mission_id":{},"milestone_id":{},"checkpoint_route":{},"checkpoint_grants_execution_authority":false}},"projection_route":"/forge/fleet-runs/projection.json","authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(if hosted_execution {
                "mdx-fleet-run-hosted-post"
            } else {
                "mdx-fleet-run-local-post"
            }),
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&fleet_id),
            json_string_literal(&resolved_repo_id),
            json_string_literal(execution_backend),
            json_string_literal(&cloud_environment_id),
            stream_count,
            json_string_literal(&started_receipt_id),
            json_string_literal(&mission_attachment.mission_id),
            json_string_literal(&mission_attachment.milestone_id),
            json_string_literal(mission_attachment.checkpoint_route()),
        ),
    ))
}

fn hosted_execution_requested(
    requested_execution_backend: Option<&str>,
    cloud_environment_id: &str,
) -> Result<bool, &'static str> {
    let requested_execution_backend = requested_execution_backend
        .unwrap_or(if cloud_environment_id.is_empty() {
            "local"
        } else {
            "hosted_sandbox"
        })
        .trim();
    match requested_execution_backend {
        "local" => Ok(false),
        "hosted" | "hosted_sandbox" | "cloud" | "aws_sandbox" => Ok(true),
        _ => Err("execution_backend must be local or hosted_sandbox"),
    }
}

fn prepare_fleet_execution_target(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    repo_id: &str,
    hosted_execution: bool,
    cloud_environment_id: &str,
) -> Result<(String, String), String> {
    if !repo_id.is_empty() {
        let recovered =
            crate::mobile_cloud_route::ensure_managed_cloud_checkout(kernel, tenant_id, repo_id)?;
        if recovered.is_none() {
            crate::forge_repo_route::ensure_managed_remote_checkout(kernel, repo_id)?;
        }
    }
    let repo_root = if repo_id.is_empty() {
        repo_root().to_string_lossy().to_string()
    } else {
        kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?
            .forge_repo_root(repo_id)
            .ok_or_else(|| format!("connect repo {repo_id} before starting this fleet"))?
    };
    if !hosted_execution {
        return Ok((repo_id.to_string(), repo_root));
    }
    if repo_id.is_empty() || cloud_environment_id.is_empty() {
        return Err(
            "hosted fleet execution needs a connected repository and cloud_environment_id"
                .to_string(),
        );
    }
    let guard = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    crate::mobile_cloud_route::require_verified_environment(
        &guard,
        tenant_id,
        cloud_environment_id,
        repo_id,
    )?;
    let root = std::path::Path::new(&repo_root);
    let definition_path = root.join(".mdx/environment.json");
    if std::fs::symlink_metadata(&definition_path).is_err() {
        crate::mobile_cloud_route::restore_verified_environment_definition(
            &guard,
            tenant_id,
            cloud_environment_id,
            repo_id,
            root,
        )?;
    }
    let definition = crate::mobile_hosted_sandbox::load_environment(root)?;
    if definition.environment_id != cloud_environment_id || definition.repository_id != repo_id {
        return Err(
            "hosted fleet execution must name the verified environment for this repository"
                .to_string(),
        );
    }
    Ok((repo_id.to_string(), repo_root))
}

#[derive(Clone, Debug, Default)]
struct MissionAttachment {
    mission_id: String,
    milestone_id: String,
}

impl MissionAttachment {
    fn from_body(parsed: &serde_json::Value) -> Self {
        Self {
            mission_id: parsed["mission_id"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string(),
            milestone_id: parsed["mission_milestone_id"]
                .as_str()
                .or_else(|| parsed["milestone_id"].as_str())
                .unwrap_or("")
                .trim()
                .to_string(),
        }
    }

    fn checkpoint_route(&self) -> &'static str {
        if self.mission_id.trim().is_empty() {
            ""
        } else {
            "/forge/long-horizon-mission-checkpoints.json"
        }
    }

    fn validate(&self, kernel: &MdxKernel) -> Result<(), String> {
        if self.mission_id.trim().is_empty() {
            return Ok(());
        }
        if self.milestone_id.trim().is_empty() {
            return Err(
                "mission-attached fleets need mission_milestone_id so Forge can checkpoint the right milestone"
                    .to_string(),
            );
        }
        let Some(mission) = kernel
            .project_forge_long_horizon_missions()
            .packets
            .into_iter()
            .find(|packet| packet.mission_id == self.mission_id)
        else {
            return Err(format!(
                "mission-attached fleet points at unknown mission {}",
                self.mission_id
            ));
        };
        if mission
            .milestones
            .iter()
            .any(|milestone| milestone.milestone_id == self.milestone_id)
        {
            Ok(())
        } else {
            Err(format!(
                "mission-attached fleet points at unknown milestone {} for mission {}",
                self.milestone_id, self.mission_id
            ))
        }
    }
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
    // Fold the event stream into lanes per fleet: latest state per stream
    // plus the fleet-level start/finish brackets.
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Fleet {
        started: bool,
        finished: bool,
        repo_id: String,
        execution_backend: String,
        cloud_environment_id: String,
        final_detail: String,
        integration_state: String,
        integration_detail: String,
        integration_run_id: String,
        review_verdict: String,
        mission_id: String,
        mission_milestone_id: String,
        mission_checkpoint_route: String,
        lane_repair_started_count: u32,
        lane_repair_finished_count: u32,
        lane_repair_target_count: u32,
        lane_repair_repaired_count: u32,
        lane_repair_detail: String,
        lanes: Vec<(String, String, String, String)>, // stream, state, run id, detail
    }
    let mut fleets: BTreeMap<String, Fleet> = BTreeMap::new();
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
        let fleet = fleets.entry(fleet_id).or_default();
        let event = get("event");
        let stream_id = get("stream_id");
        match event.as_str() {
            "run_started" => {
                fleet.started = true;
                let detail = get("detail");
                fleet.repo_id = detail_field(&detail, "repo_id");
                fleet.execution_backend = detail_field(&detail, "execution_backend");
                if fleet.execution_backend.is_empty() {
                    fleet.execution_backend = "local".to_string();
                }
                fleet.cloud_environment_id = detail_field(&detail, "cloud_environment_id");
                fleet.mission_id = detail_field(&detail, "mission_id");
                fleet.mission_milestone_id = detail_field(&detail, "mission_milestone_id");
                fleet.mission_checkpoint_route = if fleet.mission_id.trim().is_empty() {
                    String::new()
                } else {
                    "/forge/long-horizon-mission-checkpoints.json".to_string()
                };
            }
            "run_finished" => {
                fleet.finished = true;
                fleet.final_detail = get("detail");
            }
            "integration_started" => {
                fleet.integration_state = "working".to_string();
                fleet.integration_detail = get("detail");
            }
            "integration_finished" => {
                let detail = get("detail");
                fleet.integration_state = if detail.starts_with("no branch") {
                    "did_not_land".to_string()
                } else if detail.starts_with("skipped:") {
                    "skipped".to_string()
                } else if detail.contains("wired=false") {
                    "needs_attention".to_string()
                } else {
                    "done".to_string()
                };
                fleet.integration_detail = detail;
                fleet.integration_run_id = get("forge_run_id");
            }
            "review_finished" => fleet.review_verdict = get("detail"),
            "lane_repair_started" => {
                let detail = get("detail");
                fleet.lane_repair_started_count += 1;
                fleet.lane_repair_target_count = detail_field(&detail, "targets")
                    .parse()
                    .unwrap_or(fleet.lane_repair_target_count);
                fleet.lane_repair_detail = detail;
            }
            "lane_repair_finished" => {
                let detail = get("detail");
                fleet.lane_repair_finished_count += 1;
                fleet.lane_repair_target_count = detail_field(&detail, "targets")
                    .parse()
                    .unwrap_or(fleet.lane_repair_target_count);
                fleet.lane_repair_repaired_count = detail_field(&detail, "repaired")
                    .parse()
                    .unwrap_or(fleet.lane_repair_repaired_count);
                fleet.lane_repair_detail = detail;
            }
            "stream_started" | "stream_finished" | "stream_needs_attention" => {
                let state = match event.as_str() {
                    "stream_started" => "working",
                    "stream_finished" => "done",
                    _ => "needs_attention",
                };
                if let Some(lane) = fleet
                    .lanes
                    .iter_mut()
                    .find(|(lane_stream, ..)| lane_stream == &stream_id)
                {
                    lane.1 = state.to_string();
                    lane.3 = get("detail");
                    if !get("forge_run_id").is_empty() {
                        lane.2 = get("forge_run_id");
                    }
                } else {
                    fleet.lanes.push((
                        stream_id,
                        state.to_string(),
                        get("forge_run_id"),
                        get("detail"),
                    ));
                }
            }
            _ => {}
        }
    }
    let rows: Vec<String> = order
        .iter()
        .map(|fleet_id| {
            let fleet = &fleets[fleet_id];
            // Join the plan's casting decisions onto each lane so the operator
            // sees the harness's reasoning, not just the outcome: which coder
            // was cast to this stream and the data-sensitivity tier that
            // governs where it may run. The intelligence is legible, which is
            // half its value - a human ratifying or reviewing sees WHY.
            let casting: std::collections::BTreeMap<String, (String, String)> = kernel
                .latest_fleet_plan(fleet_id)
                .map(|plan| {
                    mdx_core::fleet_plan_streams(&plan)
                        .into_iter()
                        .map(|s| (s.stream_id, (s.builder_slot, s.data_sensitivity)))
                        .collect()
                })
                .unwrap_or_default();
            let plan_fields = kernel.latest_fleet_plan(fleet_id).unwrap_or_default();
            let plan_streams = mdx_core::fleet_plan_streams(&plan_fields);
            let fleet_branch = detail_field(&fleet.final_detail, "fleet_branch");
            let latest_revision = latest_fleet_branch_revision(&kernel, &fleet_branch);
            let latest_lane_revision = latest_repaired_lane_revision(&kernel, &fleet.lanes);
            let lane_repair_attempt_observed = latest_lane_revision.attempt_count > 0;
            let effective_lane_repair_started_count = fleet.lane_repair_started_count.max(
                if lane_repair_attempt_observed { 1 } else { 0 },
            );
            let effective_lane_repair_finished_count = fleet.lane_repair_finished_count.max(
                if lane_repair_attempt_observed { 1 } else { 0 },
            );
            let effective_lane_repair_target_count = fleet.lane_repair_target_count.max(
                if lane_repair_attempt_observed { 1 } else { 0 },
            );
            let lane_revision_repaired = latest_lane_revision.status == "done"
                && latest_lane_revision.checks_passed > 0
                && latest_lane_revision.checks_failed == 0;
            let effective_lane_repair_repaired_count = fleet.lane_repair_repaired_count.max(
                if lane_revision_repaired { 1 } else { 0 },
            );
            let lane_repair_detail = if fleet.lane_repair_detail.is_empty()
                && lane_repair_attempt_observed
            {
                format!(
                    "targets={} repaired={} attempts={} source=runtime_recovery",
                    effective_lane_repair_target_count,
                    effective_lane_repair_repaired_count,
                    latest_lane_revision.attempt_count,
                )
            } else {
                fleet.lane_repair_detail.clone()
            };
            let proof = fleet_proof_packet(FleetProofPacketInput {
                started: fleet.started,
                finished: fleet.finished,
                integration_state: &fleet.integration_state,
                integration_run_id: &fleet.integration_run_id,
                integration_detail: &fleet.integration_detail,
                review_verdict: &fleet.review_verdict,
                latest_revision: &latest_revision,
                latest_lane_revision: &latest_lane_revision,
                lanes: &fleet.lanes,
                kernel: &kernel,
                plan_fields: &plan_fields,
                streams: &plan_streams,
            });
            let lanes: Vec<String> = fleet
                .lanes
                .iter()
                .map(|(stream, state, run, detail)| {
                    let (slot, sensitivity) = casting.get(stream).cloned().unwrap_or_default();
                    let builder_casting = run_builder_casting(&kernel, run);
                    let selected_slot = if builder_casting.selected_slot.is_empty() {
                        slot.as_str()
                    } else {
                        builder_casting.selected_slot.as_str()
                    };
                    let coder = if selected_slot.is_empty() {
                        "default"
                    } else {
                        selected_slot
                    };
                    let planner_coder = if slot.is_empty() { "default" } else { &slot };
                    format!(
                        r#"{{"stream_id":{},"state":{},"forge_run_id":{},"detail":{},"coder":{},"planner_coder":{},"data_sensitivity":{},"builder_casting":{}}}"#,
                        json_string_literal(stream),
                        json_string_literal(state),
                        json_string_literal(run),
                        json_string_literal(detail),
                        json_string_literal(coder),
                        json_string_literal(planner_coder),
                        json_string_literal(if sensitivity.is_empty() {
                            "internal"
                        } else {
                            &sensitivity
                        }),
                        builder_casting.to_json(),
                    )
                })
                .collect();
            format!(
                r#"{{"fleet_id":{},"repo_id":{},"execution_backend":{},"execution_target_kind":{},"cloud_environment_id":{},"goal":{},"checks":{},"running":{},"finished":{},"final_detail":{},"integration_state":{},"integration_detail":{},"review_verdict":{},"latest_revision":{},"latest_lane_revision":{},"runtime_repair":{{"started_count":{},"finished_count":{},"target_count":{},"repaired_count":{},"detail":{},"grants_execution_authority":false}},"mission":{{"mission_id":{},"milestone_id":{},"checkpoint_route":{},"checkpoint_grants_execution_authority":false}},"proof":{},"lanes":[{}]}}"#,
                json_string_literal(fleet_id),
                json_string_literal(&fleet.repo_id),
                json_string_literal(&fleet.execution_backend),
                json_string_literal(if fleet.execution_backend == "hosted_sandbox" {
                    "mdx_cloud"
                } else {
                    "paired_host"
                }),
                json_string_literal(&fleet.cloud_environment_id),
                json_string_literal(plan_fields.get("goal").map(String::as_str).unwrap_or("")),
                serde_json::to_string(&lines_field(&plan_fields, "checks"))
                    .unwrap_or_else(|_| "[]".to_string()),
                fleet.started && !fleet.finished,
                fleet.finished,
                json_string_literal(&fleet.final_detail),
                json_string_literal(&fleet.integration_state),
                json_string_literal(&fleet.integration_detail),
                json_string_literal(&fleet.review_verdict),
                latest_revision.to_json(),
                latest_lane_revision.to_json(),
                effective_lane_repair_started_count,
                effective_lane_repair_finished_count,
                effective_lane_repair_target_count,
                effective_lane_repair_repaired_count,
                json_string_literal(&lane_repair_detail),
                json_string_literal(&fleet.mission_id),
                json_string_literal(&fleet.mission_milestone_id),
                json_string_literal(&fleet.mission_checkpoint_route),
                proof,
                lanes.join(","),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-run-projection","receipt_kind":"fleet.run.event","fleet_count":{},"fleet_runs":[{}],"production_write_allowed":false}}"#,
            rows.len(),
            rows.join(","),
        ),
    ))
}

fn detail_field(detail: &str, key: &str) -> String {
    let needle = format!("{key}=");
    detail
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&needle))
        .unwrap_or("")
        .to_string()
}

#[derive(Clone, Debug, Default)]
struct FleetBranchRevision {
    run_id: String,
    status: String,
    branch: String,
    checks_passed: u32,
    checks_failed: u32,
    attempt_count: u32,
    detail: String,
}

impl FleetBranchRevision {
    fn to_json(&self) -> String {
        format!(
            r#"{{"run_id":{},"status":{},"branch":{},"checks_passed":{},"checks_failed":{},"attempt_count":{},"detail":{},"grants_execution_authority":false}}"#,
            json_string_literal(&self.run_id),
            json_string_literal(&self.status),
            json_string_literal(&self.branch),
            self.checks_passed,
            self.checks_failed,
            self.attempt_count,
            json_string_literal(&self.detail),
        )
    }
}

fn latest_fleet_branch_revision(kernel: &MdxKernel, fleet_branch: &str) -> FleetBranchRevision {
    let fleet_branch = fleet_branch.trim();
    if fleet_branch.is_empty() {
        return FleetBranchRevision::default();
    }
    let mut revision_run_ids = Vec::<String>::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("event").map(String::as_str) != Some("run_started") {
            continue;
        }
        let detail = receipt
            .payload
            .get("detail")
            .map(String::as_str)
            .unwrap_or("");
        if detail_field(detail, "revising") == fleet_branch
            && let Some(run_id) = receipt.payload.get("run_id")
        {
            revision_run_ids.push(run_id.clone());
        }
    }

    let Some(run_id) = revision_run_ids.last() else {
        return FleetBranchRevision::default();
    };
    let (checks_passed, checks_failed) = run_check_counts(kernel, run_id);
    let mut revision = FleetBranchRevision {
        run_id: run_id.clone(),
        status: run_status(kernel, run_id),
        branch: fleet_branch.to_string(),
        checks_passed,
        checks_failed,
        attempt_count: revision_run_ids.len() as u32,
        detail: String::new(),
    };
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id)
            || receipt.payload.get("event").map(String::as_str) != Some("run_finished")
        {
            continue;
        }
        revision.detail = receipt.payload.get("detail").cloned().unwrap_or_default();
    }
    revision
}

fn latest_repaired_lane_revision(
    kernel: &MdxKernel,
    lanes: &[(String, String, String, String)],
) -> FleetBranchRevision {
    let mut latest = FleetBranchRevision::default();
    for (_, state, run_id, _) in lanes {
        if state != "needs_attention" || run_id.trim().is_empty() {
            continue;
        }
        let branch = run_branch(kernel, run_id);
        if branch.trim().is_empty() {
            continue;
        }
        let revision = latest_fleet_branch_revision(kernel, &branch);
        if !revision.run_id.trim().is_empty() {
            latest = revision;
        }
    }
    latest
}

fn run_branch(kernel: &MdxKernel, run_id: &str) -> String {
    let mut branch = String::new();
    let mut run_seen = false;
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
            continue;
        }
        run_seen = true;
        if receipt.payload.get("event").map(String::as_str) != Some("evidence_appended") {
            continue;
        }
        let detail = receipt
            .payload
            .get("detail")
            .map(String::as_str)
            .unwrap_or("");
        if let Some(value) = detail.strip_prefix("branch=") {
            branch = value.split_whitespace().next().unwrap_or("").to_string();
        }
    }
    if branch.is_empty() && run_seen {
        branch = format!("forge/run-{run_id}");
    }
    branch
}

fn repo_root() -> std::path::PathBuf {
    std::env::current_dir()
        .ok()
        .and_then(|dir| {
            dir.ancestors()
                .find(|candidate| candidate.join(".git").exists())
                .map(std::path::Path::to_path_buf)
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

#[derive(Clone, Debug, Default)]
struct RunBuilderCasting {
    status: String,
    requested_slot: String,
    selected_slot: String,
    recommended_slot: String,
    selected_model_profile_id: String,
    selected_provider_family: String,
    selected_model_id: String,
    recommended_model_profile_id: String,
    recommended_provider_family: String,
    recommended_model_id: String,
    basis: String,
    matching_eval_score_count: u32,
    accepted_eval_score_count: u32,
    matching_run_count: u32,
    done_rate_pct: u32,
    requested_slot_matches_evidence: bool,
    grants_execution_authority: bool,
}

impl RunBuilderCasting {
    fn to_json(&self) -> String {
        format!(
            r#"{{"status":{},"requested_slot":{},"selected_slot":{},"selected_model_profile_id":{},"selected_provider_family":{},"selected_model_id":{},"recommended_slot":{},"recommended_model_profile_id":{},"recommended_provider_family":{},"recommended_model_id":{},"basis":{},"matching_eval_score_count":{},"accepted_eval_score_count":{},"matching_run_count":{},"done_rate_pct":{},"requested_slot_matches_evidence":{},"grants_execution_authority":{}}}"#,
            json_string_literal(&self.status),
            json_string_literal(&self.requested_slot),
            json_string_literal(&self.selected_slot),
            json_string_literal(&self.selected_model_profile_id),
            json_string_literal(&self.selected_provider_family),
            json_string_literal(&self.selected_model_id),
            json_string_literal(&self.recommended_slot),
            json_string_literal(&self.recommended_model_profile_id),
            json_string_literal(&self.recommended_provider_family),
            json_string_literal(&self.recommended_model_id),
            json_string_literal(&self.basis),
            self.matching_eval_score_count,
            self.accepted_eval_score_count,
            self.matching_run_count,
            self.done_rate_pct,
            self.requested_slot_matches_evidence,
            self.grants_execution_authority,
        )
    }
}

fn run_builder_casting(kernel: &MdxKernel, run_id: &str) -> RunBuilderCasting {
    if run_id.trim().is_empty() {
        return RunBuilderCasting::default();
    }
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id)
            || receipt.payload.get("event").map(String::as_str) != Some("run_started")
        {
            continue;
        }
        let get = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
        return RunBuilderCasting {
            status: get("builder_casting_status").to_string(),
            requested_slot: get("builder_casting_requested_slot").to_string(),
            selected_slot: get("builder_casting_selected_slot").to_string(),
            recommended_slot: get("builder_casting_recommended_slot").to_string(),
            selected_model_profile_id: get("builder_casting_selected_model_profile_id").to_string(),
            selected_provider_family: get("builder_casting_selected_provider_family").to_string(),
            selected_model_id: get("builder_casting_selected_model_id").to_string(),
            recommended_model_profile_id: get("builder_casting_recommended_model_profile_id")
                .to_string(),
            recommended_provider_family: get("builder_casting_recommended_provider_family")
                .to_string(),
            recommended_model_id: get("builder_casting_recommended_model_id").to_string(),
            basis: get("builder_casting_basis").to_string(),
            matching_eval_score_count: get("builder_casting_matching_eval_score_count")
                .parse()
                .unwrap_or(0),
            accepted_eval_score_count: get("builder_casting_accepted_eval_score_count")
                .parse()
                .unwrap_or(0),
            matching_run_count: get("builder_casting_matching_run_count")
                .parse()
                .unwrap_or(0),
            done_rate_pct: get("builder_casting_done_rate_pct").parse().unwrap_or(0),
            requested_slot_matches_evidence: get("builder_casting_requested_slot_matches_evidence")
                == "true",
            grants_execution_authority: get("builder_casting_grants_execution_authority") == "true",
        };
    }
    RunBuilderCasting::default()
}

struct FleetProofPacketInput<'a> {
    started: bool,
    finished: bool,
    integration_state: &'a str,
    integration_run_id: &'a str,
    integration_detail: &'a str,
    review_verdict: &'a str,
    latest_revision: &'a FleetBranchRevision,
    latest_lane_revision: &'a FleetBranchRevision,
    lanes: &'a [(String, String, String, String)],
    kernel: &'a MdxKernel,
    plan_fields: &'a std::collections::BTreeMap<String, String>,
    streams: &'a [mdx_core::FleetStream],
}

struct FleetRuntimeRecovery<'a> {
    recovery_class: &'a str,
    runtime_responsibility: &'a str,
    harness_responsibility: &'a str,
    next_action: &'a str,
    runtime_superstep: &'a str,
    recovery_health: &'a str,
    retry_policy: &'a str,
    auto_continue_basis: &'a str,
    checkpoint_scope: &'a str,
    interrupt_group: &'a str,
    interrupt_required: bool,
    grouped_interrupt_count: usize,
    can_auto_continue: bool,
    repair_attempt_count: u32,
    max_repair_attempts: u32,
    repair_budget_remaining: u32,
    repair_targets_json: String,
    durable_resume_from: String,
    last_good_run_id: String,
    last_good_branch: String,
}

impl FleetRuntimeRecovery<'_> {
    fn to_json(&self) -> String {
        format!(
            r#"{{"recovery_class":{},"runtime_responsibility":{},"harness_responsibility":{},"next_action":{},"runtime_superstep":{},"recovery_health":{},"retry_policy":{},"auto_continue_basis":{},"checkpoint_scope":{},"interrupt_group":{},"interrupt_required":{},"grouped_interrupt_count":{},"can_auto_continue":{},"repair_attempt_count":{},"max_repair_attempts":{},"repair_budget_remaining":{},"repair_targets":[{}],"durable_resume_from":{},"last_good_run_id":{},"last_good_branch":{},"grants_execution_authority":false}}"#,
            json_string_literal(self.recovery_class),
            json_string_literal(self.runtime_responsibility),
            json_string_literal(self.harness_responsibility),
            json_string_literal(self.next_action),
            json_string_literal(self.runtime_superstep),
            json_string_literal(self.recovery_health),
            json_string_literal(self.retry_policy),
            json_string_literal(self.auto_continue_basis),
            json_string_literal(self.checkpoint_scope),
            json_string_literal(self.interrupt_group),
            self.interrupt_required,
            self.grouped_interrupt_count,
            self.can_auto_continue,
            self.repair_attempt_count,
            self.max_repair_attempts,
            self.repair_budget_remaining,
            self.repair_targets_json,
            json_string_literal(&self.durable_resume_from),
            json_string_literal(&self.last_good_run_id),
            json_string_literal(&self.last_good_branch),
        )
    }
}

fn fleet_proof_packet(input: FleetProofPacketInput<'_>) -> String {
    let FleetProofPacketInput {
        started,
        finished,
        integration_state,
        integration_run_id,
        integration_detail,
        review_verdict,
        latest_revision,
        latest_lane_revision,
        lanes,
        kernel,
        plan_fields,
        streams,
    } = input;
    let value = |key: &str| plan_fields.get(key).map(String::as_str).unwrap_or("");
    let declared_stream_checks = streams
        .iter()
        .flat_map(|stream| stream.checks.iter())
        .filter(|check| !check.trim().is_empty())
        .count();
    let full_suite_checks = lines_field(plan_fields, "full_suite_checks").len();
    let declared_check_count = declared_stream_checks + full_suite_checks;
    let artifact_filter_count = csv_field(plan_fields, "repo_profile_artifact_patterns").len();
    let semantic_signal_count = csv_field(plan_fields, "repo_profile_semantic_intelligence").len()
        + csv_field(plan_fields, "repo_profile_semantic_tool_readiness").len();
    let toolchain_signal_count = csv_field(plan_fields, "repo_profile_toolchain_readiness").len();
    let lane_done_count = lanes
        .iter()
        .filter(|(_, state, _, _)| state == "done")
        .count();
    let lane_attention_count = lanes
        .iter()
        .filter(|(_, state, _, _)| state == "needs_attention")
        .count();
    let terminal_lane_count = lane_done_count + lane_attention_count;
    let stream_count = streams.len();
    let repo_intelligence_recorded = !value("language_pack_id").trim().is_empty()
        && !value("repo_primary_language").trim().is_empty();
    let disjoint_streams_validated = stream_count > 0;
    let proof_commands_declared = declared_check_count > 0;
    let artifact_filters_recorded = artifact_filter_count > 0;
    let semantic_orientation_declared = semantic_signal_count > 0;
    let integration_reviewed = !review_verdict.trim().is_empty();
    let integration_review_ready = integration_reviewed
        && !review_verdict
            .to_ascii_lowercase()
            .contains("verdict: needs work")
        && !review_verdict
            .to_ascii_lowercase()
            .contains("review unavailable");
    let review_needs_work = integration_reviewed && !integration_review_ready;
    let revision_repaired = latest_revision.status == "done"
        && latest_revision.checks_passed > 0
        && latest_revision.checks_failed == 0;
    let lane_revision_repaired = latest_lane_revision.status == "done"
        && latest_lane_revision.checks_passed > 0
        && latest_lane_revision.checks_failed == 0;
    let all_streams_terminal = stream_count > 0 && terminal_lane_count >= stream_count;
    let runtime_recovery = fleet_runtime_recovery(FleetRuntimeRecoveryInput {
        started,
        finished,
        integration_state,
        integration_run_id,
        lane_attention_count,
        all_streams_terminal,
        review_needs_work,
        integration_review_ready,
        revision_repaired,
        lane_revision_repaired,
        latest_revision,
        latest_lane_revision,
        lanes,
        kernel,
    });
    let semantic_strategy =
        fleet_semantic_strategy_rollup(kernel, lanes, integration_run_id, integration_detail);
    let fleet_selection_blocked_reason = if lane_attention_count > 0 && lane_revision_repaired {
        "lane_revision_requires_integration"
    } else if lane_attention_count > 0 {
        "streams_need_attention"
    } else if started && !all_streams_terminal {
        "streams_not_terminal"
    } else {
        ""
    };
    let fleet_lane_selection =
        fleet_lane_selection_packet(kernel, lanes, fleet_selection_blocked_reason);

    let mut missing = Vec::<String>::new();
    if !repo_intelligence_recorded {
        missing.push("repo_intelligence".to_string());
    }
    if !proof_commands_declared {
        missing.push("proof_commands".to_string());
    }
    if !artifact_filters_recorded {
        missing.push("artifact_filters".to_string());
    }
    if !semantic_orientation_declared {
        missing.push("semantic_orientation".to_string());
    }
    if started && !all_streams_terminal {
        missing.push("all_streams_terminal".to_string());
    }
    if finished && !integration_reviewed {
        missing.push("integration_review".to_string());
    }
    if finished && review_needs_work && !revision_repaired {
        missing.push("integration_review_needs_work".to_string());
    } else if finished && review_needs_work && revision_repaired {
        missing.push("revision_review_required".to_string());
    }
    if lane_attention_count > 0 && !lane_revision_repaired {
        missing.push("streams_need_attention".to_string());
    } else if lane_attention_count > 0 && lane_revision_repaired {
        missing.push("lane_revision_review_required".to_string());
    }
    if semantic_strategy.assigned_count > 0
        && semantic_strategy.satisfied_count < semantic_strategy.assigned_count
    {
        missing.push("semantic_strategy_operations".to_string());
    }

    let status = if !started {
        "planned_waiting_for_start"
    } else if lane_attention_count > 0 && lane_revision_repaired {
        "lane_revision_ready_for_integration"
    } else if lane_attention_count > 0 {
        "needs_attention"
    } else if finished && review_needs_work && revision_repaired {
        "revision_ready_for_review"
    } else if finished && review_needs_work {
        "needs_principal_attention"
    } else if finished && integration_review_ready {
        "ready_for_principal_review"
    } else if all_streams_terminal && integration_state != "done" {
        "waiting_for_integration"
    } else {
        "running_or_waiting"
    };

    format!(
        r#"{{"status":{},"repo_id":{},"repo_primary_language":{},"language_pack_id":{},"repo_profile_source":{},"stream_count":{},"lane_done_count":{},"lane_attention_count":{},"declared_check_count":{},"artifact_filter_count":{},"semantic_signal_count":{},"toolchain_signal_count":{},"semantic_strategy_assignment_count":{},"semantic_strategy_satisfied_count":{},"semantic_strategy_missing_operations":[{}],"execution_geometry":{},"runtime_recovery":{},"fleet_lane_selection":{},"repo_intelligence_recorded":{},"disjoint_streams_validated":{},"proof_commands_declared":{},"artifact_filters_recorded":{},"semantic_orientation_declared":{},"integration_reviewed":{},"revision_repaired":{},"lane_revision_repaired":{},"principal_engineer_evidence_ready":{},"missing_gates":[{}],"production_write_allowed":false}}"#,
        json_string_literal(status),
        json_string_literal(value("repo_id")),
        json_string_literal(value("repo_primary_language")),
        json_string_literal(value("language_pack_id")),
        json_string_literal(value("repo_profile_source")),
        stream_count,
        lane_done_count,
        lane_attention_count,
        declared_check_count,
        artifact_filter_count,
        semantic_signal_count,
        toolchain_signal_count,
        semantic_strategy.assigned_count,
        semantic_strategy.satisfied_count,
        semantic_strategy
            .missing_operations
            .iter()
            .map(|item| json_string_literal(item))
            .collect::<Vec<_>>()
            .join(","),
        fleet_execution_geometry_packet(streams, plan_fields, lanes),
        runtime_recovery.to_json(),
        fleet_lane_selection,
        repo_intelligence_recorded,
        disjoint_streams_validated,
        proof_commands_declared,
        artifact_filters_recorded,
        semantic_orientation_declared,
        integration_reviewed,
        revision_repaired,
        lane_revision_repaired,
        finished
            && integration_review_ready
            && lane_attention_count == 0
            && (semantic_strategy.assigned_count == 0
                || semantic_strategy.satisfied_count == semantic_strategy.assigned_count),
        missing
            .iter()
            .map(|gate| json_string_literal(gate))
            .collect::<Vec<_>>()
            .join(","),
    )
}

struct FleetRuntimeRecoveryInput<'a> {
    started: bool,
    finished: bool,
    integration_state: &'a str,
    integration_run_id: &'a str,
    lane_attention_count: usize,
    all_streams_terminal: bool,
    review_needs_work: bool,
    integration_review_ready: bool,
    revision_repaired: bool,
    lane_revision_repaired: bool,
    latest_revision: &'a FleetBranchRevision,
    latest_lane_revision: &'a FleetBranchRevision,
    lanes: &'a [(String, String, String, String)],
    kernel: &'a MdxKernel,
}

fn fleet_runtime_recovery(input: FleetRuntimeRecoveryInput<'_>) -> FleetRuntimeRecovery<'static> {
    let (last_good_run_id, last_good_branch) = if input.lane_revision_repaired {
        (
            input.latest_lane_revision.run_id.clone(),
            input.latest_lane_revision.branch.clone(),
        )
    } else if input.revision_repaired {
        (
            input.latest_revision.run_id.clone(),
            input.latest_revision.branch.clone(),
        )
    } else {
        (String::new(), String::new())
    };
    let repair_attempt_count = if input.lane_attention_count > 0 || input.lane_revision_repaired {
        input.latest_lane_revision.attempt_count
    } else if input.review_needs_work || input.revision_repaired {
        input.latest_revision.attempt_count
    } else {
        0
    };
    let repair_budget_remaining = if input.revision_repaired
        || input.lane_revision_repaired
        || input.review_needs_work
        || input.lane_attention_count > 0
    {
        FLEET_RUNTIME_MAX_REPAIR_ATTEMPTS.saturating_sub(repair_attempt_count)
    } else {
        0
    };
    let repair_budget_exhausted =
        repair_attempt_count >= FLEET_RUNTIME_MAX_REPAIR_ATTEMPTS && repair_budget_remaining == 0;
    let repair_targets_json = recovery_targets_json(
        input.kernel,
        input.lanes,
        input.latest_revision,
        input.review_needs_work,
        input.lane_attention_count,
    );

    let (
        recovery_class,
        runtime_responsibility,
        harness_responsibility,
        next_action,
        runtime_superstep,
        recovery_health,
        retry_policy,
        auto_continue_basis,
        checkpoint_scope,
        interrupt_group,
        interrupt_required,
    ) = if !input.started {
        (
            "not_started",
            "fleet_runtime",
            "harness",
            "start_after_human_ratification",
            "plan_start",
            "healthy",
            "none",
            "operator_ratification_required",
            "fleet_plan",
            "none",
            false,
        )
    } else if input.lane_attention_count > 0 && input.lane_revision_repaired {
        (
            "lane_revision_ready",
            "fleet_runtime",
            "harness",
            "rerun_integration_after_lane_repair",
            "lane_repair_integration",
            "degraded_recoverable",
            "bounded_repair_then_interrupt",
            "repaired_lane_ready",
            "lane_branch",
            "attention_lanes",
            false,
        )
    } else if input.lane_attention_count > 0 && repair_budget_exhausted {
        (
            "lane_repair_exhausted",
            "human_interrupt",
            "principal_review",
            "group_attention_lanes_for_human_unblock",
            "human_unblock",
            "exhausted",
            "bounded_repair_then_interrupt",
            "repair_budget_exhausted",
            "lane_branch",
            "attention_lanes",
            true,
        )
    } else if input.lane_attention_count > 0 {
        (
            "lane_repair_needed",
            "fleet_runtime",
            "harness",
            "repair_attention_lanes_before_failing_fleet",
            "lane_repair",
            "degraded_recoverable",
            "bounded_repair_then_interrupt",
            "repair_budget_available",
            "lane_branch",
            "attention_lanes",
            false,
        )
    } else if input.finished && input.review_needs_work && input.revision_repaired {
        (
            "integration_revision_ready",
            "human_interrupt",
            "principal_review",
            "review_repaired_integration",
            "principal_review",
            "degraded_reviewable",
            "bounded_repair_then_interrupt",
            "repaired_integration_ready",
            "fleet_branch",
            "integration_review",
            true,
        )
    } else if input.finished && input.review_needs_work && repair_budget_exhausted {
        (
            "integration_repair_exhausted",
            "human_interrupt",
            "principal_review",
            "group_integration_findings_for_human_unblock",
            "human_unblock",
            "exhausted",
            "bounded_repair_then_interrupt",
            "repair_budget_exhausted",
            "fleet_branch",
            "integration_review",
            true,
        )
    } else if input.finished && input.review_needs_work {
        (
            "integration_repair_needed",
            "fleet_runtime",
            "harness",
            "repair_integration_branch_before_failing_fleet",
            "integration_repair",
            "degraded_recoverable",
            "bounded_repair_then_interrupt",
            "repair_budget_available",
            "fleet_branch",
            "integration_review",
            false,
        )
    } else if input.finished && input.integration_review_ready {
        (
            "ready_for_human_review",
            "human_interrupt",
            "principal_review",
            "review_integrated_branch",
            "principal_review",
            "healthy_reviewable",
            "none",
            "human_review_required",
            "fleet_branch",
            "principal_review",
            true,
        )
    } else if input.all_streams_terminal && input.integration_state != "done" {
        (
            "integration_needed",
            "fleet_runtime",
            "harness",
            "integrate_terminal_lanes",
            "integration",
            "healthy",
            "durable_resume",
            "terminal_lanes_ready",
            "fleet_branch",
            "none",
            false,
        )
    } else if input.integration_state == "working" || !input.integration_run_id.trim().is_empty() {
        (
            "integration_running",
            "fleet_runtime",
            "harness",
            "wait_for_integration_result",
            "integration",
            "healthy",
            "durable_resume",
            "integration_in_progress",
            "fleet_branch",
            "none",
            false,
        )
    } else {
        (
            "lanes_running",
            "fleet_runtime",
            "harness",
            "wait_for_lane_results",
            "lane_execution",
            "healthy",
            "durable_resume",
            "lanes_in_progress",
            "fleet_run",
            "none",
            false,
        )
    };

    FleetRuntimeRecovery {
        recovery_class,
        runtime_responsibility,
        harness_responsibility,
        next_action,
        runtime_superstep,
        recovery_health,
        retry_policy,
        auto_continue_basis,
        checkpoint_scope,
        interrupt_group,
        interrupt_required,
        grouped_interrupt_count: if !interrupt_required {
            0
        } else if interrupt_group == "attention_lanes" {
            input.lane_attention_count
        } else {
            1
        },
        can_auto_continue: !interrupt_required
            && runtime_responsibility == "fleet_runtime"
            && (retry_policy != "bounded_repair_then_interrupt"
                || repair_budget_remaining > 0
                || recovery_class == "lane_revision_ready"),
        repair_attempt_count,
        max_repair_attempts: FLEET_RUNTIME_MAX_REPAIR_ATTEMPTS,
        repair_budget_remaining,
        repair_targets_json,
        durable_resume_from: if last_good_branch.trim().is_empty() {
            checkpoint_scope.to_string()
        } else {
            last_good_branch.clone()
        },
        last_good_run_id,
        last_good_branch,
    }
}

fn recovery_targets_json(
    kernel: &MdxKernel,
    lanes: &[(String, String, String, String)],
    latest_revision: &FleetBranchRevision,
    review_needs_work: bool,
    lane_attention_count: usize,
) -> String {
    if lane_attention_count > 0 {
        return lanes
            .iter()
            .filter(|(_, state, run_id, _)| state == "needs_attention" && !run_id.trim().is_empty())
            .map(|(stream_id, _, run_id, detail)| {
                let branch = run_branch(kernel, run_id);
                let revision = latest_fleet_branch_revision(kernel, &branch);
                format!(
                    r#"{{"target_type":"lane","stream_id":{},"run_id":{},"branch":{},"status":{},"attempt_count":{},"detail":{},"grants_execution_authority":false}}"#,
                    json_string_literal(stream_id),
                    json_string_literal(run_id),
                    json_string_literal(&branch),
                    json_string_literal(&run_status(kernel, run_id)),
                    revision.attempt_count,
                    json_string_literal(detail),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
    }
    if review_needs_work && !latest_revision.branch.trim().is_empty() {
        return format!(
            r#"{{"target_type":"integration","stream_id":"","run_id":{},"branch":{},"status":{},"attempt_count":{},"detail":{},"grants_execution_authority":false}}"#,
            json_string_literal(&latest_revision.run_id),
            json_string_literal(&latest_revision.branch),
            json_string_literal(&latest_revision.status),
            latest_revision.attempt_count,
            json_string_literal(&latest_revision.detail),
        );
    }
    String::new()
}

#[derive(Default)]
struct FleetSemanticStrategyRollup {
    assigned_count: usize,
    satisfied_count: usize,
    missing_operations: Vec<String>,
}

fn fleet_semantic_strategy_rollup(
    kernel: &MdxKernel,
    lanes: &[(String, String, String, String)],
    integration_run_id: &str,
    integration_detail: &str,
) -> FleetSemanticStrategyRollup {
    let mut rollup = FleetSemanticStrategyRollup::default();
    for (stream_id, _, run_id, _) in lanes {
        add_run_strategy_evidence(
            kernel,
            run_id,
            "fleet_stream_required_semantic_operations",
            stream_id,
            &mut rollup,
        );
    }
    if !integration_run_id.trim().is_empty() && !integration_detail.contains("wiring=not_needed") {
        add_run_strategy_evidence(
            kernel,
            integration_run_id,
            "fleet_integration_required_semantic_operations",
            "integration",
            &mut rollup,
        );
    }
    rollup.missing_operations.sort();
    rollup.missing_operations.dedup();
    rollup
}

fn add_run_strategy_evidence(
    kernel: &MdxKernel,
    run_id: &str,
    required_field: &str,
    label: &str,
    rollup: &mut FleetSemanticStrategyRollup,
) {
    if run_id.trim().is_empty() {
        return;
    }
    let Some(missing) = run_strategy_missing_operations(kernel, run_id, required_field) else {
        return;
    };
    rollup.assigned_count += 1;
    if missing.is_empty() {
        rollup.satisfied_count += 1;
    } else {
        rollup.missing_operations.extend(
            missing
                .into_iter()
                .map(|operation| format!("{label}:{operation}")),
        );
    }
}

fn run_strategy_missing_operations(
    kernel: &MdxKernel,
    run_id: &str,
    required_field: &str,
) -> Option<Vec<String>> {
    let mut required = Vec::<String>::new();
    let mut observed = Vec::<String>::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
            continue;
        }
        if receipt.payload.get("event").map(String::as_str) == Some("run_started") {
            required.extend(lines_from_payload(receipt.payload.get(required_field)));
        }
        if receipt.payload.get("event").map(String::as_str) == Some("tool_executed")
            && let Some(operation) = semantic_operation_from_detail(
                receipt
                    .payload
                    .get("detail")
                    .map(String::as_str)
                    .unwrap_or(""),
            )
        {
            observed.push(operation);
        }
    }
    required.sort();
    required.dedup();
    observed.sort();
    observed.dedup();
    if required.is_empty() {
        return None;
    }
    Some(
        required
            .iter()
            .filter(|operation| !observed.iter().any(|seen| seen == *operation))
            .cloned()
            .collect::<Vec<_>>(),
    )
}

fn run_status(kernel: &MdxKernel, run_id: &str) -> String {
    let mut status = "working".to_string();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id)
            || receipt.payload.get("event").map(String::as_str) != Some("run_finished")
        {
            continue;
        }
        let detail = receipt
            .payload
            .get("detail")
            .map(String::as_str)
            .unwrap_or("");
        let upper = detail.to_ascii_uppercase();
        status = if upper.contains("RUN_FINISHED_DONE") {
            "done".to_string()
        } else if upper.contains("RUN_FINISHED_NO_CHANGE") {
            "no_change".to_string()
        } else if upper.contains("CANNOT_PROCEED") {
            "cannot_proceed".to_string()
        } else if upper.contains("BUDGET_EXHAUSTED") {
            "budget_exhausted".to_string()
        } else if upper.contains("ERROR") || upper.contains("FAILED") {
            "error".to_string()
        } else {
            "finished".to_string()
        };
    }
    status
}

fn run_check_counts(kernel: &MdxKernel, run_id: &str) -> (u32, u32) {
    let mut passed = 0;
    let mut failed = 0;
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
            continue;
        }
        match receipt.payload.get("event").map(String::as_str) {
            Some("check_passed") => passed += 1,
            Some("check_failed") => failed += 1,
            _ => {}
        }
    }
    (passed, failed)
}

fn fleet_lane_selection_packet(
    kernel: &MdxKernel,
    lanes: &[(String, String, String, String)],
    blocked_reason: &str,
) -> String {
    let mut candidates = lanes
        .iter()
        .filter(|(_, _, run_id, _)| !run_id.trim().is_empty())
        .map(|(stream_id, state, run_id, detail)| {
            let builder_casting = run_builder_casting(kernel, run_id);
            let semantic_missing_result = run_strategy_missing_operations(
                kernel,
                run_id,
                "fleet_stream_required_semantic_operations",
            );
            let semantic_strategy_assigned = semantic_missing_result.is_some();
            let missing_semantic_operations = semantic_missing_result.unwrap_or_default();
            let semantic_strategy_satisfied = missing_semantic_operations.is_empty();
            let status = run_status(kernel, run_id);
            let (checks_passed, checks_failed) = run_check_counts(kernel, run_id);
            let score = fleet_lane_score(
                state,
                &status,
                checks_passed,
                checks_failed,
                semantic_strategy_assigned,
                &missing_semantic_operations,
                &builder_casting,
            );
            (
                -score,
                stream_id.clone(),
                run_id.clone(),
                format!(
                    r#"{{"stream_id":{},"state":{},"forge_run_id":{},"detail":{},"status":{},"checks_passed":{},"checks_failed":{},"score":{},"semantic_strategy_operations_satisfied":{},"missing_semantic_strategy_operations":[{}],"builder_casting":{},"grants_execution_authority":false}}"#,
                    json_string_literal(stream_id),
                    json_string_literal(state),
                    json_string_literal(run_id),
                    json_string_literal(detail),
                    json_string_literal(&status),
                    checks_passed,
                    checks_failed,
                    score,
                    semantic_strategy_satisfied,
                    missing_semantic_operations
                        .iter()
                        .map(|operation| json_string_literal(operation))
                        .collect::<Vec<_>>()
                        .join(","),
                    builder_casting.to_json(),
                ),
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let mut recommended_run_id = String::new();
    let mut recommended_stream_id = String::new();
    let selection_blocked = !blocked_reason.trim().is_empty();
    let candidate_json = candidates
        .into_iter()
        .enumerate()
        .map(|(rank, (_, stream_id, run_id, mut candidate))| {
            if rank == 0 && !selection_blocked {
                recommended_stream_id = stream_id;
                recommended_run_id = run_id;
            }
            candidate.insert_str(candidate.len() - 1, &format!(r#","rank":{}"#, rank + 1));
            candidate
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"generated_from":"fleet.run.event.stream_lanes","selection_required_for_delivery":{},"selection_blocked_reason":{},"candidate_count":{},"recommended_stream_id":{},"recommended_run_id":{},"comparison_basis":["fleet_lane_state","run_terminal_status","selected_check_events","fleet_stream_semantic_strategy","builder_casting_evidence"],"candidates":[{}],"model_judgment_included":false,"grants_execution_authority":false}}"#,
        candidate_json.len() > 1 && !selection_blocked,
        json_string_literal(blocked_reason),
        candidate_json.len(),
        json_string_literal(&recommended_stream_id),
        json_string_literal(&recommended_run_id),
        candidate_json.join(","),
    )
}

fn fleet_lane_score(
    lane_state: &str,
    run_status: &str,
    checks_passed: u32,
    checks_failed: u32,
    semantic_strategy_assigned: bool,
    missing_semantic_operations: &[String],
    builder_casting: &RunBuilderCasting,
) -> i64 {
    let state_score = match lane_state {
        "done" => 100,
        "working" => 25,
        "needs_attention" => -80,
        _ => 0,
    };
    let run_score = match run_status {
        "done" => 80,
        "finished" => 55,
        "working" => 20,
        "cannot_proceed" | "budget_exhausted" | "error" => -70,
        _ => 0,
    };
    let semantic_score = if !semantic_strategy_assigned {
        0
    } else if missing_semantic_operations.is_empty() {
        35
    } else {
        -(missing_semantic_operations.len() as i64 * 18)
    };
    let builder_score = (builder_casting.accepted_eval_score_count as i64 * 8)
        + (builder_casting.matching_eval_score_count as i64 * 3)
        + (builder_casting.matching_run_count as i64 * 2)
        + (builder_casting.done_rate_pct as i64 / 10);
    state_score + run_score + (checks_passed as i64 * 18) - (checks_failed as i64 * 25)
        + semantic_score
        + builder_score.min(35)
}

fn lines_from_payload(value: Option<&String>) -> Vec<String> {
    value
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

fn semantic_operation_from_detail(detail: &str) -> Option<String> {
    if !detail.contains("semantic_query") || !detail.contains("status=OK") {
        return None;
    }
    detail.split_whitespace().find_map(|token| {
        token
            .strip_prefix("operation=")
            .map(str::trim)
            .filter(|operation| !operation.is_empty())
            .map(str::to_string)
    })
}

fn fleet_execution_geometry_packet(
    streams: &[mdx_core::FleetStream],
    plan_fields: &std::collections::BTreeMap<String, String>,
    lanes: &[(String, String, String, String)],
) -> String {
    use std::collections::BTreeMap;
    let integration_owned = lines_field(plan_fields, "integration_owned_paths");
    let full_suite_checks = lines_field(plan_fields, "full_suite_checks");
    let mut by_coder: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_sensitivity: BTreeMap<String, usize> = BTreeMap::new();
    for stream in streams {
        let coder = if stream.builder_slot.trim().is_empty() {
            "default".to_string()
        } else {
            stream.builder_slot.trim().to_string()
        };
        *by_coder.entry(coder).or_default() += 1;
        let sensitivity = if stream.data_sensitivity.trim().is_empty() {
            "internal".to_string()
        } else {
            stream.data_sensitivity.trim().to_ascii_lowercase()
        };
        *by_sensitivity.entry(sensitivity).or_default() += 1;
    }
    let stream_check_count = streams
        .iter()
        .flat_map(|stream| stream.checks.iter())
        .filter(|check| !check.trim().is_empty())
        .count();
    let dependency_edge_count = streams
        .iter()
        .flat_map(|stream| stream.depends_on.iter())
        .filter(|dependency| !dependency.trim().is_empty())
        .count();
    let started_lane_count = lanes
        .iter()
        .filter(|(_, state, _, _)| state != "waiting")
        .count();
    let requested_width = plan_fields
        .get("requested_width")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(streams.len() as u32)
        .max(1);
    let max_concurrent = crate::fleet_executor::pool_status().capacity;
    format!(
        r#"{{"strategy":"parallel_streams_plus_integration","requested_width":{},"stream_count":{},"started_lane_count":{},"max_concurrent_workers":{},"disjoint_write_scopes_validated":{},"integration_owned_path_count":{},"stream_check_count":{},"full_suite_check_count":{},"dependency_edge_count":{},"max_turn_budget":{},"by_coder":{},"by_sensitivity":{},"production_write_allowed":false}}"#,
        requested_width,
        streams.len(),
        started_lane_count,
        max_concurrent,
        mdx_core::validate_fleet_streams(streams, &integration_owned).is_ok(),
        integration_owned.len(),
        stream_check_count,
        full_suite_checks
            .iter()
            .filter(|check| !check.trim().is_empty())
            .count(),
        dependency_edge_count,
        streams.iter().map(|stream| stream.max_turns).sum::<u32>(),
        count_object(&by_coder),
        count_object(&by_sensitivity),
    )
}

fn count_object(map: &std::collections::BTreeMap<String, usize>) -> String {
    let pairs = map
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_string_literal(key)))
        .collect::<Vec<_>>();
    format!("{{{}}}", pairs.join(","))
}

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

fn csv_field(plan_fields: &std::collections::BTreeMap<String, String>, key: &str) -> Vec<String> {
    plan_fields
        .get(key)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn fleet_run_repo_id(
    requested_repo_id: &str,
    plan_fields: &std::collections::BTreeMap<String, String>,
) -> String {
    let requested_repo_id = requested_repo_id.trim();
    if !requested_repo_id.is_empty() {
        normalize_local_runtime_repo_id(requested_repo_id)
    } else {
        plan_fields
            .get("repo_id")
            .map(|value| normalize_local_runtime_repo_id(value))
            .unwrap_or_default()
    }
}

fn normalize_local_runtime_repo_id(repo_id: &str) -> String {
    let repo_id = repo_id.trim();
    if repo_id == "mdx" {
        String::new()
    } else {
        repo_id.to_string()
    }
}

fn validate_wide_plan_review(
    plan_fields: &std::collections::BTreeMap<String, String>,
    requested_width: u32,
) -> Result<(), String> {
    if requested_width < WIDE_FLEET_REVIEW_REQUIRED_WIDTH {
        return Ok(());
    }
    let review_required = plan_fields
        .get("wide_plan_review_required")
        .map(String::as_str)
        == Some("true");
    let review_status = plan_fields
        .get("wide_plan_review_status")
        .map(String::as_str)
        .unwrap_or("");
    let review_verdict = plan_fields
        .get("wide_plan_review_verdict")
        .map(String::as_str)
        .unwrap_or("");
    if review_required && review_status == "recorded" && review_verdict == "ready" {
        return Ok(());
    }
    Err(format!(
        "wide fleet execution requires a recorded ready plan review before starting; status={review_status} verdict={review_verdict}"
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-run-local-post","status":"REFUSED","reason":{},"run_started_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{FleetPlanDraft, FleetStream, GovernedWriteIdentity};
    use std::collections::BTreeMap;

    #[test]
    fn fleet_start_inherits_repo_id_from_plan_unless_operator_overrides() {
        let mut plan = BTreeMap::new();
        plan.insert("repo_id".to_string(), "swift-canary".to_string());

        assert_eq!(fleet_run_repo_id("", &plan), "swift-canary");
        assert_eq!(fleet_run_repo_id("java-service", &plan), "java-service");
    }

    #[test]
    fn fleet_start_treats_mdx_alias_as_local_runtime_workspace() {
        let mut plan = BTreeMap::new();
        plan.insert("repo_id".to_string(), "swift-canary".to_string());

        assert_eq!(fleet_run_repo_id("mdx", &plan), "");

        plan.insert("repo_id".to_string(), "mdx".to_string());
        assert_eq!(fleet_run_repo_id("", &plan), "");
    }

    #[test]
    fn fleet_start_refuses_unknown_execution_backend_before_repo_preparation() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_start(
            "POST",
            r#"{"fleet_id":"fleet_unknown_backend","execution_backend":"render_shell"}"#,
            &kernel,
        )
        .expect("response");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("local or hosted_sandbox"));
    }

    #[test]
    fn cloud_environment_selects_hosted_execution_unless_local_is_explicit() {
        assert!(hosted_execution_requested(None, "env_cloud_beta").unwrap());
        assert!(!hosted_execution_requested(None, "").unwrap());
        assert!(!hosted_execution_requested(Some("local"), "env_cloud_beta").unwrap());
        assert!(hosted_execution_requested(Some("cloud"), "env_cloud_beta").unwrap());
        assert!(hosted_execution_requested(Some("render_shell"), "").is_err());
    }

    #[test]
    fn fleet_lane_selection_does_not_recommend_delivery_when_a_lane_needs_attention() {
        let kernel = MdxKernel::boot_local();
        let lanes = vec![
            (
                "s1".to_string(),
                "done".to_string(),
                "forge_run_green".to_string(),
                "status=RUN_FINISHED_DONE done=1/2".to_string(),
            ),
            (
                "s2".to_string(),
                "needs_attention".to_string(),
                "forge_run_red".to_string(),
                "status=RUN_BUDGET_EXHAUSTED done=2/2".to_string(),
            ),
        ];

        let packet = fleet_lane_selection_packet(&kernel, &lanes, "streams_need_attention");

        assert!(packet.contains(r#""selection_required_for_delivery":false"#));
        assert!(packet.contains(r#""selection_blocked_reason":"streams_need_attention""#));
        assert!(packet.contains(r#""recommended_stream_id":"""#));
        assert!(packet.contains(r#""candidate_count":2"#));
    }

    #[test]
    fn wide_fleet_start_requires_recorded_ready_plan_review() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            let streams = vec![FleetStream {
                stream_id: "s1".to_string(),
                objective: "Build one wide lane".to_string(),
                write_scope: vec!["src/lib.rs".to_string()],
                interface_contract: "keep API stable".to_string(),
                depends_on: Vec::new(),
                checks: vec!["cargo test".to_string()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            }];
            kernel
                .record_fleet_plan_draft(
                    FleetPlanDraft {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: "wide_missing_review",
                        spec: "wide fleet with a ready review",
                        goal: "wide fleet with a ready review",
                        checks: &["cargo test".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["cargo test".to_string()],
                        justification: "operator asked for width eight",
                        requested_width: 8,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "",
                        repo_primary_language: "rust",
                        language_pack_id: "rust-cargo",
                        repo_profile_suggested_checks: "cargo test",
                        repo_profile_artifact_patterns: "target/**",
                        repo_profile_semantic_intelligence: "rust-analyzer",
                        repo_profile_semantic_tool_readiness: "semantic_query:available",
                        repo_profile_toolchain_readiness: "cargo:test-ready",
                        repo_profile_proof_plan_status: "ready",
                        repo_profile_proof_plan_summary: "Use cargo test.",
                        repo_profile_source: "test",
                        wide_plan_review_required: true,
                        wide_plan_review_status: "recorded",
                        wide_plan_review_reviewer_model: "advisor-test",
                        wide_plan_review_verdict: "ready",
                        wide_plan_review_confidence: "high",
                        wide_plan_review_concerns: "",
                    },
                    &streams,
                )
                .expect("draft");
            kernel
                .ratify_fleet_plan_with_identity(
                    "t",
                    "human:eng",
                    "wide_missing_review",
                    "ratified with recorded review",
                    &GovernedWriteIdentity::local_demo("human:eng"),
                )
                .expect("ratify");
            kernel
                .record_fleet_plan_draft(
                    FleetPlanDraft {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: "wide_missing_review",
                        spec: "wide fleet with stale ratification",
                        goal: "wide fleet with stale ratification",
                        checks: &["cargo test".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["cargo test".to_string()],
                        justification: "operator asked for width eight",
                        requested_width: 8,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "",
                        repo_primary_language: "rust",
                        language_pack_id: "rust-cargo",
                        repo_profile_suggested_checks: "cargo test",
                        repo_profile_artifact_patterns: "target/**",
                        repo_profile_semantic_intelligence: "rust-analyzer",
                        repo_profile_semantic_tool_readiness: "semantic_query:available",
                        repo_profile_toolchain_readiness: "cargo:test-ready",
                        repo_profile_proof_plan_status: "ready",
                        repo_profile_proof_plan_summary: "Use cargo test.",
                        repo_profile_source: "test",
                        wide_plan_review_required: true,
                        wide_plan_review_status: "missing_reviewer",
                        wide_plan_review_reviewer_model: "",
                        wide_plan_review_verdict: "not_recorded",
                        wide_plan_review_confidence: "none",
                        wide_plan_review_concerns: "reviewer missing",
                    },
                    &streams,
                )
                .expect("draft");
        }

        let response = handle_start(
            "POST",
            r#"{"fleet_id":"wide_missing_review","actor_id":"human:eng"}"#,
            &kernel,
        )
        .expect("start response");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("recorded ready plan review"));
        assert!(response.body.contains("status=missing_reviewer"));
        assert!(response.body.contains("verdict=not_recorded"));
    }

    #[test]
    fn fleet_projection_exposes_mission_attachment() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            let streams = vec![FleetStream {
                stream_id: "s1".to_string(),
                objective: "Build one slice".to_string(),
                write_scope: vec!["src/lib.rs".to_string()],
                interface_contract: "keep API stable".to_string(),
                depends_on: Vec::new(),
                checks: vec!["cargo test".to_string()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            }];
            kernel
                .record_fleet_plan_draft(
                    FleetPlanDraft {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: "fleet_mission",
                        spec: "fleet mission test",
                        goal: "fleet mission test",
                        checks: &["cargo test".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["cargo test".to_string()],
                        justification: "one stream",
                        requested_width: 1,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "mdx",
                        repo_primary_language: "rust",
                        language_pack_id: "rust-cargo",
                        repo_profile_suggested_checks: "cargo test",
                        repo_profile_artifact_patterns: "target/**",
                        repo_profile_semantic_intelligence: "rust-analyzer",
                        repo_profile_semantic_tool_readiness: "semantic_query:available",
                        repo_profile_toolchain_readiness: "cargo:test-ready",
                        repo_profile_proof_plan_status: "ready",
                        repo_profile_proof_plan_summary: "Use cargo test.",
                        repo_profile_source: "test",
                        wide_plan_review_required: false,
                        wide_plan_review_status: "not_required",
                        wide_plan_review_reviewer_model: "",
                        wide_plan_review_verdict: "not_required",
                        wide_plan_review_confidence: "none",
                        wide_plan_review_concerns: "",
                    },
                    &streams,
                )
                .expect("draft");
            kernel
                .ratify_fleet_plan_with_identity(
                    "t",
                    "human:eng",
                    "fleet_mission",
                    "ratified for projection",
                    &GovernedWriteIdentity::local_demo("human:eng"),
                )
                .expect("ratify");
            kernel
                .record_fleet_run_event(FleetRunEvent {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    fleet_id: "fleet_mission",
                    event: "run_started",
                    stream_id: "",
                    forge_run_id: "",
                    detail: "streams=4 repo_id=swift repo_root=/tmp/swift execution_backend=hosted_sandbox cloud_environment_id=env_swift mission_id=mission_123 mission_milestone_id=mission_milestone_02",
                })
                .expect("fleet start");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let value: serde_json::Value =
            serde_json::from_str(response.body_text()).expect("valid json");
        assert_eq!(value["fleet_runs"][0]["goal"], "fleet mission test");
        assert_eq!(value["fleet_runs"][0]["checks"][0], "cargo test");
        assert_eq!(value["fleet_runs"][0]["repo_id"], "swift");
        assert_eq!(
            value["fleet_runs"][0]["execution_backend"],
            "hosted_sandbox"
        );
        assert_eq!(value["fleet_runs"][0]["execution_target_kind"], "mdx_cloud");
        assert_eq!(value["fleet_runs"][0]["cloud_environment_id"], "env_swift");
        assert_eq!(
            value["fleet_runs"][0]["mission"]["mission_id"],
            "mission_123"
        );
        assert_eq!(
            value["fleet_runs"][0]["mission"]["milestone_id"],
            "mission_milestone_02"
        );
        assert_eq!(
            value["fleet_runs"][0]["mission"]["checkpoint_route"],
            "/forge/long-horizon-mission-checkpoints.json"
        );
        assert_eq!(
            value["fleet_runs"][0]["mission"]["checkpoint_grants_execution_authority"],
            false
        );
    }

    #[test]
    fn fleet_semantic_strategy_rollup_counts_stream_and_integration_gates() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    run_id: "forge_run_stream",
                    event: "run_started",
                    work_item_id: "wi",
                    detail: "stream",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &GovernedWriteIdentity::local_demo("human:eng"),
                &[(
                    "fleet_stream_required_semantic_operations",
                    "related_tests\nreferences",
                )],
            )
            .expect("stream start");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_stream",
                event: "tool_executed",
                work_item_id: "wi",
                detail: "semantic_query status=OK operation=related_tests",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("stream semantic");
        kernel
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    run_id: "forge_run_integration",
                    event: "run_started",
                    work_item_id: "wi",
                    detail: "integration",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &GovernedWriteIdentity::local_demo("human:eng"),
                &[(
                    "fleet_integration_required_semantic_operations",
                    "file_outline\ndiagnostics",
                )],
            )
            .expect("integration start");
        for operation in ["file_outline", "diagnostics"] {
            kernel
                .record_forge_run_event(mdx_core::ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    run_id: "forge_run_integration",
                    event: "tool_executed",
                    work_item_id: "wi",
                    detail: &format!("semantic_query status=OK operation={operation}"),
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("integration semantic");
        }

        let lanes = vec![(
            "s1".to_string(),
            "done".to_string(),
            "forge_run_stream".to_string(),
            "done".to_string(),
        )];
        let rollup = fleet_semantic_strategy_rollup(&kernel, &lanes, "forge_run_integration", "");

        assert_eq!(rollup.assigned_count, 2);
        assert_eq!(rollup.satisfied_count, 1);
        assert_eq!(rollup.missing_operations, vec!["s1:references"]);

        let no_wiring_rollup = fleet_semantic_strategy_rollup(
            &kernel,
            &lanes,
            "forge_run_integration",
            "full_suite=passed commands=1 wiring=not_needed",
        );
        assert_eq!(no_wiring_rollup.assigned_count, 1);
        assert_eq!(no_wiring_rollup.satisfied_count, 0);
        assert_eq!(no_wiring_rollup.missing_operations, vec!["s1:references"]);
    }

    #[test]
    fn fleet_projection_reports_principal_engineer_proof_gates() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let fleet_id = {
            let mut kernel = kernel.write().expect("kernel");
            let streams = vec![FleetStream {
                stream_id: "s1_slugger".to_string(),
                objective: "Implement slug generation".to_string(),
                write_scope: vec!["Sources/Canary/Slugger.swift".to_string()],
                interface_contract: "Keep Slugger API stable".to_string(),
                depends_on: Vec::new(),
                checks: vec!["swift test".to_string()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            }];
            let report = kernel
                .record_fleet_plan_draft(
                    FleetPlanDraft {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: "",
                        spec: "modernize slug generation",
                        goal: "modernize slug generation",
                        checks: &["swift test".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["swift test".to_string()],
                        justification: "one scoped stream",
                        requested_width: 0,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "swift-canary",
                        repo_primary_language: "swift",
                        language_pack_id: "swift-spm",
                        repo_profile_suggested_checks: "swift test",
                        repo_profile_artifact_patterns: ".build/**,DerivedData/**",
                        repo_profile_semantic_intelligence: "sourcekit-lsp,tree-sitter",
                        repo_profile_semantic_tool_readiness: "semantic_query:available",
                        repo_profile_toolchain_readiness: "swift:test-ready",
                        repo_profile_proof_plan_status: "ready",
                        repo_profile_proof_plan_summary: "Use swift test.",
                        repo_profile_source: "live-profile",
                        wide_plan_review_required: false,
                        wide_plan_review_status: "not_required",
                        wide_plan_review_reviewer_model: "",
                        wide_plan_review_verdict: "not_required",
                        wide_plan_review_confidence: "none",
                        wide_plan_review_concerns: "",
                    },
                    &streams,
                )
                .expect("draft");
            kernel
                .ratify_fleet_plan_with_identity(
                    "t",
                    "human:eng",
                    &report.fleet_id,
                    "split is scoped",
                    &GovernedWriteIdentity::local_demo("human:eng"),
                )
                .expect("ratify");
            for event in [
                (
                    "run_started",
                    "",
                    "",
                    "streams=1 repo_id=swift-canary repo_root=/tmp/swift",
                ),
                (
                    "stream_started",
                    "s1_slugger",
                    "forge_run_1",
                    "scope_paths=1",
                ),
                (
                    "stream_finished",
                    "s1_slugger",
                    "forge_run_1",
                    "status=RUN_FINISHED_DONE done=1/1",
                ),
                (
                    "integration_finished",
                    "",
                    "",
                    "branch=forge/fleet merged=1 wired=true ok",
                ),
                ("review_finished", "", "", "verdict: ready"),
                (
                    "run_finished",
                    "",
                    "",
                    "streams=1 green=1 needs_attention=0 fleet_branch=forge/fleet",
                ),
            ] {
                kernel
                    .record_fleet_run_event(FleetRunEvent {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: &report.fleet_id,
                        event: event.0,
                        stream_id: event.1,
                        forge_run_id: event.2,
                        detail: event.3,
                    })
                    .expect(event.0);
            }
            kernel
                .record_forge_run_event_with_evidence_fields(
                    mdx_core::ForgeRunEvent {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        run_id: "forge_run_1",
                        event: "run_started",
                        work_item_id: "work_item_fleet_s1_slugger",
                        detail: "fleet stream accepted",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("human:eng"),
                    &[
                        ("builder_casting_status", "LOCAL_RUN_EVIDENCE_SLOT_READY"),
                        ("builder_casting_requested_slot", ""),
                        ("builder_casting_selected_slot", "OPUS"),
                        ("builder_casting_recommended_slot", "OPUS"),
                        (
                            "builder_casting_recommended_model_profile_id",
                            "codex_anthropic_responses_profile",
                        ),
                        ("builder_casting_recommended_provider_family", "anthropic"),
                        ("builder_casting_recommended_model_id", "claude-opus-4-8"),
                        ("builder_casting_basis", "local_run_track_record"),
                        ("builder_casting_matching_eval_score_count", "0"),
                        ("builder_casting_accepted_eval_score_count", "0"),
                        ("builder_casting_matching_run_count", "4"),
                        ("builder_casting_done_rate_pct", "75"),
                        ("builder_casting_requested_slot_matches_evidence", "true"),
                        ("builder_casting_grants_execution_authority", "false"),
                    ],
                )
                .expect("stream run started");
            report.fleet_id
        };

        let response = handle_projection("GET", &kernel).expect("projection");

        for expected in [
            &format!(r#""fleet_id":"{fleet_id}""#),
            r#""proof":{"status":"ready_for_principal_review""#,
            r#""repo_id":"swift-canary""#,
            r#""language_pack_id":"swift-spm""#,
            r#""declared_check_count":2"#,
            r#""execution_geometry":{"strategy":"parallel_streams_plus_integration""#,
            r#""fleet_lane_selection":{"generated_from":"fleet.run.event.stream_lanes""#,
            r#""recommended_stream_id":"s1_slugger""#,
            r#""recommended_run_id":"forge_run_1""#,
            r#""coder":"OPUS""#,
            r#""planner_coder":"default""#,
            r#""builder_casting":{"status":"LOCAL_RUN_EVIDENCE_SLOT_READY""#,
            r#""recommended_model_profile_id":"codex_anthropic_responses_profile""#,
            r#""matching_run_count":4"#,
            r#""runtime_repair":{"started_count":0,"finished_count":0,"target_count":0,"repaired_count":0"#,
            r#""started_lane_count":1"#,
            r#""disjoint_write_scopes_validated":true"#,
            r#""artifact_filter_count":2"#,
            r#""semantic_signal_count":3"#,
            r#""principal_engineer_evidence_ready":true"#,
            r#""missing_gates":[]"#,
        ] {
            assert!(
                response.body.contains(expected),
                "{expected}: {}",
                response.body
            );
        }
    }

    #[test]
    fn fleet_projection_exposes_backend_lane_repair_telemetry() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let fleet_id = {
            let mut kernel = kernel.write().expect("kernel");
            let streams = vec![FleetStream {
                stream_id: "s1_export".to_string(),
                objective: "Implement export lane".to_string(),
                write_scope: vec!["src/export.js".to_string()],
                interface_contract: "Export summarize".to_string(),
                depends_on: Vec::new(),
                checks: vec!["npm test".to_string()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            }];
            let report = kernel
                .record_fleet_plan_draft(
                    FleetPlanDraft {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: "",
                        spec: "repair one lane",
                        goal: "repair one lane",
                        checks: &["npm test".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["npm test".to_string()],
                        justification: "one scoped stream",
                        requested_width: 1,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "node-canary",
                        repo_primary_language: "javascript-typescript",
                        language_pack_id: "node",
                        repo_profile_suggested_checks: "npm test",
                        repo_profile_artifact_patterns: "node_modules/**",
                        repo_profile_semantic_intelligence: "typescript-language-server",
                        repo_profile_semantic_tool_readiness: "semantic_query:available",
                        repo_profile_toolchain_readiness: "npm:test-ready",
                        repo_profile_proof_plan_status: "ready",
                        repo_profile_proof_plan_summary: "Use npm test.",
                        repo_profile_source: "live-profile",
                        wide_plan_review_required: false,
                        wide_plan_review_status: "not_required",
                        wide_plan_review_reviewer_model: "",
                        wide_plan_review_verdict: "not_required",
                        wide_plan_review_confidence: "none",
                        wide_plan_review_concerns: "",
                    },
                    &streams,
                )
                .expect("draft");
            kernel
                .ratify_fleet_plan_with_identity(
                    "t",
                    "human:eng",
                    &report.fleet_id,
                    "repair proof",
                    &GovernedWriteIdentity::local_demo("human:eng"),
                )
                .expect("ratify");
            for event in [
                (
                    "run_started",
                    "",
                    "",
                    "streams=1 repo_id=node-canary repo_root=/tmp/node",
                ),
                (
                    "stream_started",
                    "s1_export",
                    "forge_run_lane",
                    "scope_paths=1",
                ),
                (
                    "stream_needs_attention",
                    "s1_export",
                    "forge_run_lane",
                    "status=RUN_BUDGET_EXHAUSTED done=1/1",
                ),
                ("lane_repair_started", "", "", "targets=1 max_attempts=2"),
                (
                    "stream_started",
                    "s1_export",
                    "forge_run_lane_repair",
                    "repair_attempt=1 revising=forge/run-forge_run_lane",
                ),
                (
                    "stream_finished",
                    "s1_export",
                    "forge_run_lane_repair",
                    "repair_attempt=1 status=RUN_FINISHED_DONE done=1/1",
                ),
                ("lane_repair_finished", "", "", "targets=1 repaired=1"),
                (
                    "integration_finished",
                    "",
                    "forge_run_integration",
                    "branch=forge/fleet merged=1 wired=true full_suite=passed",
                ),
                ("review_finished", "", "", "verdict: ready"),
                (
                    "run_finished",
                    "",
                    "",
                    "streams=1 green=1 needs_attention=0 fleet_branch=forge/fleet",
                ),
            ] {
                kernel
                    .record_fleet_run_event(FleetRunEvent {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: &report.fleet_id,
                        event: event.0,
                        stream_id: event.1,
                        forge_run_id: event.2,
                        detail: event.3,
                    })
                    .expect(event.0);
            }
            report.fleet_id
        };

        let response = handle_projection("GET", &kernel).expect("projection");

        for expected in [
            &format!(r#""fleet_id":"{fleet_id}""#),
            r#""runtime_repair":{"started_count":1,"finished_count":1,"target_count":1,"repaired_count":1,"detail":"targets=1 repaired=1""#,
            r#""stream_id":"s1_export","state":"done","forge_run_id":"forge_run_lane_repair""#,
            r#""final_detail":"streams=1 green=1 needs_attention=0 fleet_branch=forge/fleet""#,
        ] {
            assert!(
                response.body.contains(expected),
                "{expected}: {}",
                response.body
            );
        }
    }

    #[test]
    fn fleet_projection_infers_repair_telemetry_from_lane_revision_when_events_are_missing() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let fleet_id = {
            let mut kernel = kernel.write().expect("kernel");
            let streams = vec![FleetStream {
                stream_id: "s1_export".to_string(),
                objective: "Implement export lane".to_string(),
                write_scope: vec!["src/export.js".to_string()],
                interface_contract: "Export summarize".to_string(),
                depends_on: Vec::new(),
                checks: vec!["npm test".to_string()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            }];
            let report = kernel
                .record_fleet_plan_draft(
                    FleetPlanDraft {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: "",
                        spec: "repair one lane without explicit repair events",
                        goal: "repair one lane without explicit repair events",
                        checks: &["npm test".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["npm test".to_string()],
                        justification: "runtime repair telemetry fallback proof",
                        requested_width: 1,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "node-canary",
                        repo_primary_language: "javascript-typescript",
                        language_pack_id: "node",
                        repo_profile_suggested_checks: "npm test",
                        repo_profile_artifact_patterns: "node_modules/**",
                        repo_profile_semantic_intelligence: "typescript-language-server",
                        repo_profile_semantic_tool_readiness: "semantic_query:available",
                        repo_profile_toolchain_readiness: "npm:test-ready",
                        repo_profile_proof_plan_status: "ready",
                        repo_profile_proof_plan_summary: "Use npm test.",
                        repo_profile_source: "live-profile",
                        wide_plan_review_required: false,
                        wide_plan_review_status: "not_required",
                        wide_plan_review_reviewer_model: "",
                        wide_plan_review_verdict: "not_required",
                        wide_plan_review_confidence: "none",
                        wide_plan_review_concerns: "",
                    },
                    &streams,
                )
                .expect("draft");
            kernel
                .ratify_fleet_plan_with_identity(
                    "t",
                    "human:eng",
                    &report.fleet_id,
                    "repair fallback proof",
                    &GovernedWriteIdentity::local_demo("human:eng"),
                )
                .expect("ratify");
            for event in [
                (
                    "run_started",
                    "",
                    "",
                    "streams=1 repo_id=node-canary repo_root=/tmp/node",
                ),
                (
                    "stream_needs_attention",
                    "s1_export",
                    "forge_run_lane",
                    "status=RUN_FINISHED_CANNOT_PROCEED done=1/1",
                ),
                (
                    "integration_finished",
                    "",
                    "",
                    "skipped: streams_need_attention green=0 needs_attention=1",
                ),
                (
                    "run_finished",
                    "",
                    "",
                    "streams=1 green=0 needs_attention=1",
                ),
            ] {
                kernel
                    .record_fleet_run_event(FleetRunEvent {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: &report.fleet_id,
                        event: event.0,
                        stream_id: event.1,
                        forge_run_id: event.2,
                        detail: event.3,
                    })
                    .expect(event.0);
            }
            for event in [
                (
                    "forge_run_lane",
                    "run_started",
                    "accepted: 1 allowed_commands repo_root=/tmp/node",
                ),
                (
                    "forge_run_lane",
                    "run_finished",
                    "RUN_FINISHED_CANNOT_PROCEED",
                ),
                (
                    "forge_run_lane_repair",
                    "run_started",
                    "accepted: 1 allowed_commands repo_root=/tmp/node revising=forge/run-forge_run_lane",
                ),
                (
                    "forge_run_lane_repair",
                    "run_finished",
                    "RUN_FINISHED_CANNOT_PROCEED",
                ),
            ] {
                kernel
                    .record_forge_run_event(mdx_core::ForgeRunEvent {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        run_id: event.0,
                        event: event.1,
                        work_item_id: "",
                        detail: event.2,
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    })
                    .expect(event.1);
            }
            report.fleet_id
        };

        let response = handle_projection("GET", &kernel).expect("projection");

        for expected in [
            &format!(r#""fleet_id":"{fleet_id}""#),
            r#""runtime_repair":{"started_count":1,"finished_count":1,"target_count":1,"repaired_count":0,"detail":"targets=1 repaired=0 attempts=1 source=runtime_recovery""#,
            r#""latest_lane_revision":{"run_id":"forge_run_lane_repair","status":"cannot_proceed""#,
        ] {
            assert!(
                response.body.contains(expected),
                "{expected}: {}",
                response.body
            );
        }
    }

    #[test]
    fn fleet_projection_marks_skipped_integration_as_skipped_not_done() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let fleet_id = {
            let mut kernel = kernel.write().expect("kernel");
            let streams = vec![FleetStream {
                stream_id: "s1_export".to_string(),
                objective: "Implement export lane".to_string(),
                write_scope: vec!["src/export.js".to_string()],
                interface_contract: "Export summarize".to_string(),
                depends_on: Vec::new(),
                checks: vec!["npm test".to_string()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            }];
            let report = kernel
                .record_fleet_plan_draft(
                    FleetPlanDraft {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: "",
                        spec: "one lane remains blocked",
                        goal: "one lane remains blocked",
                        checks: &["npm test".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["npm test".to_string()],
                        justification: "projection classification proof",
                        requested_width: 1,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "node-canary",
                        repo_primary_language: "javascript-typescript",
                        language_pack_id: "node",
                        repo_profile_suggested_checks: "npm test",
                        repo_profile_artifact_patterns: "node_modules/**",
                        repo_profile_semantic_intelligence: "typescript-language-server",
                        repo_profile_semantic_tool_readiness: "semantic_query:available",
                        repo_profile_toolchain_readiness: "npm:test-ready",
                        repo_profile_proof_plan_status: "ready",
                        repo_profile_proof_plan_summary: "Use npm test.",
                        repo_profile_source: "live-profile",
                        wide_plan_review_required: false,
                        wide_plan_review_status: "not_required",
                        wide_plan_review_reviewer_model: "",
                        wide_plan_review_verdict: "not_required",
                        wide_plan_review_confidence: "none",
                        wide_plan_review_concerns: "",
                    },
                    &streams,
                )
                .expect("draft");
            kernel
                .ratify_fleet_plan_with_identity(
                    "t",
                    "human:eng",
                    &report.fleet_id,
                    "projection proof",
                    &GovernedWriteIdentity::local_demo("human:eng"),
                )
                .expect("ratify");
            for event in [
                (
                    "run_started",
                    "",
                    "",
                    "streams=1 repo_id=node-canary repo_root=/tmp/node",
                ),
                (
                    "stream_needs_attention",
                    "s1_export",
                    "forge_run_lane",
                    "status=RUN_BUDGET_EXHAUSTED done=1/1",
                ),
                (
                    "integration_finished",
                    "",
                    "",
                    "skipped: streams_need_attention green=0 needs_attention=1",
                ),
            ] {
                kernel
                    .record_fleet_run_event(FleetRunEvent {
                        tenant_id: "t",
                        actor_id: "human:eng",
                        fleet_id: &report.fleet_id,
                        event: event.0,
                        stream_id: event.1,
                        forge_run_id: event.2,
                        detail: event.3,
                    })
                    .expect(event.0);
            }
            report.fleet_id
        };

        let response = handle_projection("GET", &kernel).expect("projection");

        assert!(
            response
                .body
                .contains(&format!(r#""fleet_id":"{fleet_id}""#)),
            "{}",
            response.body
        );
        assert!(
            response.body.contains(r#""integration_state":"skipped""#),
            "{}",
            response.body
        );
        assert!(
            !response.body.contains(r#""integration_state":"done""#),
            "{}",
            response.body
        );
    }

    #[test]
    fn latest_revision_is_folded_from_forge_run_receipts() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_revision",
                event: "run_started",
                work_item_id: "",
                detail: "accepted: 1 allowed_commands repo_root=/tmp/repo revising=fleet/demo",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision started");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_revision",
                event: "check_passed",
                work_item_id: "",
                detail: "run_command npm test exit=0",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision check");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_revision",
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE turns=3 files_changed=2",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision finished");

        let revision = latest_fleet_branch_revision(&kernel, "fleet/demo");

        assert_eq!(revision.run_id, "forge_run_revision");
        assert_eq!(revision.status, "done");
        assert_eq!(revision.checks_passed, 1);
        assert_eq!(revision.checks_failed, 0);
        assert!(revision.detail.contains("RUN_FINISHED_DONE"));
    }

    #[test]
    fn repaired_revision_moves_fleet_to_revision_ready_for_review() {
        let kernel = MdxKernel::boot_local();
        let mut plan_fields = BTreeMap::new();
        plan_fields.insert("repo_id".to_string(), "node-wide".to_string());
        plan_fields.insert(
            "repo_primary_language".to_string(),
            "javascript-typescript".to_string(),
        );
        plan_fields.insert("language_pack_id".to_string(), "node".to_string());
        plan_fields.insert("repo_profile_source".to_string(), "test".to_string());
        plan_fields.insert(
            "repo_profile_artifact_patterns".to_string(),
            "node_modules/**".to_string(),
        );
        plan_fields.insert(
            "repo_profile_semantic_intelligence".to_string(),
            "typescript-language-server".to_string(),
        );
        plan_fields.insert(
            "repo_profile_semantic_tool_readiness".to_string(),
            "semantic_query=available".to_string(),
        );
        plan_fields.insert(
            "repo_profile_toolchain_readiness".to_string(),
            "npm=test-ready".to_string(),
        );
        plan_fields.insert("full_suite_checks".to_string(), "npm test".to_string());
        let streams = vec![FleetStream {
            stream_id: "s1".to_string(),
            objective: "build".to_string(),
            write_scope: vec!["src/a.js".to_string()],
            interface_contract: "export a".to_string(),
            depends_on: Vec::new(),
            checks: vec!["npm test".to_string()],
            max_turns: 10,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        }];
        let lanes = vec![(
            "s1".to_string(),
            "done".to_string(),
            "forge_run_stream".to_string(),
            "status=RUN_FINISHED_DONE".to_string(),
        )];
        let latest_revision = FleetBranchRevision {
            run_id: "forge_run_revision".to_string(),
            status: "done".to_string(),
            branch: "fleet/demo".to_string(),
            checks_passed: 1,
            checks_failed: 0,
            attempt_count: 1,
            detail: "status=RUN_FINISHED_DONE turns=3 files_changed=2".to_string(),
        };
        let latest_lane_revision = FleetBranchRevision::default();

        let proof = fleet_proof_packet(FleetProofPacketInput {
            started: true,
            finished: true,
            integration_state: "done",
            integration_run_id: "forge_run_integration",
            integration_detail: "branch=fleet/demo merged=1 wired=true full_suite=passed",
            review_verdict: "VERDICT: needs work\n1. fix edge case",
            latest_revision: &latest_revision,
            latest_lane_revision: &latest_lane_revision,
            lanes: &lanes,
            kernel: &kernel,
            plan_fields: &plan_fields,
            streams: &streams,
        });

        assert!(proof.contains(r#""status":"revision_ready_for_review""#));
        assert!(
            proof.contains(r#""runtime_recovery":{"recovery_class":"integration_revision_ready""#)
        );
        assert!(proof.contains(r#""runtime_responsibility":"human_interrupt""#));
        assert!(proof.contains(r#""next_action":"review_repaired_integration""#));
        assert!(proof.contains(r#""runtime_superstep":"principal_review""#));
        assert!(proof.contains(r#""recovery_health":"degraded_reviewable""#));
        assert!(proof.contains(r#""retry_policy":"bounded_repair_then_interrupt""#));
        assert!(proof.contains(r#""auto_continue_basis":"repaired_integration_ready""#));
        assert!(proof.contains(r#""checkpoint_scope":"fleet_branch""#));
        assert!(proof.contains(r#""grouped_interrupt_count":1"#));
        assert!(proof.contains(r#""last_good_run_id":"forge_run_revision""#));
        assert!(proof.contains(r#""repair_attempt_count":1"#));
        assert!(proof.contains(r#""repair_budget_remaining":1"#));
        assert!(proof.contains(r#""revision_repaired":true"#));
        assert!(proof.contains(r#""revision_review_required""#));
        assert!(!proof.contains(r#""integration_review_needs_work""#));

        let rereviewed = fleet_proof_packet(FleetProofPacketInput {
            started: true,
            finished: true,
            integration_state: "done",
            integration_run_id: "forge_run_integration",
            integration_detail: "branch=fleet/demo merged=1 wired=true full_suite=passed",
            review_verdict: "VERDICT: ready\n1. repair verified",
            latest_revision: &latest_revision,
            latest_lane_revision: &latest_lane_revision,
            lanes: &lanes,
            kernel: &kernel,
            plan_fields: &plan_fields,
            streams: &streams,
        });
        assert!(rereviewed.contains(r#""status":"ready_for_principal_review""#));
        assert!(rereviewed.contains(r#""repair_attempt_count":1"#));
        assert!(rereviewed.contains(r#""repair_budget_remaining":1"#));
    }

    #[test]
    fn integration_review_recovery_interrupts_after_repair_budget_is_spent() {
        let kernel = MdxKernel::boot_local();
        let mut plan_fields = BTreeMap::new();
        plan_fields.insert("repo_id".to_string(), "node-wide".to_string());
        plan_fields.insert(
            "repo_primary_language".to_string(),
            "javascript-typescript".to_string(),
        );
        plan_fields.insert("language_pack_id".to_string(), "node".to_string());
        plan_fields.insert("repo_profile_source".to_string(), "test".to_string());
        plan_fields.insert(
            "repo_profile_artifact_patterns".to_string(),
            "node_modules/**".to_string(),
        );
        plan_fields.insert(
            "repo_profile_semantic_intelligence".to_string(),
            "typescript-language-server".to_string(),
        );
        plan_fields.insert("full_suite_checks".to_string(), "npm test".to_string());
        let streams = vec![FleetStream {
            stream_id: "s1".to_string(),
            objective: "build".to_string(),
            write_scope: vec!["src/a.js".to_string()],
            interface_contract: "export a".to_string(),
            depends_on: Vec::new(),
            checks: vec!["npm test".to_string()],
            max_turns: 10,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        }];
        let lanes = vec![(
            "s1".to_string(),
            "done".to_string(),
            "forge_run_stream".to_string(),
            "status=RUN_FINISHED_DONE".to_string(),
        )];
        let latest_revision = FleetBranchRevision {
            run_id: "forge_run_revision_2".to_string(),
            status: "cannot_proceed".to_string(),
            branch: "fleet/demo".to_string(),
            checks_passed: 0,
            checks_failed: 1,
            attempt_count: 2,
            detail: "status=RUN_FINISHED_CANNOT_PROCEED turns=12 files_changed=2".to_string(),
        };
        let latest_lane_revision = FleetBranchRevision::default();

        let proof = fleet_proof_packet(FleetProofPacketInput {
            started: true,
            finished: true,
            integration_state: "done",
            integration_run_id: "forge_run_integration",
            integration_detail: "branch=fleet/demo merged=1 wired=true full_suite=passed",
            review_verdict: "VERDICT: needs work\n1. fix edge case",
            latest_revision: &latest_revision,
            latest_lane_revision: &latest_lane_revision,
            lanes: &lanes,
            kernel: &kernel,
            plan_fields: &plan_fields,
            streams: &streams,
        });

        assert!(proof.contains(r#""status":"needs_principal_attention""#));
        assert!(proof.contains(r#""recovery_class":"integration_repair_exhausted""#));
        assert!(proof.contains(r#""runtime_responsibility":"human_interrupt""#));
        assert!(proof.contains(r#""next_action":"group_integration_findings_for_human_unblock""#));
        assert!(proof.contains(r#""runtime_superstep":"human_unblock""#));
        assert!(proof.contains(r#""recovery_health":"exhausted""#));
        assert!(proof.contains(r#""interrupt_group":"integration_review""#));
        assert!(proof.contains(r#""grouped_interrupt_count":1"#));
        assert!(proof.contains(r#""repair_attempt_count":2"#));
        assert!(proof.contains(r#""repair_budget_remaining":0"#));
        assert!(proof.contains(r#""integration_review_needs_work""#));
    }

    #[test]
    fn lane_attention_runtime_recovery_prefers_repair_before_failure() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_csv",
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_BUDGET_EXHAUSTED turns=30 files_changed=1",
                turn: 30,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lane finished");
        let mut plan_fields = BTreeMap::new();
        plan_fields.insert("repo_id".to_string(), "node-wide".to_string());
        plan_fields.insert(
            "repo_primary_language".to_string(),
            "javascript-typescript".to_string(),
        );
        plan_fields.insert("language_pack_id".to_string(), "node".to_string());
        plan_fields.insert("repo_profile_source".to_string(), "test".to_string());
        plan_fields.insert(
            "repo_profile_artifact_patterns".to_string(),
            "node_modules/**".to_string(),
        );
        plan_fields.insert(
            "repo_profile_semantic_intelligence".to_string(),
            "typescript-language-server".to_string(),
        );
        plan_fields.insert("full_suite_checks".to_string(), "npm test".to_string());
        let streams = vec![FleetStream {
            stream_id: "csv".to_string(),
            objective: "build csv export".to_string(),
            write_scope: vec!["src/csv.js".to_string()],
            interface_contract: "export csv".to_string(),
            depends_on: Vec::new(),
            checks: vec!["npm test".to_string()],
            max_turns: 10,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        }];
        let lanes = vec![(
            "csv".to_string(),
            "needs_attention".to_string(),
            "forge_run_csv".to_string(),
            "status=RUN_BUDGET_EXHAUSTED".to_string(),
        )];
        let latest_revision = FleetBranchRevision::default();
        let latest_lane_revision = FleetBranchRevision::default();

        let proof = fleet_proof_packet(FleetProofPacketInput {
            started: true,
            finished: false,
            integration_state: "",
            integration_run_id: "",
            integration_detail: "",
            review_verdict: "",
            latest_revision: &latest_revision,
            latest_lane_revision: &latest_lane_revision,
            lanes: &lanes,
            kernel: &kernel,
            plan_fields: &plan_fields,
            streams: &streams,
        });

        assert!(proof.contains(r#""status":"needs_attention""#));
        assert!(proof.contains(r#""recovery_class":"lane_repair_needed""#));
        assert!(proof.contains(r#""runtime_responsibility":"fleet_runtime""#));
        assert!(proof.contains(r#""next_action":"repair_attention_lanes_before_failing_fleet""#));
        assert!(proof.contains(r#""runtime_superstep":"lane_repair""#));
        assert!(proof.contains(r#""recovery_health":"degraded_recoverable""#));
        assert!(proof.contains(r#""retry_policy":"bounded_repair_then_interrupt""#));
        assert!(proof.contains(r#""auto_continue_basis":"repair_budget_available""#));
        assert!(proof.contains(r#""checkpoint_scope":"lane_branch""#));
        assert!(proof.contains(r#""interrupt_group":"attention_lanes""#));
        assert!(proof.contains(r#""interrupt_required":false"#));
        assert!(proof.contains(r#""grouped_interrupt_count":0"#));
        assert!(proof.contains(r#""can_auto_continue":true"#));
        assert!(proof.contains(r#""repair_attempt_count":0"#));
        assert!(proof.contains(r#""max_repair_attempts":2"#));
        assert!(proof.contains(r#""repair_budget_remaining":2"#));
        assert!(proof.contains(r#""repair_targets":[{"target_type":"lane""#));
        assert!(proof.contains(r#""stream_id":"csv""#));
        assert!(proof.contains(r#""branch":"forge/run-forge_run_csv""#));
        assert!(proof.contains(r#""streams_need_attention""#));
    }

    #[test]
    fn lane_attention_runtime_recovery_interrupts_after_repair_budget_is_spent() {
        let mut kernel = MdxKernel::boot_local();
        let lane_run = "forge_run_csv";
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: lane_run,
                event: "evidence_appended",
                work_item_id: "",
                detail: "branch=forge/run-csv sha=abc123",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lane branch");
        for (run_id, turn) in [("forge_run_csv_repair_1", 1), ("forge_run_csv_repair_2", 2)] {
            kernel
                .record_forge_run_event(mdx_core::ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    run_id,
                    event: "run_started",
                    work_item_id: "",
                    detail: "accepted: 1 selected_checks repo_root=/tmp/repo revising=forge/run-csv",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("repair started");
            kernel
                .record_forge_run_event(mdx_core::ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    run_id,
                    event: "run_finished",
                    work_item_id: "",
                    detail: "status=RUN_FINISHED_CANNOT_PROCEED turns=10 files_changed=1",
                    turn,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("repair failed");
        }
        let mut plan_fields = BTreeMap::new();
        plan_fields.insert("repo_id".to_string(), "node-wide".to_string());
        plan_fields.insert(
            "repo_primary_language".to_string(),
            "javascript-typescript".to_string(),
        );
        plan_fields.insert("language_pack_id".to_string(), "node".to_string());
        plan_fields.insert("repo_profile_source".to_string(), "test".to_string());
        plan_fields.insert(
            "repo_profile_artifact_patterns".to_string(),
            "node_modules/**".to_string(),
        );
        plan_fields.insert("full_suite_checks".to_string(), "npm test".to_string());
        let streams = vec![FleetStream {
            stream_id: "csv".to_string(),
            objective: "build csv export".to_string(),
            write_scope: vec!["src/csv.js".to_string()],
            interface_contract: "export csv".to_string(),
            depends_on: Vec::new(),
            checks: vec!["npm test".to_string()],
            max_turns: 10,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        }];
        let lanes = vec![(
            "csv".to_string(),
            "needs_attention".to_string(),
            lane_run.to_string(),
            "status=RUN_BUDGET_EXHAUSTED".to_string(),
        )];
        let latest_revision = FleetBranchRevision::default();
        let latest_lane_revision = latest_repaired_lane_revision(&kernel, &lanes);

        let proof = fleet_proof_packet(FleetProofPacketInput {
            started: true,
            finished: false,
            integration_state: "",
            integration_run_id: "",
            integration_detail: "",
            review_verdict: "",
            latest_revision: &latest_revision,
            latest_lane_revision: &latest_lane_revision,
            lanes: &lanes,
            kernel: &kernel,
            plan_fields: &plan_fields,
            streams: &streams,
        });

        assert!(proof.contains(r#""status":"needs_attention""#));
        assert!(proof.contains(r#""recovery_class":"lane_repair_exhausted""#));
        assert!(proof.contains(r#""runtime_responsibility":"human_interrupt""#));
        assert!(proof.contains(r#""next_action":"group_attention_lanes_for_human_unblock""#));
        assert!(proof.contains(r#""runtime_superstep":"human_unblock""#));
        assert!(proof.contains(r#""recovery_health":"exhausted""#));
        assert!(proof.contains(r#""interrupt_required":true"#));
        assert!(proof.contains(r#""grouped_interrupt_count":1"#));
        assert!(proof.contains(r#""can_auto_continue":false"#));
        assert!(proof.contains(r#""repair_attempt_count":2"#));
        assert!(proof.contains(r#""repair_budget_remaining":0"#));
    }

    #[test]
    fn repaired_lane_revision_moves_attention_fleet_to_integration_review() {
        let mut kernel = MdxKernel::boot_local();
        let lane_run = "forge_run_lane";
        let revision_run = "forge_run_lane_revision";
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: lane_run,
                event: "evidence_appended",
                work_item_id: "",
                detail: "branch=forge/run-lane sha=abc123",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lane branch");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: revision_run,
                event: "run_started",
                work_item_id: "",
                detail: "accepted: 1 allowed_commands repo_root=/tmp/repo revising=forge/run-lane",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision started");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: revision_run,
                event: "check_passed",
                work_item_id: "",
                detail: "run_command npm test exit=0",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision check");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: revision_run,
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE turns=4 files_changed=1",
                turn: 4,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision finished");
        let lanes = vec![(
            "csv".to_string(),
            "needs_attention".to_string(),
            lane_run.to_string(),
            "status=RUN_BUDGET_EXHAUSTED".to_string(),
        )];
        let latest_lane_revision = latest_repaired_lane_revision(&kernel, &lanes);

        assert_eq!(latest_lane_revision.run_id, revision_run);
        assert_eq!(latest_lane_revision.status, "done");
        assert_eq!(latest_lane_revision.checks_passed, 1);

        let mut plan_fields = BTreeMap::new();
        plan_fields.insert("repo_id".to_string(), "node-wide".to_string());
        plan_fields.insert(
            "repo_primary_language".to_string(),
            "javascript-typescript".to_string(),
        );
        plan_fields.insert("language_pack_id".to_string(), "node".to_string());
        plan_fields.insert("repo_profile_source".to_string(), "test".to_string());
        plan_fields.insert(
            "repo_profile_artifact_patterns".to_string(),
            "node_modules/**".to_string(),
        );
        plan_fields.insert(
            "repo_profile_semantic_intelligence".to_string(),
            "typescript-language-server".to_string(),
        );
        plan_fields.insert("full_suite_checks".to_string(), "npm test".to_string());
        let streams = vec![FleetStream {
            stream_id: "csv".to_string(),
            objective: "build csv export".to_string(),
            write_scope: vec!["src/csv.js".to_string()],
            interface_contract: "export csv".to_string(),
            depends_on: Vec::new(),
            checks: vec!["npm test".to_string()],
            max_turns: 10,
            builder_slot: "GROK".to_string(),
            data_sensitivity: "internal".to_string(),
        }];
        let latest_revision = FleetBranchRevision::default();

        let proof = fleet_proof_packet(FleetProofPacketInput {
            started: true,
            finished: false,
            integration_state: "",
            integration_run_id: "",
            integration_detail: "",
            review_verdict: "",
            latest_revision: &latest_revision,
            latest_lane_revision: &latest_lane_revision,
            lanes: &lanes,
            kernel: &kernel,
            plan_fields: &plan_fields,
            streams: &streams,
        });

        assert!(proof.contains(r#""status":"lane_revision_ready_for_integration""#));
        assert!(proof.contains(r#""recovery_class":"lane_revision_ready""#));
        assert!(proof.contains(r#""next_action":"rerun_integration_after_lane_repair""#));
        assert!(proof.contains(r#""auto_continue_basis":"repaired_lane_ready""#));
        assert!(proof.contains(r#""can_auto_continue":true"#));
        assert!(
            proof.contains(r#""selection_blocked_reason":"lane_revision_requires_integration""#)
        );
        assert!(proof.contains(r#""last_good_run_id":"forge_run_lane_revision""#));
    }

    #[test]
    fn repaired_lane_revision_uses_standard_branch_when_lane_branch_receipt_is_missing() {
        let mut kernel = MdxKernel::boot_local();
        let lane_run = "forge_run_lane";
        let revision_run = "forge_run_lane_revision";
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: lane_run,
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_BUDGET_EXHAUSTED turns=30 files_changed=1",
                turn: 30,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lane finished without branch evidence");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: revision_run,
                event: "run_started",
                work_item_id: "",
                detail: "accepted: 1 selected_checks repo_root=/tmp/repo revising=forge/run-forge_run_lane",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision started");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: revision_run,
                event: "check_passed",
                work_item_id: "",
                detail: "run_command npm test exit=0",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision check");
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: revision_run,
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE turns=4 files_changed=1",
                turn: 4,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revision finished");
        let lanes = vec![(
            "csv".to_string(),
            "needs_attention".to_string(),
            lane_run.to_string(),
            "status=RUN_BUDGET_EXHAUSTED".to_string(),
        )];
        let latest_lane_revision = latest_repaired_lane_revision(&kernel, &lanes);

        assert_eq!(latest_lane_revision.run_id, revision_run);
        assert_eq!(latest_lane_revision.branch, "forge/run-forge_run_lane");
        assert_eq!(latest_lane_revision.status, "done");
        assert_eq!(latest_lane_revision.checks_passed, 1);
    }
}
