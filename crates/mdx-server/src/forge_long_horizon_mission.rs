use crate::RouteResponse;
use mdx_core::{
    FleetPlanDraft, FleetStream, ForgeLongHorizonMissionAdmission,
    ForgeLongHorizonMissionCheckpoint, ForgeLongHorizonMissionDashboard,
    ForgeLongHorizonMissionMilestone, ForgeLongHorizonMissionPacket, MdxKernel,
    json_string_literal, mission_milestones_json,
};
use std::path::Path;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/forge/long-horizon-missions.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_mission_admission_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/long-horizon-mission-checkpoints.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_mission_checkpoint_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/long-horizon-mission-launches.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        return Some(render_mission_launch_json(body, kernel));
    }
    if path == "/forge/long-horizon-missions/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_mission_projection_json(&kernel),
        )));
    }
    if path == "/forge/long-horizon-mission-dashboard.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_mission_dashboard_json(&kernel),
        )));
    }
    None
}

fn render_mission_admission_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let goal = json_string_field(body, "goal").unwrap_or_else(|| {
        "Ship a meaty Forge fleet feature through governed checkpoints".to_string()
    });
    // A caller may name the mission (the API and seed data do). When it does not
    // (the shape-a-mission UI omits it), generate a UNIQUE id instead of reusing
    // a fixed default - otherwise every mission shaped from the product collides
    // on one id, and launching a freshly shaped mission fires a different
    // same-id mission's milestone.
    let validation_commands_for_id =
        json_string_field(body, "validation_commands").unwrap_or_default();
    let mission_id = json_string_field(body, "mission_id")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            let existing = kernel.project_forge_long_horizon_missions().packets.len();
            generated_mission_id(&goal, &validation_commands_for_id, existing)
        });
    let non_goals = json_string_field(body, "non_goals").unwrap_or_else(|| {
        "no live provider calls,no adapter execution,no production writes".to_string()
    });
    let constraints = json_string_field(body, "constraints").unwrap_or_else(|| {
        "bounded write scope,receipt-backed checkpoints,verification after every milestone"
            .to_string()
    });
    let done_when = json_string_field(body, "done_when").unwrap_or_else(|| {
        "mission packet,projection,dashboard,schemas,docs,and checks are coherent".to_string()
    });
    let allowed_write_scope = json_string_field(body, "allowed_write_scope").unwrap_or_else(|| {
        "crates/mdx-core/src/,crates/mdx-server/src/,crates/mdx-codegen/src/,docs/,scripts/"
            .to_string()
    });
    let blocked_paths = json_string_field(body, "blocked_paths")
        .unwrap_or_else(|| "provider secrets,production state,live adapter execution".to_string());
    let validation_commands = json_string_field(body, "validation_commands").unwrap_or_else(|| {
        "make forge-long-horizon-mission-check,cargo test -p mdx-core harness_long_horizon_mission,cargo check -p mdx-server".to_string()
    });
    let model_policy = json_string_field(body, "model_policy")
        .unwrap_or_else(|| "mdx_approved_responses_compatible_provider_profiles_only".to_string());
    let provider_allowlist = json_string_field(body, "provider_allowlist")
        .unwrap_or_else(|| "gemini,anthropic,xai,aws_bedrock".to_string());
    let fleet_width = json_u32_field(body, "fleet_width").unwrap_or(128);
    let max_runtime_ms = json_u64_field(body, "max_runtime_ms").unwrap_or(86_400_000);
    let max_cost_cents = json_u32_field(body, "max_cost_cents").unwrap_or(1_000);
    let checkpoint_cadence_minutes =
        json_u32_field(body, "checkpoint_cadence_minutes").unwrap_or(30);
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:codex",
        "operator",
    );
    let report = kernel
        .admit_forge_long_horizon_mission_local_with_identity(
            ForgeLongHorizonMissionAdmission {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                mission_id: &mission_id,
                goal: &goal,
                non_goals: &non_goals,
                constraints: &constraints,
                done_when: &done_when,
                allowed_write_scope: &allowed_write_scope,
                blocked_paths: &blocked_paths,
                validation_commands: &validation_commands,
                model_policy: &model_policy,
                provider_allowlist: &provider_allowlist,
                fleet_width,
                max_runtime_ms,
                max_cost_cents,
                checkpoint_cadence_minutes,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-long-horizon-mission-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/long-horizon-missions.json","projection_route":"/forge/long-horizon-missions/projection.json","dashboard_route":"/forge/long-horizon-mission-dashboard.json","mission_id":{},"mission_receipt_id":{},"policy_decision_id":{},"milestone_count":{},"fleet_width":{},"max_runtime_ms":{},"max_cost_cents":{},"checkpoint_cadence_minutes":{},"spec_packet_recorded":{},"milestone_plan_recorded":{},"runbook_recorded":{},"status_log_recorded":{},"dashboard_ready":{},"live_provider_calls_allowed":{},"adapter_execution_allowed":{},"production_write_allowed":{},"blocked_reason":{},"live_substrate_required":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.mission_id),
        json_string_literal(&report.mission_receipt_id),
        json_string_literal(&report.policy_decision_id),
        report.milestone_count,
        report.fleet_width,
        report.max_runtime_ms,
        report.max_cost_cents,
        report.checkpoint_cadence_minutes,
        report.spec_packet_recorded,
        report.milestone_plan_recorded,
        report.runbook_recorded,
        report.status_log_recorded,
        report.dashboard_ready,
        report.live_provider_calls_allowed,
        report.adapter_execution_allowed,
        report.production_write_allowed,
        json_string_literal(report.blocked_reason),
    ))
}

fn render_mission_launch_json(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let mission_id = parsed["mission_id"].as_str().unwrap_or("").trim();
    if mission_id.is_empty() {
        return Ok(mission_launch_refusal("name the mission to launch"));
    }
    let milestone_id = parsed["mission_milestone_id"]
        .as_str()
        .or_else(|| parsed["milestone_id"].as_str())
        .unwrap_or("")
        .trim();
    if milestone_id.is_empty() {
        return Ok(mission_launch_refusal(
            "name the mission milestone to launch",
        ));
    }
    let requested_launch_shape = parsed["shape"]
        .as_str()
        .or_else(|| parsed["execution_shape"].as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let fleet_id = parsed["fleet_id"].as_str().unwrap_or("").trim();
    let repo_id = parsed["repo_id"].as_str().unwrap_or("").trim();
    let max_turns = parsed["max_turns"].as_u64().unwrap_or(0);
    let builder_slot = parsed["builder_slot"].as_str().unwrap_or("").trim();
    let requested_workers_from_body = parsed["requested_workers"]
        .as_u64()
        .or_else(|| parsed["fleet_width"].as_u64());
    let (packet, milestone) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let dashboard = kernel.project_forge_long_horizon_missions();
        let Some(packet) = dashboard
            .packets
            .iter()
            .find(|packet| packet.mission_id == mission_id)
        else {
            return Ok(mission_launch_refusal(&format!(
                "unknown mission {mission_id}"
            )));
        };
        let Some(milestone) = packet
            .milestones
            .iter()
            .find(|milestone| milestone.milestone_id == milestone_id)
        else {
            return Ok(mission_launch_refusal(&format!(
                "unknown milestone {milestone_id} for mission {mission_id}"
            )));
        };
        (packet.clone(), milestone.clone())
    };
    let requested_workers = requested_workers_from_body.unwrap_or(
        if requested_launch_shape == "fleet" || !fleet_id.is_empty() {
            packet.fleet_width as u64
        } else {
            1
        },
    );
    let launch_shape = mission_launch_shape(&requested_launch_shape, fleet_id, requested_workers);
    let checks = string_array_or_single(&parsed, "allowed_commands")
        .or_else(|| string_array_or_single(&parsed, "checks"))
        .filter(|items| items.iter().any(|item| !item.trim().is_empty()))
        .unwrap_or_else(|| vec![milestone.validation_command.clone()]);
    let intent = parsed["intent"].as_str().unwrap_or("").trim().to_string();
    let intent = if intent.is_empty() {
        mission_launch_default_intent(
            mission_id,
            milestone_id,
            &packet.goal,
            &milestone.title,
            &milestone.validation_command,
        )
    } else {
        intent
    };
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:codex",
        "operator",
    );

    let delegated_route;
    let mut auto_fleet_plan = serde_json::json!({"created":false});
    let delegated_body = match launch_shape.as_str() {
        "run" => {
            delegated_route = "/forge/runs.json";
            let mut value = serde_json::json!({
                "tenant_id": resolved.tenant_id,
                "actor_id": resolved.actor_id,
                "intent": intent,
                "run_title": packet.goal,
                "repo_id": repo_id,
                "allowed_commands": checks,
                "mission_id": mission_id,
                "mission_milestone_id": milestone_id,
            });
            if max_turns > 0 {
                value["max_turns"] = serde_json::json!(max_turns);
            }
            if !builder_slot.is_empty() {
                value["builder_slot"] = serde_json::json!(builder_slot);
            }
            if requested_workers > 1 {
                value["requested_workers"] = serde_json::json!(requested_workers);
            }
            value
        }
        "fleet" => {
            let fleet_id = if fleet_id.is_empty() {
                let auto_plan = match auto_mission_fleet_plan(AutoMissionFleetPlanInput {
                    packet: &packet,
                    milestone: &milestone,
                    repo_id,
                    requested_workers,
                    max_turns,
                    builder_slot,
                    checks: &checks,
                    tenant_id: &resolved.tenant_id,
                    actor_id: &resolved.actor_id,
                    identity: &resolved.identity,
                    kernel,
                }) {
                    Ok(plan) => plan,
                    Err(response) => return Ok(response),
                };
                auto_fleet_plan = auto_plan.to_json();
                auto_plan.fleet_id
            } else {
                fleet_id.to_string()
            };
            delegated_route = "/forge/fleet-runs.json";
            serde_json::json!({
                "tenant_id": resolved.tenant_id,
                "actor_id": resolved.actor_id,
                "fleet_id": fleet_id,
                "repo_id": repo_id,
                "mission_id": mission_id,
                "mission_milestone_id": milestone_id,
            })
        }
        _ => {
            return Ok(mission_launch_refusal(
                "mission launch shape must be run or fleet",
            ));
        }
    };
    let delegated_body_text = delegated_body.to_string();
    let delegated_response = match delegated_route {
        "/forge/runs.json" => crate::forge_run_route::route_response(
            "POST",
            delegated_route,
            &delegated_body_text,
            kernel,
        )
        .ok_or_else(|| "run route unavailable".to_string())??,
        "/forge/fleet-runs.json" => crate::fleet_run_route::route_response(
            "POST",
            delegated_route,
            &delegated_body_text,
            kernel,
        )
        .ok_or_else(|| "fleet run route unavailable".to_string())??,
        _ => return Ok(mission_launch_refusal("mission launch route unavailable")),
    };
    let delegated_json: serde_json::Value = serde_json::from_str(&delegated_response.body)
        .unwrap_or_else(|_| serde_json::json!({"status":"UNKNOWN","body":delegated_response.body}));
    let delegated_status = delegated_json["status"].as_str().unwrap_or("UNKNOWN");
    let receipt_ids = delegated_json["run_started_receipt_id"]
        .as_str()
        .filter(|receipt_id| !receipt_id.trim().is_empty())
        .map(|receipt_id| vec![receipt_id.to_string()])
        .unwrap_or_default();
    let status = if delegated_status == "REFUSED" {
        "REFUSED"
    } else {
        "MISSION_LAUNCH_DELEGATED"
    };
    let related_run_id = delegated_json["run_id"].as_str().unwrap_or("");
    let mut launch_checkpoint_recorded = false;
    let mut launch_checkpoint_receipt_id = String::new();
    if status != "REFUSED" && !related_run_id.trim().is_empty() {
        let summary = format!("Launched delegated run {related_run_id} for mission milestone");
        let checkpoint = ForgeLongHorizonMissionCheckpoint {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            actor_role: "operator",
            mission_id,
            milestone_id,
            checkpoint_event: "milestone_started",
            summary: &summary,
            validation_status: "running",
            related_run_id,
            related_fleet_id: "",
            steering_note: "",
        };
        let report = kernel
            .write()
            .map_err(|_| {
                "kernel lock poisoned while recording mission launch checkpoint".to_string()
            })?
            .record_forge_long_horizon_mission_checkpoint_local_with_identity(
                checkpoint,
                &resolved.identity,
            )
            .map_err(|error| error.message())?;
        launch_checkpoint_recorded = true;
        launch_checkpoint_receipt_id = report.checkpoint_receipt_id;
    }
    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-long-horizon-mission-launch-local-post",
            "status": status,
            "mission_id": mission_id,
            "milestone_id": milestone_id,
            "shape": launch_shape,
            "delegated_route": delegated_route,
            "delegated_status": delegated_status,
            "delegated_response": delegated_json,
            "receipt_ids": receipt_ids,
            "auto_fleet_plan": auto_fleet_plan,
            "checkpoint_route": "/forge/long-horizon-mission-checkpoints.json",
            "dashboard_route": "/forge/long-horizon-mission-dashboard.json",
            "authority_opened": "delegated_route_only",
            "launch_records_checkpoint": launch_checkpoint_recorded,
            "launch_checkpoint_receipt_id": launch_checkpoint_receipt_id,
            "live_provider_calls_allowed": false,
            "adapter_execution_allowed": false,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

/// A unique mission id for a mission the caller did not name. Deterministic per
/// distinct goal plus checks, and disambiguated by how many missions already
/// exist, so two missions never share an id (which would make launch pick the
/// wrong one).
fn generated_mission_id(goal: &str, validation_commands: &str, existing_count: usize) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in goal.as_bytes().iter().chain(validation_commands.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("forge_mission_{hash:016x}_{}", existing_count + 1)
}

fn mission_launch_shape(
    requested_launch_shape: &str,
    fleet_id: &str,
    requested_workers: u64,
) -> String {
    if !requested_launch_shape.trim().is_empty() {
        return requested_launch_shape.trim().to_ascii_lowercase();
    }
    if !fleet_id.trim().is_empty() || requested_workers > 4 {
        "fleet".to_string()
    } else {
        "run".to_string()
    }
}

fn mission_launch_default_intent(
    mission_id: &str,
    milestone_id: &str,
    goal: &str,
    milestone_title: &str,
    validation_command: &str,
) -> String {
    format!(
        "Mission checkpoint {milestone_id} for {mission_id}. Goal: {goal}. Milestone: {milestone_title}. Validation command: {validation_command}"
    )
}

#[derive(Clone, Debug)]
struct AutoMissionFleetPlan {
    fleet_id: String,
    requested_width: u64,
    effective_stream_count: usize,
    plan_receipt_id: String,
    ratification_receipt_id: String,
}

impl AutoMissionFleetPlan {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "created": true,
            "fleet_id": self.fleet_id,
            "source": "mission_packet_write_scope",
            "requested_width": self.requested_width,
            "effective_stream_count": self.effective_stream_count,
            "plan_receipt_id": self.plan_receipt_id,
            "ratification_receipt_id": self.ratification_receipt_id,
            "authority_opened": "fleet_plan_and_ratification_only",
            "production_write_allowed": false,
        })
    }
}

struct AutoMissionFleetPlanInput<'a> {
    packet: &'a ForgeLongHorizonMissionPacket,
    milestone: &'a ForgeLongHorizonMissionMilestone,
    repo_id: &'a str,
    requested_workers: u64,
    max_turns: u64,
    builder_slot: &'a str,
    checks: &'a [String],
    tenant_id: &'a str,
    actor_id: &'a str,
    identity: &'a mdx_core::GovernedWriteIdentity,
    kernel: &'a Arc<RwLock<MdxKernel>>,
}

fn auto_mission_fleet_plan(
    input: AutoMissionFleetPlanInput<'_>,
) -> Result<AutoMissionFleetPlan, RouteResponse> {
    let AutoMissionFleetPlanInput {
        packet,
        milestone,
        repo_id,
        requested_workers,
        max_turns,
        builder_slot,
        checks,
        tenant_id,
        actor_id,
        identity,
        kernel,
    } = input;
    if requested_workers == 0 || requested_workers > 512 {
        return Err(mission_launch_refusal(
            "mission fleet launch width must be between 1 and 512 workers",
        ));
    }
    let lanes = mission_scope_lanes(&packet.allowed_write_scope);
    if lanes.is_empty() {
        return Err(mission_launch_refusal(
            "mission fleet auto-plan needs concrete allowed_write_scope lanes before Forge can run wide",
        ));
    }
    let effective_stream_count = std::cmp::min(lanes.len(), requested_workers as usize).max(1);
    let selected_lanes = lanes
        .into_iter()
        .take(effective_stream_count)
        .collect::<Vec<_>>();
    let stream_checks = if checks.is_empty() {
        vec![milestone.validation_command.clone()]
    } else {
        checks.to_vec()
    };
    let blocked_paths = mission_scope_lanes(&packet.blocked_paths);
    let max_turns = if max_turns == 0 {
        30
    } else {
        max_turns.min(u32::MAX as u64) as u32
    };
    let streams = selected_lanes
        .iter()
        .enumerate()
        .map(|(index, lane)| FleetStream {
            stream_id: format!("m{}_{}", index + 1, sanitize_stream_token(&milestone.milestone_id)),
            objective: format!(
                "Mission {} milestone {} stream {}/{}.\nGoal: {}\nMilestone: {}\nOwn only {}. Do not touch blocked paths: {}.",
                packet.mission_id,
                milestone.milestone_id,
                index + 1,
                effective_stream_count,
                packet.goal,
                milestone.title,
                lane,
                if packet.blocked_paths.trim().is_empty() {
                    "none named"
                } else {
                    packet.blocked_paths.as_str()
                }
            ),
            write_scope: vec![lane.clone()],
            interface_contract: format!(
                "This stream contributes one safe slice of mission {}. Keep changes inside {}, preserve the milestone validation command {}, and leave cross-lane integration to the fleet conductor.",
                packet.mission_id, lane, milestone.validation_command
            ),
            depends_on: Vec::new(),
            checks: stream_checks.clone(),
            max_turns,
            builder_slot: builder_slot.to_string(),
            data_sensitivity: "internal".to_string(),
        })
        .collect::<Vec<_>>();
    let fleet_id = format!(
        "mission_{}_{}",
        sanitize_stream_token(&packet.mission_id),
        sanitize_stream_token(&milestone.milestone_id)
    );
    let repo_root = {
        let kernel = kernel.read().map_err(|_| {
            mission_launch_refusal("kernel lock poisoned while reading mission fleet repo")
        })?;
        if repo_id.trim().is_empty() {
            std::env::current_dir()
                .ok()
                .and_then(|dir| {
                    dir.ancestors()
                        .find(|candidate| candidate.join(".git").exists())
                        .map(std::path::Path::to_path_buf)
                })
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        } else {
            let Some(root) = kernel.forge_repo_root(repo_id.trim()) else {
                return Err(mission_launch_refusal(&format!(
                    "connect repo {} before launching this mission fleet",
                    repo_id.trim()
                )));
            };
            std::path::PathBuf::from(root)
        }
    };
    let repo_profile = crate::forge_repo_profile::profile_repo(Path::new(&repo_root));
    let suggested_checks = repo_profile.suggested_checks.join(",");
    let artifact_patterns = repo_profile.artifact_patterns.join(",");
    let semantic_intelligence = repo_profile.semantic_intelligence.join(",");
    let semantic_tool_readiness = repo_profile.semantic_tool_readiness.join(",");
    let toolchain_readiness = repo_profile.toolchain_readiness.join(",");
    let proof_plan_summary = repo_profile.proof_plan_summary.clone();
    let mut kernel = kernel.write().map_err(|_| {
        mission_launch_refusal("kernel lock poisoned while recording mission fleet")
    })?;
    let draft = match kernel.record_fleet_plan_draft_with_identity(
        FleetPlanDraft {
            tenant_id,
            actor_id,
            fleet_id: &fleet_id,
            spec: &format!(
                "Auto fleet for mission {} milestone {}: {}",
                packet.mission_id, milestone.milestone_id, milestone.title
            ),
            goal: &packet.goal,
            checks,
            integration_owned_paths: &blocked_paths,
            full_suite_checks: &stream_checks,
            justification: "mission launch auto-plan from admitted write-scope lanes",
            requested_width: requested_workers as u32,
            planner_model: "mission_packet_auto_plan",
            bet_id: "",
            repo_id,
            repo_primary_language: repo_profile.primary_language,
            language_pack_id: repo_profile.language_pack_id,
            repo_profile_suggested_checks: &suggested_checks,
            repo_profile_artifact_patterns: &artifact_patterns,
            repo_profile_semantic_intelligence: &semantic_intelligence,
            repo_profile_semantic_tool_readiness: &semantic_tool_readiness,
            repo_profile_toolchain_readiness: &toolchain_readiness,
            repo_profile_proof_plan_status: repo_profile.proof_plan_status,
            repo_profile_proof_plan_summary: &proof_plan_summary,
            repo_profile_source: "mission-launch-auto-profile",
            wide_plan_review_required: false,
            wide_plan_review_status: "not_required",
            wide_plan_review_reviewer_model: "",
            wide_plan_review_verdict: "not_required",
            wide_plan_review_confidence: "none",
            wide_plan_review_concerns: "",
        },
        &streams,
        identity,
    ) {
        Ok(report) => report,
        Err(error) => return Err(mission_launch_refusal(&error.message())),
    };
    let ratification = match kernel.ratify_fleet_plan_with_identity(
        tenant_id,
        actor_id,
        &fleet_id,
        "mission launch auto-ratifies an already admitted checkpoint packet",
        identity,
    ) {
        Ok(report) => report,
        Err(error) => return Err(mission_launch_refusal(&error.message())),
    };
    Ok(AutoMissionFleetPlan {
        fleet_id,
        requested_width: requested_workers,
        effective_stream_count,
        plan_receipt_id: draft.receipt_id,
        ratification_receipt_id: ratification.receipt_id,
    })
}

fn mission_scope_lanes(value: &str) -> Vec<String> {
    value
        .split([',', '\n', ';'])
        .map(str::trim)
        .filter(|lane| !lane.is_empty())
        .filter(|lane| mission_scope_lane_is_concrete(lane))
        .map(str::to_string)
        .collect()
}

fn mission_scope_lane_is_concrete(lane: &str) -> bool {
    let lower = lane.to_ascii_lowercase();
    if lower.contains("to be confirmed")
        || lower.contains("provider secret")
        || lower.contains("production state")
        || lower.contains("live adapter")
    {
        return false;
    }
    lane.contains('/') || lane.contains('.') || lane.ends_with("**")
}

fn sanitize_stream_token(value: &str) -> String {
    let token = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(48)
        .collect::<String>();
    if token.is_empty() {
        "mission".to_string()
    } else {
        token
    }
}

fn string_array_or_single(parsed: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    if let Some(values) = parsed[key].as_array() {
        return Some(
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
        );
    }
    let value = parsed[key].as_str()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(vec![value.to_string()])
    }
}

fn mission_launch_refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-long-horizon-mission-launch-local-post",
            "status": "REFUSED",
            "reason": reason,
            "delegated_route": "",
            "delegated_status": "not_delegated",
            "receipt_ids": [],
            "checkpoint_route": "/forge/long-horizon-mission-checkpoints.json",
            "dashboard_route": "/forge/long-horizon-mission-dashboard.json",
            "live_provider_calls_allowed": false,
            "adapter_execution_allowed": false,
            "production_write_allowed": false,
        })
        .to_string(),
    )
}

fn render_mission_checkpoint_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let mission_id = json_string_field(body, "mission_id")
        .unwrap_or_else(|| "forge_long_horizon_mission_local_001".to_string());
    let milestone_id = json_string_field(body, "milestone_id")
        .or_else(|| json_string_field(body, "mission_milestone_id"))
        .unwrap_or_default();
    let checkpoint_event = json_string_field(body, "checkpoint_event")
        .unwrap_or_else(|| "mission_steered".to_string());
    let summary = json_string_field(body, "summary")
        .unwrap_or_else(|| "Operator checkpoint recorded".to_string());
    let validation_status = json_string_field(body, "validation_status").unwrap_or_default();
    let related_run_id = json_string_field(body, "related_run_id").unwrap_or_default();
    let related_fleet_id = json_string_field(body, "related_fleet_id").unwrap_or_default();
    let steering_note = json_string_field(body, "steering_note").unwrap_or_default();
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:codex",
        "operator",
    );
    let report = match kernel.record_forge_long_horizon_mission_checkpoint_local_with_identity(
        ForgeLongHorizonMissionCheckpoint {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            actor_role: &resolved.actor_role,
            mission_id: &mission_id,
            milestone_id: &milestone_id,
            checkpoint_event: &checkpoint_event,
            summary: &summary,
            validation_status: &validation_status,
            related_run_id: &related_run_id,
            related_fleet_id: &related_fleet_id,
            steering_note: &steering_note,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(mission_checkpoint_refusal_json(
                &error.message(),
                resolved.auth_session_status,
                &mission_id,
                &milestone_id,
                &checkpoint_event,
            ));
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-forge-long-horizon-mission-checkpoint-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/long-horizon-mission-checkpoints.json","projection_route":"/forge/long-horizon-missions/projection.json","dashboard_route":"/forge/long-horizon-mission-dashboard.json","mission_id":{},"milestone_id":{},"checkpoint_event":{},"checkpoint_receipt_id":{},"policy_decision_id":{},"mission_state":{},"live_provider_calls_allowed":{},"adapter_execution_allowed":{},"production_write_allowed":{},"live_substrate_required":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.mission_id),
        json_string_literal(&report.milestone_id),
        json_string_literal(&report.checkpoint_event),
        json_string_literal(&report.checkpoint_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(report.mission_state),
        report.live_provider_calls_allowed,
        report.adapter_execution_allowed,
        report.production_write_allowed,
    ))
}

fn mission_checkpoint_refusal_json(
    reason: &str,
    auth_session_status: &str,
    mission_id: &str,
    milestone_id: &str,
    checkpoint_event: &str,
) -> String {
    format!(
        r#"{{"name":"mdx-forge-long-horizon-mission-checkpoint-local-post","status":"REFUSED","reason":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/long-horizon-mission-checkpoints.json","projection_route":"/forge/long-horizon-missions/projection.json","dashboard_route":"/forge/long-horizon-mission-dashboard.json","mission_id":{},"milestone_id":{},"checkpoint_event":{},"checkpoint_receipt_id":"","policy_decision_id":"","mission_state":"UNKNOWN","receipt_ids":[],"live_provider_calls_allowed":false,"adapter_execution_allowed":false,"production_write_allowed":false,"live_substrate_required":false}}"#,
        json_string_literal(reason),
        json_string_literal(auth_session_status),
        json_string_literal(mission_id),
        json_string_literal(milestone_id),
        json_string_literal(checkpoint_event),
    )
}

fn render_mission_projection_json(kernel: &MdxKernel) -> String {
    let dashboard = kernel.project_forge_long_horizon_missions();
    format!(
        r#"{{"name":"mdx-forge-long-horizon-mission-local-projection","status":"OK","route":"/forge/long-horizon-missions/projection.json","writes_route":"/forge/long-horizon-missions.json","checkpoint_route":"/forge/long-horizon-mission-checkpoints.json","dashboard_route":"/forge/long-horizon-mission-dashboard.json","mission_count":{},"active_mission_count":{},"live_provider_calls_allowed":false,"adapter_execution_allowed":false,"production_write_allowed":false,"missions":[{}]}}"#,
        dashboard.mission_count,
        dashboard.active_mission_count,
        packets_json(&dashboard),
    )
}

pub(crate) fn render_mission_dashboard_json(kernel: &MdxKernel) -> String {
    let dashboard = kernel.project_forge_long_horizon_missions();
    format!(
        r#"{{"name":"mdx-forge-long-horizon-mission-dashboard","status":{},"route":"/forge/long-horizon-mission-dashboard.json","writes_route":"/forge/long-horizon-missions.json","checkpoint_route":"/forge/long-horizon-mission-checkpoints.json","projection_route":"/forge/long-horizon-missions/projection.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","provider_preflight_route":"/forge/fleet-eval-provider-preflight.json","live_run_approval_route":"/forge/fleet-eval-live-run-approvals.json","mission_count":{},"active_mission_count":{},"milestone_count":{},"completed_milestone_count":{},"blocked_milestone_count":{},"verification_cadence":{},"dashboard_ready":{},"live_provider_calls_allowed":{},"adapter_execution_allowed":{},"production_write_allowed":{},"human_checkpoint_required":{},"provider_matrix_count":{},"benchmark_task_count":{},"scoring_dimension_count":{},"runner_profile_count":{},"openai_pattern_packets":["spec_packet","milestone_plan","implementation_runbook","live_status_log","verification_after_each_milestone","checkpoint_control_log"],"dashboard_signals":["goal","milestones","receipts","validation","budget","models","fleet_width","blocked_authority","human_checkpoint","pause_steer_resume"],"source_paths":["docs/FORGE-LONG-HORIZON-MISSIONS.md","generated/architecture/forge-long-horizon-missions.json","crates/mdx-core/src/harness_long_horizon_mission.rs","crates/mdx-server/src/forge_long_horizon_mission.rs"],"missions":[{}]}}"#,
        json_string_literal(dashboard.status),
        dashboard.mission_count,
        dashboard.active_mission_count,
        dashboard.milestone_count,
        dashboard.completed_milestone_count,
        dashboard.blocked_milestone_count,
        json_string_literal(dashboard.verification_cadence),
        dashboard.dashboard_ready,
        dashboard.live_provider_calls_allowed,
        dashboard.adapter_execution_allowed,
        dashboard.production_write_allowed,
        dashboard.human_checkpoint_required,
        dashboard.provider_matrix_count,
        dashboard.benchmark_task_count,
        dashboard.scoring_dimension_count,
        dashboard.runner_profile_count,
        packets_json(&dashboard),
    )
}

fn packets_json(dashboard: &ForgeLongHorizonMissionDashboard) -> String {
    dashboard
        .packets
        .iter()
        .map(packet_json)
        .collect::<Vec<_>>()
        .join(",")
}

fn packet_json(packet: &ForgeLongHorizonMissionPacket) -> String {
    format!(
        r#"{{"mission_id":{},"goal":{},"non_goals":{},"constraints":{},"done_when":{},"allowed_write_scope":{},"blocked_paths":{},"validation_commands":{},"model_policy":{},"provider_allowlist":{},"fleet_width":{},"max_runtime_ms":{},"max_cost_cents":{},"checkpoint_cadence_minutes":{},"mission_receipt_id":{},"policy_decision_id":{},"mission_state":{},"latest_checkpoint_event":{},"latest_checkpoint_summary":{},"checkpoint_count":{},"completed_milestone_count":{},"blocked_milestone_count":{},"steering_note_count":{},"pause_count":{},"resume_count":{},"related_run_ids":[{}],"related_fleet_ids":[{}],"milestones":[{}]}}"#,
        json_string_literal(&packet.mission_id),
        json_string_literal(&packet.goal),
        json_string_literal(&packet.non_goals),
        json_string_literal(&packet.constraints),
        json_string_literal(&packet.done_when),
        json_string_literal(&packet.allowed_write_scope),
        json_string_literal(&packet.blocked_paths),
        json_string_literal(&packet.validation_commands),
        json_string_literal(&packet.model_policy),
        json_string_literal(&packet.provider_allowlist),
        packet.fleet_width,
        packet.max_runtime_ms,
        packet.max_cost_cents,
        packet.checkpoint_cadence_minutes,
        json_string_literal(&packet.mission_receipt_id),
        json_string_literal(&packet.policy_decision_id),
        json_string_literal(packet.mission_state),
        json_string_literal(&packet.latest_checkpoint_event),
        json_string_literal(&packet.latest_checkpoint_summary),
        packet.checkpoint_count,
        packet.completed_milestone_count,
        packet.blocked_milestone_count,
        packet.steering_note_count,
        packet.pause_count,
        packet.resume_count,
        json_string_array(&packet.related_run_ids),
        json_string_array(&packet.related_fleet_ids),
        mission_milestones_json(packet),
    )
}

fn json_string_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after_key = body.split(&needle).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn json_u32_field(body: &str, key: &str) -> Option<u32> {
    json_u64_field(body, key).and_then(|value| u32::try_from(value).ok())
}

fn json_u64_field(body: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let after_key = body.split(&needle).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let digits = after_colon
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_mission_ids_are_unique_per_mission() {
        // Distinct goals get distinct ids.
        let a = generated_mission_id("goal one", "node a.test.mjs", 0);
        let b = generated_mission_id("goal two", "node b.test.mjs", 1);
        assert_ne!(a, b);
        // Even the same goal shaped again (existing count advanced) does not
        // collide, so launch never targets the wrong same-id mission.
        let first = generated_mission_id("same goal", "node x.test.mjs", 0);
        let second = generated_mission_id("same goal", "node x.test.mjs", 1);
        assert_ne!(first, second);
        assert!(first.starts_with("forge_mission_"));
    }

    #[test]
    fn admission_without_mission_id_does_not_reuse_the_default() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let shape = |goal: &str| {
            let body = format!(
                r#"{{"goal":"{goal}","validation_commands":"node apps/mdx-host/test/x.test.mjs"}}"#
            );
            let response =
                route_response("POST", "/forge/long-horizon-missions.json", &body, &kernel)
                    .expect("admission route")
                    .expect("admission response");
            let parsed: serde_json::Value = serde_json::from_str(&response.body).expect("json");
            parsed["mission_id"].as_str().unwrap_or("").to_string()
        };
        let first = shape("first meaty goal");
        let second = shape("second meaty goal");
        assert_ne!(first, second);
        assert_ne!(first, "forge_long_horizon_mission_local_001");
        assert_ne!(second, "forge_long_horizon_mission_local_001");
    }

    #[test]
    fn checkpoint_route_records_progress_and_projection_state() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let admission = route_response(
            "POST",
            "/forge/long-horizon-missions.json",
            r#"{"mission_id":"route_mission_001","validation_commands":"cargo test -p mdx-core harness_long_horizon_mission,cargo check -p mdx-server"}"#,
            &kernel,
        )
        .expect("admission route")
        .expect("admission response");
        assert_eq!(admission.status, "200 OK");
        assert!(
            admission
                .body
                .contains(r#""status":"FORGE_LONG_HORIZON_MISSION_ADMITTED""#)
        );

        let checkpoint = route_response(
            "POST",
            "/forge/long-horizon-mission-checkpoints.json",
            r#"{"mission_id":"route_mission_001","milestone_id":"mission_milestone_01","checkpoint_event":"milestone_completed","summary":"core proof passed","validation_status":"passed","related_run_id":"forge_run_route_001","related_fleet_id":"forge_fleet_route_001"}"#,
            &kernel,
        )
        .expect("checkpoint route")
        .expect("checkpoint response");
        assert_eq!(checkpoint.status, "200 OK");
        for expected in [
            r#""name":"mdx-forge-long-horizon-mission-checkpoint-local-post""#,
            r#""status":"MISSION_MILESTONE_COMPLETED""#,
            r#""mission_state":"IN_PROGRESS_LOCAL_CHECKPOINTS""#,
            r#""adapter_execution_allowed":false"#,
            r#""production_write_allowed":false"#,
        ] {
            assert!(checkpoint.body.contains(expected), "{expected}");
        }

        let projection = route_response(
            "GET",
            "/forge/long-horizon-missions/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            r#""checkpoint_route":"/forge/long-horizon-mission-checkpoints.json""#,
            r#""mission_state":"IN_PROGRESS_LOCAL_CHECKPOINTS""#,
            r#""completed_milestone_count":1"#,
            r#""related_run_ids":["forge_run_route_001"]"#,
            r#""related_fleet_ids":["forge_fleet_route_001"]"#,
            r#""status":"COMPLETED_LOCAL_CHECKPOINT""#,
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn checkpoint_route_refuses_unknown_mission_as_json() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let checkpoint = route_response(
            "POST",
            "/forge/long-horizon-mission-checkpoints.json",
            r#"{"mission_id":"missing_mission","milestone_id":"mission_milestone_01","checkpoint_event":"milestone_completed","summary":"stale UI checkpoint"}"#,
            &kernel,
        )
        .expect("checkpoint route")
        .expect("checkpoint response");
        assert_eq!(checkpoint.status, "200 OK");
        let value: serde_json::Value = serde_json::from_str(&checkpoint.body).expect("valid json");
        assert_eq!(value["status"], "REFUSED");
        assert_eq!(value["mission_id"], "missing_mission");
        assert_eq!(value["milestone_id"], "mission_milestone_01");
        assert_eq!(value["mission_state"], "UNKNOWN");
        assert_eq!(value["receipt_ids"].as_array().unwrap().len(), 0);
        assert!(
            value["reason"]
                .as_str()
                .unwrap()
                .contains("mission is unknown")
        );
    }

    #[test]
    fn checkpoint_route_accepts_mission_milestone_id_alias() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_response(
            "POST",
            "/forge/long-horizon-missions.json",
            r#"{"mission_id":"alias_mission","validation_commands":"cargo test -p mdx-core"}"#,
            &kernel,
        )
        .expect("admission route")
        .expect("admission response");

        let checkpoint = route_response(
            "POST",
            "/forge/long-horizon-mission-checkpoints.json",
            r#"{"mission_id":"alias_mission","mission_milestone_id":"mission_milestone_01","checkpoint_event":"milestone_completed","summary":"alias worked"}"#,
            &kernel,
        )
        .expect("checkpoint route")
        .expect("checkpoint response");
        let value: serde_json::Value = serde_json::from_str(&checkpoint.body).expect("valid json");
        assert_eq!(value["status"], "MISSION_MILESTONE_COMPLETED");
        assert_eq!(value["milestone_id"], "mission_milestone_01");
    }

    #[test]
    fn mission_launch_delegates_run_with_checkpoint_attachment() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        clear_local_model_configuration_for_launch_refusal_test();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_response(
            "POST",
            "/forge/long-horizon-missions.json",
            r#"{"tenant_id":"tenant_launch_refusal_no_builder","mission_id":"launch_run_mission","validation_commands":"cargo test -p mdx-core"}"#,
            &kernel,
        )
        .expect("admission route")
        .expect("admission response");

        let launch = route_response(
            "POST",
            "/forge/long-horizon-mission-launches.json",
            r#"{"tenant_id":"tenant_launch_refusal_no_builder","mission_id":"launch_run_mission","milestone_id":"mission_milestone_01","shape":"run","repo_id":"","max_turns":1}"#,
            &kernel,
        )
        .expect("launch route")
        .expect("launch response");
        assert_eq!(launch.status, "200 OK");
        let value: serde_json::Value = serde_json::from_str(&launch.body).expect("valid json");
        assert_eq!(value["status"], "REFUSED");
        assert_eq!(value["delegated_route"], "/forge/runs.json");
        assert_eq!(
            value["delegated_response"]["reason"],
            "Connect a model first. A Welcome-connected model, or XAI_API_KEY plus MDX_DEFAULT_MODEL_PROVIDER=xai in the server environment, can power Forge."
        );
        assert_eq!(value["mission_id"], "launch_run_mission");
        assert_eq!(value["milestone_id"], "mission_milestone_01");
    }

    #[test]
    fn mission_launch_records_started_checkpoint_with_delegated_run() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _skip_background_runs = crate::forge_run_route::skip_background_runs_for_test();
        clear_local_model_configuration_for_launch_refusal_test();
        // SAFETY: this test holds ENV_TEST_LOCK while mutating process env.
        unsafe {
            std::env::set_var("MDX_DEFAULT_MODEL_PROVIDER", "xai");
            std::env::set_var("XAI_API_KEY", "xai-test-key");
        }
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_response(
            "POST",
            "/forge/long-horizon-missions.json",
            r#"{"tenant_id":"tenant_launch_checkpoint","actor_id":"agent:codex","mission_id":"launch_checkpoint_mission","validation_commands":"cargo test -p mdx-core","fleet_width":4}"#,
            &kernel,
        )
        .expect("admission route")
        .expect("admission response");

        let launch = route_response(
            "POST",
            "/forge/long-horizon-mission-launches.json",
            r#"{"tenant_id":"tenant_launch_checkpoint","actor_id":"agent:codex","mission_id":"launch_checkpoint_mission","milestone_id":"mission_milestone_01","requested_workers":4,"max_turns":1}"#,
            &kernel,
        )
        .expect("launch route")
        .expect("launch response");
        let value: serde_json::Value = serde_json::from_str(&launch.body).expect("valid json");
        assert_eq!(value["status"], "MISSION_LAUNCH_DELEGATED");
        assert_eq!(value["delegated_response"]["status"], "RUN_STARTED");
        assert_eq!(value["launch_records_checkpoint"], true);
        let run_id = value["delegated_response"]["run_id"]
            .as_str()
            .expect("delegated run id");
        let run_projection = crate::forge_run_route::route_response(
            "GET",
            "/forge/runs/projection.json",
            "",
            &kernel,
        )
        .expect("run projection route")
        .expect("run projection response");
        let run_projection: serde_json::Value =
            serde_json::from_str(&run_projection.body).expect("run projection json");
        let delegated_run = run_projection["runs"]
            .as_array()
            .expect("runs")
            .iter()
            .find(|run| run["run_id"] == run_id)
            .expect("delegated run projection");
        assert_eq!(
            delegated_run["run_title"],
            "Ship a meaty Forge fleet feature through governed checkpoints"
        );
        assert_eq!(delegated_run["terminal_state"], "IN_PROGRESS");

        let projection = route_response(
            "GET",
            "/forge/long-horizon-missions/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        let projection: serde_json::Value =
            serde_json::from_str(&projection.body).expect("projection json");
        let mission = &projection["missions"][0];
        assert_eq!(mission["latest_checkpoint_event"], "milestone_started");
        assert_eq!(mission["related_run_ids"][0], run_id);
        assert_eq!(
            mission["milestones"][0]["status"],
            "RUNNING_LOCAL_CHECKPOINT"
        );
        assert_eq!(mission["milestones"][0]["related_run_id"], run_id);
        clear_local_model_configuration_for_launch_refusal_test();
    }

    fn clear_local_model_configuration_for_launch_refusal_test() {
        crate::secret_store::global().clear_tenant("local_tenant");
        let keys = [
            "MDX_DEFAULT_MODEL_PROVIDER",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "XAI_API_KEY",
            "GEMINI_API_KEY",
            "MDX_OPENAI_MODEL",
            "MDX_ANTHROPIC_MODEL",
            "MDX_XAI_MODEL",
            "MDX_XAI_BUILD_MODEL",
            "MDX_FLEET_BUILDER_GPT_BASE_URL",
            "MDX_FLEET_BUILDER_GPT_API_KEY",
            "MDX_FLEET_BUILDER_GPT_MODEL",
            "MDX_FLEET_BUILDER_OPUS_BASE_URL",
            "MDX_FLEET_BUILDER_OPUS_API_KEY",
            "MDX_FLEET_BUILDER_OPUS_MODEL",
            "MDX_FLEET_BUILDER_GROK_BASE_URL",
            "MDX_FLEET_BUILDER_GROK_API_KEY",
            "MDX_FLEET_BUILDER_GROK_MODEL",
        ];
        // SAFETY: this test holds ENV_TEST_LOCK while mutating process env.
        unsafe {
            for key in keys {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn mission_launch_shape_routes_n_width_to_fleet_geometry() {
        assert_eq!(mission_launch_shape("", "", 1), "run");
        assert_eq!(mission_launch_shape("", "", 4), "run");
        assert_eq!(mission_launch_shape("", "", 5), "fleet");
        assert_eq!(mission_launch_shape("", "ratified_fleet", 1), "fleet");
        assert_eq!(mission_launch_shape("run", "", 64), "run");
        assert_eq!(mission_launch_shape("fleet", "", 1), "fleet");
    }

    #[test]
    fn default_mission_launch_intent_is_operator_readable() {
        let intent = mission_launch_default_intent(
            "mission_123",
            "milestone_1",
            "Refactor the core service",
            "Checkpoint 1",
            "cargo test",
        );
        assert_eq!(
            intent,
            "Mission checkpoint milestone_1 for mission_123. Goal: Refactor the core service. Milestone: Checkpoint 1. Validation command: cargo test"
        );
        assert!(!intent.contains("\\n"));
        assert!(!intent.contains(".nGoal"));
    }

    #[test]
    fn mission_launch_delegates_fleet_with_checkpoint_attachment() {
        let _skip_fleet_conductor = crate::fleet_run_route::skip_fleet_conductor_for_test();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_response(
            "POST",
            "/forge/long-horizon-missions.json",
            r#"{"mission_id":"launch_fleet_mission","validation_commands":"cargo test -p mdx-server"}"#,
            &kernel,
        )
        .expect("admission route")
        .expect("admission response");
        {
            let mut kernel = kernel.write().expect("kernel");
            let streams = vec![mdx_core::FleetStream {
                stream_id: "s1".to_string(),
                objective: "Build one mission stream".to_string(),
                write_scope: vec!["crates/mdx-server/src/main.rs".to_string()],
                interface_contract: "Keep server route behavior stable".to_string(),
                depends_on: Vec::new(),
                checks: vec!["cargo test -p mdx-server forge_long_horizon_mission::tests::mission_launch_delegates_run_with_checkpoint_attachment".to_string()],
                max_turns: 1,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            }];
            kernel
                .record_fleet_plan_draft(
                    mdx_core::FleetPlanDraft {
                        tenant_id: "local_tenant",
                        actor_id: "local_user",
                        fleet_id: "mission_launch_fleet",
                        spec: "Execute a mission milestone as a fleet",
                        goal: "Execute a mission milestone as a fleet",
                        checks: &["cargo test -p mdx-server".to_string()],
                        integration_owned_paths: &[],
                        full_suite_checks: &["cargo test -p mdx-server".to_string()],
                        justification: "mission launch bridge test",
                        requested_width: 1,
                        planner_model: "test",
                        bet_id: "",
                        repo_id: "",
                        repo_primary_language: "rust",
                        language_pack_id: "rust-cargo",
                        repo_profile_suggested_checks: "cargo test -p mdx-server",
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
                    "local_tenant",
                    "local_user",
                    "mission_launch_fleet",
                    "ratified",
                    &mdx_core::GovernedWriteIdentity::local_demo("local_user"),
                )
                .expect("ratify");
        }

        let launch = route_response(
            "POST",
            "/forge/long-horizon-mission-launches.json",
            r#"{"mission_id":"launch_fleet_mission","milestone_id":"mission_milestone_01","shape":"fleet","fleet_id":"mission_launch_fleet"}"#,
            &kernel,
        )
        .expect("launch route")
        .expect("launch response");
        let value: serde_json::Value = serde_json::from_str(&launch.body).expect("valid json");
        assert_eq!(value["status"], "MISSION_LAUNCH_DELEGATED");
        assert_eq!(value["delegated_route"], "/forge/fleet-runs.json");
        assert_eq!(value["delegated_response"]["status"], "FLEET_STARTED");

        let projection = crate::fleet_run_route::route_response(
            "GET",
            "/forge/fleet-runs/projection.json",
            "",
            &kernel,
        )
        .expect("fleet projection route")
        .expect("fleet projection");
        assert!(
            projection
                .body
                .contains(r#""mission_id":"launch_fleet_mission""#)
        );
        assert!(
            projection
                .body
                .contains(r#""milestone_id":"mission_milestone_01""#)
        );
    }

    #[test]
    fn mission_launch_auto_plans_fleet_from_write_scope_lanes() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let repo = temp_repo("auto_wide_mission");
        std::fs::create_dir_all(repo.path.join("Sources")).expect("sources");
        std::fs::create_dir_all(repo.path.join("Tests")).expect("tests");
        std::fs::write(repo.path.join("Package.swift"), "// swift package\n").expect("package");
        std::fs::write(repo.path.join("Sources/App.swift"), "struct App {}\n").expect("source");
        std::fs::write(repo.path.join("Tests/AppTests.swift"), "import Testing\n").expect("test");
        let connect_body = serde_json::json!({
            "repo_id": "auto_wide_repo",
            "label": "Auto Wide Repo",
            "root": repo.path.to_string_lossy(),
            "kind": "local",
            "default_branch": "trunk"
        })
        .to_string();
        crate::forge_repo_route::route_response(
            "POST",
            "/forge/repos.json",
            &connect_body,
            &kernel,
        )
        .expect("repo route")
        .expect("repo connect");
        route_response(
            "POST",
            "/forge/long-horizon-missions.json",
            r#"{"mission_id":"auto_wide_mission","goal":"Run a wide governed mission","done_when":"all lanes have proof","allowed_write_scope":"Sources/App.swift,Tests/AppTests.swift,Package.swift","blocked_paths":"Secrets.plist","validation_commands":"swift test","fleet_width":8}"#,
            &kernel,
        )
        .expect("admission route")
        .expect("admission response");

        let (packet, milestone) = {
            let guard = kernel.read().expect("kernel");
            let dashboard = guard.project_forge_long_horizon_missions();
            let packet = dashboard
                .packets
                .iter()
                .find(|packet| packet.mission_id == "auto_wide_mission")
                .expect("packet")
                .clone();
            let milestone = packet
                .milestones
                .iter()
                .find(|milestone| milestone.milestone_id == "mission_milestone_01")
                .expect("milestone")
                .clone();
            (packet, milestone)
        };
        let identity = mdx_core::GovernedWriteIdentity::local_demo("agent:codex");
        let auto_plan = auto_mission_fleet_plan(AutoMissionFleetPlanInput {
            packet: &packet,
            milestone: &milestone,
            repo_id: "auto_wide_repo",
            requested_workers: 8,
            max_turns: 1,
            builder_slot: "",
            checks: &["swift test".to_string()],
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            identity: &identity,
            kernel: &kernel,
        })
        .expect("auto mission fleet plan");
        assert_eq!(auto_plan.requested_width, 8);
        assert_eq!(auto_plan.effective_stream_count, 3);
        let value = auto_plan.to_json();
        assert_eq!(value["created"], true);
        assert_eq!(value["source"], "mission_packet_write_scope");

        let projection = crate::fleet_plan_route::route_response(
            "GET",
            "/forge/fleet-plans/projection.json",
            "",
            &kernel,
        )
        .expect("fleet plan projection route")
        .expect("fleet plan projection");
        assert!(projection.body.contains(&auto_plan.fleet_id));
        assert!(projection.body.contains(r#""requested_width":8"#));
        assert!(projection.body.contains(r#""stream_count":3"#));
    }

    struct TempRepo {
        path: std::path::PathBuf,
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn temp_repo(label: &str) -> TempRepo {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mdx_long_horizon_{label}_{}_{}",
            std::process::id(),
            nonce
        ));
        std::fs::create_dir_all(path.join(".git")).expect("git");
        TempRepo { path }
    }
}
