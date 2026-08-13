// Forge Builder Loops over HTTP. The catalog is static and the state fold is
// receipt-derived; ticking records intent but does not launch execution yet.
use crate::RouteResponse;
use mdx_core::{
    BUILDER_LOOP_TEMPLATES, BUILDER_LOOP_TRIGGERS, BuilderLoopCheckerAttachment,
    BuilderLoopExternalTrialAttachment, BuilderLoopFleetAttachment, BuilderLoopRunAttachment,
    BuilderLoopState, BuilderLoopTemplate, BuilderLoopTick, MdxKernel, builder_loop_template,
    json_string_literal,
};
use serde_json::Value;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/builder-loops.json" => Some(render_catalog(method, kernel)),
        "/forge/builder-loops/tick.json" => Some(record_tick(method, body, kernel)),
        _ => None,
    }
}

fn render_catalog(method: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let states = kernel.builder_loop_states();
    let loops = BUILDER_LOOP_TEMPLATES
        .iter()
        .map(|template| {
            let fallback = BuilderLoopState {
                loop_id: template.id.to_string(),
                status: "READY_FOR_TICK".to_string(),
                tick_count: 0,
                last_tick_id: String::new(),
                last_tick_receipt_id: String::new(),
                last_trigger: String::new(),
                blocked_reason: String::new(),
                needs_human: false,
                attached_run_id: String::new(),
                run_admission_status: String::new(),
                run_started_receipt_id: String::new(),
                run_projection_route: String::new(),
                stream_route: String::new(),
                attached_fleet_run_id: String::new(),
                fleet_plan_status: String::new(),
                fleet_plan_receipt_id: String::new(),
                fleet_run_status: String::new(),
                fleet_started_receipt_id: String::new(),
                fleet_plan_projection_route: String::new(),
                fleet_projection_route: String::new(),
                fleet_capacity_route: String::new(),
                control_plane_route: String::new(),
                fleet_lane_count: 0,
                fleet_lanes_needing_attention: 0,
                fleet_active_worker_count: 0,
                fleet_queue_depth: 0,
                fleet_repair_started_count: 0,
                fleet_repair_finished_count: 0,
                fleet_repair_target_count: 0,
                fleet_repair_repaired_count: 0,
                fleet_integration_state: String::new(),
                fleet_review_verdict: String::new(),
                fleet_wide_review_required: false,
                fleet_human_ratification_required: false,
                external_runner_profile_id: String::new(),
                external_codex_profile_id: String::new(),
                external_adapter_status: String::new(),
                external_machine_status: String::new(),
                external_provider_preflight_status: String::new(),
                external_result_ingestion_status: String::new(),
                external_result_receipt_id: String::new(),
                external_output_artifact_ref: String::new(),
                external_transcript_ref: String::new(),
                external_machine_route: String::new(),
                external_provider_preflight_route: String::new(),
                external_result_ingestion_route: String::new(),
                external_result_projection_route: String::new(),
                external_scoreboard_route: String::new(),
                external_output_quarantined: false,
                external_output_consumable: false,
                external_mdx_quality_gates_required: false,
                external_mdx_quality_gates_passed: false,
                external_accepted_for_scoreboard: false,
                attached_mission_id: String::new(),
                attached_review_packet_id: String::new(),
                review_packet_route: String::new(),
                checker_status: String::new(),
                checker_verdict: String::new(),
                checker_confidence: String::new(),
                checker_attachment_receipt_id: String::new(),
                checker_panel_receipt_id: String::new(),
                checker_blocked_reason: String::new(),
                review_panel_route: "/forge/review-panel.json".to_string(),
                source_receipt_ids: Vec::new(),
            };
            let state = states
                .iter()
                .find(|state| state.loop_id == template.id)
                .unwrap_or(&fallback);
            render_loop(template, state)
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-builder-loops","status":"LIVE-LOCAL-BUILDER-LOOP-EXTERNAL-TRIAL","route":"/forge/builder-loops.json","tick_route":"/forge/builder-loops/tick.json","legacy_recipes_route":"/forge/recipes.json","source_contract":"docs/FORGE-BUILDER-LOOP-GAMEPLAN.md","template_count":{},"tick_count":{},"triggers":{},"builder_loops":[{}],"receipt_backed":true,"golden_loop_launch_allowed":true,"fleet_loop_launch_allowed":true,"external_trial_allowed":true,"checker_gate_required":true,"maker_can_self_accept":false,"launch_execution_allowed":true,"adapter_execution_allowed":false,"production_write_allowed":false}}"#,
            BUILDER_LOOP_TEMPLATES.len(),
            states.iter().map(|state| state.tick_count).sum::<usize>(),
            json_array(BUILDER_LOOP_TRIGGERS),
            loops,
        ),
    ))
}

fn record_tick(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let loop_id = json_string_field(body, "loop_id").unwrap_or_default();
    let trigger = json_string_field(body, "trigger").unwrap_or_else(|| "manual".to_string());
    let reason = json_string_field(body, "reason").unwrap_or_default();
    let tenant_id =
        json_string_field(body, "tenant_id").unwrap_or_else(|| "tenant_local".to_string());
    let actor_id = json_string_field(body, "actor_id").unwrap_or_else(|| "local_user".to_string());
    let Some(template) = builder_loop_template(&loop_id) else {
        let known = BUILDER_LOOP_TEMPLATES
            .iter()
            .map(|template| template.id)
            .collect::<Vec<_>>();
        return Ok(refusal(
            &format!("unknown Builder Loop template {}", loop_id.trim()),
            &known,
        ));
    };
    let report = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        match kernel.record_builder_loop_tick(BuilderLoopTick {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            loop_id: &loop_id,
            trigger: &trigger,
            reason: &reason,
        }) {
            Ok(report) => report,
            Err(error) => return Ok(refusal(&error.message(), &[])),
        }
    };

    let launch_execution = json_bool_field(body, "launch_execution").unwrap_or(true);
    let attachment = if launch_execution && golden_loop_launchable(template.id) {
        Some(attempt_run_attachment(
            kernel,
            &tenant_id,
            &actor_id,
            template,
            &report.tick_id,
            &report.tick_receipt_id,
            body,
        )?)
    } else if launch_execution && fleet_loop_launchable(template.id) {
        Some(attempt_fleet_attachment(
            kernel,
            &tenant_id,
            &actor_id,
            template,
            &report.tick_id,
            &report.tick_receipt_id,
            body,
        )?)
    } else if launch_execution && external_trial_launchable(template.id) {
        Some(attempt_external_trial_attachment(
            kernel,
            &tenant_id,
            &actor_id,
            template,
            &report.tick_id,
            &report.tick_receipt_id,
            body,
        )?)
    } else {
        None
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-builder-loop-tick-local-post","status":{},"route":"/forge/builder-loops/tick.json","projection_route":"/forge/builder-loops.json","loop_id":{},"tick_id":{},"tick_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"blocked_reason":{},"golden_loop_launch_attempted":{},"attachment":{},"launch_execution_allowed":{},"adapter_execution_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(report.status),
            json_string_literal(&report.loop_id),
            json_string_literal(&report.tick_id),
            json_string_literal(&report.tick_receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
            json_string_literal(report.blocked_reason),
            attachment.is_some(),
            attachment.unwrap_or_else(|| "null".to_string()),
            launch_execution,
        ),
    ))
}

fn attempt_run_attachment(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    template: &BuilderLoopTemplate,
    tick_id: &str,
    tick_receipt_id: &str,
    tick_body: &str,
) -> Result<String, String> {
    let requested_run_id = format!("forge_run_{tick_id}");
    let run_body =
        golden_loop_run_body(template, &requested_run_id, tick_body, tenant_id, actor_id);
    let run_response =
        crate::forge_run_route::route_response("POST", "/forge/runs.json", &run_body, kernel)
            .ok_or_else(|| "Forge run route missing".to_string())??;
    let run_status = json_string_field(&run_response.body, "status").unwrap_or_default();
    let run_id = json_string_field(&run_response.body, "run_id").unwrap_or_default();
    let run_started_receipt_id =
        json_string_field(&run_response.body, "run_started_receipt_id").unwrap_or_default();
    let blocked_reason = json_string_field(&run_response.body, "reason").unwrap_or_default();
    let attached_run_id = if run_status == "RUN_STARTED" {
        run_id.clone()
    } else {
        String::new()
    };
    let stream_route = if attached_run_id.is_empty() {
        String::new()
    } else {
        format!("/forge/runs/{attached_run_id}/stream")
    };
    let review_packet_route = if attached_run_id.is_empty() {
        String::new()
    } else {
        format!("/forge/review-packets/{attached_run_id}.json")
    };
    let attachment = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .record_builder_loop_run_attachment(BuilderLoopRunAttachment {
                tenant_id,
                actor_id,
                loop_id: template.id,
                tick_receipt_id,
                run_admission_status: if run_status == "RUN_STARTED" {
                    "RUN_STARTED"
                } else {
                    "RUN_ADMISSION_REFUSED"
                },
                run_id: &attached_run_id,
                run_started_receipt_id: &run_started_receipt_id,
                blocked_reason: &blocked_reason,
                run_projection_route: "/forge/runs/projection.json",
                stream_route: &stream_route,
                review_packet_route: &review_packet_route,
            })
            .map_err(|error| error.message())?
    };
    let checker = if !attached_run_id.is_empty() {
        Some(record_checker_availability(
            kernel,
            tenant_id,
            actor_id,
            template,
            tick_receipt_id,
            &attached_run_id,
        )?)
    } else {
        None
    };
    Ok(format!(
        r#"{{"status":{},"attachment_receipt_id":{},"run_admission_status":{},"run_id":{},"run_started_receipt_id":{},"run_projection_route":"/forge/runs/projection.json","stream_route":{},"review_packet_route":{},"checker":{},"terminal_state":{},"blocked_reason":{}}}"#,
        json_string_literal(attachment.status),
        json_string_literal(&attachment.attachment_receipt_id),
        json_string_literal(&attachment.run_admission_status),
        json_string_literal(&attachment.run_id),
        json_string_literal(&run_started_receipt_id),
        json_string_literal(&stream_route),
        json_string_literal(&review_packet_route),
        checker.unwrap_or_else(|| "null".to_string()),
        json_string_literal(attachment.terminal_state),
        json_string_literal(&blocked_reason),
    ))
}

fn attempt_fleet_attachment(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    template: &BuilderLoopTemplate,
    tick_id: &str,
    tick_receipt_id: &str,
    tick_body: &str,
) -> Result<String, String> {
    let fleet_id = json_string_field(tick_body, "fleet_id")
        .unwrap_or_else(|| format!("builder_loop_fleet_{tick_id}"));
    let requested_width = json_u32_field(tick_body, "requested_width")
        .unwrap_or(2)
        .clamp(1, 24);
    let plan_body = fleet_loop_plan_body(
        template,
        &fleet_id,
        tick_body,
        tenant_id,
        actor_id,
        requested_width,
    );
    let plan_response = crate::fleet_plan_route::route_response(
        "POST",
        "/forge/fleet-plans.json",
        &plan_body,
        kernel,
    )
    .ok_or_else(|| "Forge fleet plan route missing".to_string())??;
    let plan_json = json_value(&plan_response.body);
    let plan_status = json_string_path(&plan_json, &["status"]);
    let plan_receipt_id = json_string_path(&plan_json, &["plan_receipt_id"]);
    let wide_review_required = json_bool_path(&plan_json, &["wide_plan_review", "required"]);

    let ratify_body = format!(
        r#"{{"tenant_id":{},"actor_id":{},"fleet_id":{},"reason":"Builder Loop manual tick ratifies the local fleet plan before conductor start"}}"#,
        json_string_literal(tenant_id),
        json_string_literal(actor_id),
        json_string_literal(&fleet_id),
    );
    let ratify_response = crate::fleet_plan_route::route_response(
        "POST",
        "/forge/fleet-plan-ratifications.json",
        &ratify_body,
        kernel,
    )
    .ok_or_else(|| "Forge fleet ratification route missing".to_string())??;
    let ratify_json = json_value(&ratify_response.body);
    let ratify_status = json_string_path(&ratify_json, &["status"]);

    let run_body = format!(
        r#"{{"tenant_id":{},"actor_id":{},"fleet_id":{}}}"#,
        json_string_literal(tenant_id),
        json_string_literal(actor_id),
        json_string_literal(&fleet_id),
    );
    let run_response =
        crate::fleet_run_route::route_response("POST", "/forge/fleet-runs.json", &run_body, kernel)
            .ok_or_else(|| "Forge fleet run route missing".to_string())??;
    let run_json = json_value(&run_response.body);
    let run_status = json_string_path(&run_json, &["status"]);
    let fleet_started_receipt_id = json_string_path(&run_json, &["run_started_receipt_id"]);
    let blocked_reason = json_string_path(&run_json, &["reason"]);

    let fleet_projection = crate::fleet_run_route::route_response(
        "GET",
        "/forge/fleet-runs/projection.json",
        "",
        kernel,
    )
    .ok_or_else(|| "Forge fleet run projection route missing".to_string())??;
    let fleet_projection_json = json_value(&fleet_projection.body);
    let fleet_capacity =
        crate::fleet_run_route::route_response("GET", "/forge/fleet-capacity.json", "", kernel)
            .ok_or_else(|| "Forge fleet capacity route missing".to_string())??;
    let fleet_capacity_json = json_value(&fleet_capacity.body);
    let control_plane_json = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        json_value(
            &crate::forge_control_plane_projection::render_forge_control_plane_projection_json(
                &kernel,
            )?,
        )
    };

    let mut rollup = fleet_rollup(
        &fleet_id,
        &fleet_projection_json,
        &fleet_capacity_json,
        &control_plane_json,
    );
    if rollup.lane_count == 0 {
        rollup.lane_count =
            json_u32_path(&run_json, &["stream_count"]).max(stream_count_hint(requested_width));
    }
    let effective_plan_status = if ratify_status == "RATIFIED" {
        "RATIFIED".to_string()
    } else {
        plan_status
    };
    let effective_run_status = if run_status == "FLEET_STARTED" {
        "FLEET_STARTED".to_string()
    } else {
        "FLEET_ADMISSION_REFUSED".to_string()
    };
    let attachment = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .record_builder_loop_fleet_attachment(BuilderLoopFleetAttachment {
                tenant_id,
                actor_id,
                loop_id: template.id,
                tick_receipt_id,
                fleet_id: &fleet_id,
                fleet_plan_status: &effective_plan_status,
                fleet_plan_receipt_id: &plan_receipt_id,
                fleet_run_status: &effective_run_status,
                fleet_started_receipt_id: &fleet_started_receipt_id,
                blocked_reason: &blocked_reason,
                fleet_plan_projection_route: "/forge/fleet-plans/projection.json",
                fleet_projection_route: "/forge/fleet-runs/projection.json",
                fleet_capacity_route: "/forge/fleet-capacity.json",
                control_plane_route: "/forge/control-plane/projection.json",
                lane_count: rollup.lane_count,
                lanes_needing_attention: rollup.lanes_needing_attention,
                active_worker_count: rollup.active_worker_count,
                queue_depth: rollup.queue_depth,
                repair_started_count: rollup.repair_started_count,
                repair_finished_count: rollup.repair_finished_count,
                repair_target_count: rollup.repair_target_count,
                repair_repaired_count: rollup.repair_repaired_count,
                integration_state: &rollup.integration_state,
                review_verdict: &rollup.review_verdict,
                wide_review_required,
                human_ratification_required: true,
            })
            .map_err(|error| error.message())?
    };
    Ok(format!(
        r#"{{"status":{},"attachment_receipt_id":{},"fleet_id":{},"fleet_plan_status":{},"fleet_plan_receipt_id":{},"fleet_run_status":{},"fleet_started_receipt_id":{},"fleet_plan_projection_route":"/forge/fleet-plans/projection.json","fleet_projection_route":"/forge/fleet-runs/projection.json","fleet_capacity_route":"/forge/fleet-capacity.json","control_plane_route":"/forge/control-plane/projection.json","lane_count":{},"lanes_needing_attention":{},"active_worker_count":{},"queue_depth":{},"integration_state":{},"review_verdict":{},"terminal_state":{},"blocked_reason":{},"human_ratification_required":true}}"#,
        json_string_literal(attachment.status),
        json_string_literal(&attachment.fleet_attachment_receipt_id),
        json_string_literal(&attachment.fleet_id),
        json_string_literal(&effective_plan_status),
        json_string_literal(&plan_receipt_id),
        json_string_literal(&attachment.fleet_run_status),
        json_string_literal(&fleet_started_receipt_id),
        rollup.lane_count,
        rollup.lanes_needing_attention,
        rollup.active_worker_count,
        rollup.queue_depth,
        json_string_literal(&rollup.integration_state),
        json_string_literal(&rollup.review_verdict),
        json_string_literal(attachment.terminal_state),
        json_string_literal(&blocked_reason),
    ))
}

fn attempt_external_trial_attachment(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    template: &BuilderLoopTemplate,
    tick_id: &str,
    tick_receipt_id: &str,
    tick_body: &str,
) -> Result<String, String> {
    // The ceremonial external-machine packet this once read was a
    // deterministic fail-closed rendering (deleted with the DXR/Talent
    // generation). Its three descriptive facts were constants in
    // practice; they stay honest constants here - the LIVE posture
    // authority is the fleet-eval provider preflight consulted below.
    let external_machine_status = "EXTERNAL_MACHINE_BLOCKED".to_string();
    let codex_profile_id = "codex_cli_external_worker".to_string();
    let adapter_status = "adapter_declared_fail_closed".to_string();
    let provider_preflight = crate::forge_fleet_eval_scoreboard::route_response(
        "GET",
        "/forge/fleet-eval-provider-preflight.json",
        "",
        kernel,
    )
    .ok_or_else(|| "Forge fleet eval provider preflight route missing".to_string())??;
    let provider_preflight_json = json_value(&provider_preflight.body);
    let provider_preflight_status = json_string_path(&provider_preflight_json, &["status"]);

    let runner_profile_id = json_string_field(tick_body, "runner_profile_id")
        .unwrap_or_else(|| "codex_cli_external_worker".to_string());
    let model_profile_id = json_string_field(tick_body, "model_profile_id")
        .unwrap_or_else(|| "codex_anthropic_responses_profile".to_string());
    let submission_id = format!("builder_loop_external_trial_{tick_id}");
    let output_artifact_ref =
        format!("quarantine://forge/fleet-eval/builder-loops/{tick_id}.patch");
    let transcript_ref = format!("quarantine://forge/fleet-eval/builder-loops/{tick_id}.jsonl");
    let pr_handoff_ref = format!("quarantine://forge/fleet-eval/builder-loops/{tick_id}-pr.md");
    let result_body = format!(
        r#"{{"tenant_id":{},"actor_id":{},"actor_role":"worker","submission_id":{},"benchmark_task_id":"bugfix_small","language_task_corpus_id":"rust-cargo-bug-fix-small","language_pack_id":"rust-cargo","repo_family":"Rust Cargo","runner_id":{},"model_profile_id":{},"output_artifact_ref":{},"output_artifact_sha256":"sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","transcript_ref":{},"claimed_check":"cargo test","hidden_check_slot":"unit regression","artifact_noise_expected":"target/**","principal_review_gate_results":"behavior=present;tests=present;stack_idioms=present;compatibility=present;security=present;maintainability=present","standards_source_fingerprints":"AGENTS.md=fnv1a64:0123456789abcdef","artifact_filter_summary":"target/** folded from external trial","diff_language_pack_impact":"rust-cargo","principal_engineer_verdict":"pending_principal_engineer_review","pr_handoff_ref":{}}}"#,
        json_string_literal(tenant_id),
        json_string_literal(actor_id),
        json_string_literal(&submission_id),
        json_string_literal(&runner_profile_id),
        json_string_literal(&model_profile_id),
        json_string_literal(&output_artifact_ref),
        json_string_literal(&transcript_ref),
        json_string_literal(&pr_handoff_ref),
    );
    let result_response = crate::forge_fleet_eval_scoreboard::route_response(
        "POST",
        "/forge/fleet-eval-results.json",
        &result_body,
        kernel,
    )
    .ok_or_else(|| "Forge fleet eval result ingestion route missing".to_string())??;
    let result_json = json_value(&result_response.body);
    let result_ingestion_status = json_string_path(&result_json, &["status"]);
    let result_receipt_id = json_string_path(&result_json, &["result_receipt_id"]);
    let blocked_reason = json_string_path(&result_json, &["blocked_reason"]);
    let output_quarantined = json_bool_path(&result_json, &["output_quarantined"]);
    let external_output_consumable = json_bool_path(&result_json, &["external_output_consumable"]);
    let mdx_quality_gates_required = json_bool_path(&result_json, &["mdx_quality_gates_required"]);
    let mdx_quality_gates_passed = json_bool_path(&result_json, &["mdx_quality_gates_passed"]);
    let accepted_for_scoreboard = json_bool_path(&result_json, &["accepted_for_scoreboard"]);

    let attachment = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .record_builder_loop_external_trial_attachment(BuilderLoopExternalTrialAttachment {
                tenant_id,
                actor_id,
                loop_id: template.id,
                tick_receipt_id,
                runner_profile_id: &runner_profile_id,
                codex_profile_id: &codex_profile_id,
                adapter_status: &adapter_status,
                external_machine_status: &external_machine_status,
                provider_preflight_status: &provider_preflight_status,
                result_ingestion_status: &result_ingestion_status,
                result_receipt_id: &result_receipt_id,
                output_artifact_ref: &output_artifact_ref,
                transcript_ref: &transcript_ref,
                blocked_reason: &blocked_reason,
                external_machine_route: "/forge/fleet-eval-provider-preflight.json",
                provider_preflight_route: "/forge/fleet-eval-provider-preflight.json",
                result_ingestion_route: "/forge/fleet-eval-results.json",
                result_projection_route: "/forge/fleet-eval-results/projection.json",
                scoreboard_route: "/forge/fleet-eval-scoreboard.json",
                output_quarantined,
                external_output_consumable,
                mdx_quality_gates_required,
                mdx_quality_gates_passed,
                accepted_for_scoreboard,
            })
            .map_err(|error| error.message())?
    };
    Ok(format!(
        r#"{{"status":{},"attachment_receipt_id":{},"runner_profile_id":{},"codex_profile_id":{},"adapter_status":{},"external_machine_status":{},"provider_preflight_status":{},"result_ingestion_status":{},"result_receipt_id":{},"output_artifact_ref":{},"transcript_ref":{},"external_machine_route":"/harness/external-machine.json","provider_preflight_route":"/forge/fleet-eval-provider-preflight.json","result_ingestion_route":"/forge/fleet-eval-results.json","result_projection_route":"/forge/fleet-eval-results/projection.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","output_quarantined":{},"external_output_consumable":{},"mdx_quality_gates_required":{},"mdx_quality_gates_passed":{},"accepted_for_scoreboard":{},"terminal_state":{},"blocked_reason":{}}}"#,
        json_string_literal(attachment.status),
        json_string_literal(&attachment.external_trial_attachment_receipt_id),
        json_string_literal(&runner_profile_id),
        json_string_literal(&codex_profile_id),
        json_string_literal(&adapter_status),
        json_string_literal(&external_machine_status),
        json_string_literal(&provider_preflight_status),
        json_string_literal(&result_ingestion_status),
        json_string_literal(&result_receipt_id),
        json_string_literal(&output_artifact_ref),
        json_string_literal(&transcript_ref),
        output_quarantined,
        external_output_consumable,
        mdx_quality_gates_required,
        mdx_quality_gates_passed,
        accepted_for_scoreboard,
        json_string_literal(attachment.terminal_state),
        json_string_literal(&blocked_reason),
    ))
}

fn fleet_loop_plan_body(
    template: &BuilderLoopTemplate,
    fleet_id: &str,
    tick_body: &str,
    tenant_id: &str,
    actor_id: &str,
    requested_width: u32,
) -> String {
    let repo_id = json_string_field(tick_body, "repo_id").unwrap_or_default();
    let selected_check = json_string_field(tick_body, "selected_check")
        .unwrap_or_else(|| "cargo check -p mdx-server".to_string());
    let spec = json_string_field(tick_body, "spec").unwrap_or_else(|| template.goal.to_string());
    let goal = json_string_field(tick_body, "goal").unwrap_or_else(|| template.goal.to_string());
    let stream_count = requested_width.clamp(1, 4);
    let streams = (1..=stream_count)
        .map(|index| {
            format!(
                r#"{{"stream_id":"builder_loop_lane_{}","objective":{},"write_scope":["builder-loop/lane-{}"],"interface_contract":"Stay inside the ratified Builder Loop fleet plan and report receipt-backed proof.","checks":[{}],"max_turns":{},"builder_slot":"","data_sensitivity":"internal"}}"#,
                index,
                json_string_literal(&format!("Lane {index}: {}", spec)),
                index,
                json_string_literal(&selected_check),
                template.max_iterations,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"tenant_id":{},"actor_id":{},"fleet_id":{},"repo_id":{},"spec":{},"goal":{},"requested_width":{},"streams":[{}],"integration_owned_paths":[],"full_suite_checks":[{}],"builder_loop_id":{},"production_write_allowed":false}}"#,
        json_string_literal(tenant_id),
        json_string_literal(actor_id),
        json_string_literal(fleet_id),
        json_string_literal(&repo_id),
        json_string_literal(&spec),
        json_string_literal(&goal),
        requested_width,
        streams,
        json_string_literal(&selected_check),
        json_string_literal(template.id),
    )
}

fn stream_count_hint(requested_width: u32) -> u32 {
    requested_width.clamp(1, 4)
}

fn record_checker_availability(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    template: &BuilderLoopTemplate,
    tick_receipt_id: &str,
    run_id: &str,
) -> Result<String, String> {
    let reviewers = crate::fleet_integration::panel_reviewers(kernel, tenant_id);
    let (checker_status, checker_actor_id, checker_verdict, checker_confidence, blocked_reason) =
        if reviewers.is_empty() {
            (
                "CHECKER_UNAVAILABLE",
                "reviewer_unavailable",
                "",
                "low",
                "no reviewer model is configured",
            )
        } else {
            (
                "CHECKER_PENDING",
                "review_panel",
                "",
                "",
                "fresh context review panel must run before human review",
            )
        };
    let checker = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .record_builder_loop_checker_attachment(BuilderLoopCheckerAttachment {
                tenant_id,
                actor_id,
                loop_id: template.id,
                tick_receipt_id,
                run_id,
                maker_actor_id: actor_id,
                checker_actor_id,
                checker_status,
                checker_verdict,
                checker_confidence,
                checker_panel_receipt_id: "",
                blocked_reason,
                review_panel_route: "/forge/review-panel.json",
            })
            .map_err(|error| error.message())?
    };
    Ok(format!(
        r#"{{"status":{},"checker_attachment_receipt_id":{},"checker_status":{},"checker_verdict":{},"checker_confidence":{},"terminal_state":{},"blocked_reason":{},"review_panel_route":"/forge/review-panel.json","maker_can_self_accept":false}}"#,
        json_string_literal(checker.status),
        json_string_literal(&checker.checker_attachment_receipt_id),
        json_string_literal(&checker.checker_status),
        json_string_literal(&checker.checker_verdict),
        json_string_literal(checker_confidence),
        json_string_literal(checker.terminal_state),
        json_string_literal(blocked_reason),
    ))
}

fn golden_loop_launchable(loop_id: &str) -> bool {
    matches!(loop_id, "quality_sweep" | "local_check_recovery")
}

fn fleet_loop_launchable(loop_id: &str) -> bool {
    loop_id == "fleet_quality_sweep"
}

fn external_trial_launchable(loop_id: &str) -> bool {
    loop_id == "external_machine_trial"
}

fn golden_loop_run_body(
    template: &BuilderLoopTemplate,
    requested_run_id: &str,
    tick_body: &str,
    tenant_id: &str,
    actor_id: &str,
) -> String {
    let intent =
        json_string_field(tick_body, "intent").unwrap_or_else(|| template.goal.to_string());
    let work_item_id = json_string_field(tick_body, "work_item_id")
        .unwrap_or_else(|| format!("builder_loop_{}", template.id));
    let repo_id = json_string_field(tick_body, "repo_id").unwrap_or_default();
    let selected_check = json_string_field(tick_body, "selected_check")
        .unwrap_or_else(|| "cargo build -p mdx-core".to_string());
    format!(
        r#"{{"tenant_id":{},"actor_id":{},"actor_role":"owner","requested_run_id":{},"work_item_id":{},"repo_id":{},"intent":{},"allowed_commands":[{}],"fleet_width":1,"max_turns":{},"builder_loop_id":{},"builder_loop_template_id":{},"human_gate":{},"production_write_allowed":false}}"#,
        json_string_literal(tenant_id),
        json_string_literal(actor_id),
        json_string_literal(requested_run_id),
        json_string_literal(&work_item_id),
        json_string_literal(&repo_id),
        json_string_literal(&intent),
        json_string_literal(&selected_check),
        template.max_iterations,
        json_string_literal(template.id),
        json_string_literal(template.id),
        json_string_literal(template.human_gate),
    )
}

fn refusal(reason: &str, known_loop_ids: &[&str]) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-builder-loop-tick-local-post","status":"BUILDER_LOOP_TICK_REFUSED","route":"/forge/builder-loops/tick.json","projection_route":"/forge/builder-loops.json","blocked_reason":{},"known_loop_ids":{},"launch_execution_allowed":false,"adapter_execution_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_array(known_loop_ids),
        ),
    )
}

fn render_loop(template: &BuilderLoopTemplate, state: &BuilderLoopState) -> String {
    format!(
        r#"{{"id":{},"title":{},"trigger":{},"goal":{},"constraints":{},"stop_condition":{},"authority_level":{},"execution_geometry":{},"allowed_tools":{},"denied_tools":{},"max_iterations":{},"time_budget_minutes":{},"maker_role":{},"checker_role":{},"human_gate":{},"metric":{},"output_policy":{},"legacy_recipe_id":{},"state":{}}}"#,
        json_string_literal(template.id),
        json_string_literal(template.title),
        json_string_literal(template.trigger),
        json_string_literal(template.goal),
        json_array(template.constraints),
        json_string_literal(template.stop_condition),
        json_string_literal(template.authority_level),
        json_string_literal(template.execution_geometry),
        json_array(template.allowed_tools),
        json_array(template.denied_tools),
        template.max_iterations,
        template.time_budget_minutes,
        json_string_literal(template.maker_role),
        json_string_literal(template.checker_role),
        json_string_literal(template.human_gate),
        json_string_literal(template.metric),
        json_string_literal(template.output_policy),
        json_string_literal(template.legacy_recipe_id),
        render_state(state),
    )
}

fn render_state(state: &BuilderLoopState) -> String {
    format!(
        r#"{{"status":{},"tick_count":{},"last_tick_id":{},"last_tick_receipt_id":{},"last_trigger":{},"blocked_reason":{},"needs_human":{},"attached_run_id":{},"run_admission_status":{},"run_started_receipt_id":{},"run_projection_route":{},"stream_route":{},"attached_fleet_run_id":{},"fleet_plan_status":{},"fleet_plan_receipt_id":{},"fleet_run_status":{},"fleet_started_receipt_id":{},"fleet_plan_projection_route":{},"fleet_projection_route":{},"fleet_capacity_route":{},"control_plane_route":{},"fleet_lane_count":{},"fleet_lanes_needing_attention":{},"fleet_active_worker_count":{},"fleet_queue_depth":{},"fleet_repair_started_count":{},"fleet_repair_finished_count":{},"fleet_repair_target_count":{},"fleet_repair_repaired_count":{},"fleet_integration_state":{},"fleet_review_verdict":{},"fleet_wide_review_required":{},"fleet_human_ratification_required":{},"external_runner_profile_id":{},"external_codex_profile_id":{},"external_adapter_status":{},"external_machine_status":{},"external_provider_preflight_status":{},"external_result_ingestion_status":{},"external_result_receipt_id":{},"external_output_artifact_ref":{},"external_transcript_ref":{},"external_machine_route":{},"external_provider_preflight_route":{},"external_result_ingestion_route":{},"external_result_projection_route":{},"external_scoreboard_route":{},"external_output_quarantined":{},"external_output_consumable":{},"external_mdx_quality_gates_required":{},"external_mdx_quality_gates_passed":{},"external_accepted_for_scoreboard":{},"attached_mission_id":{},"attached_review_packet_id":{},"review_packet_route":{},"checker_status":{},"checker_verdict":{},"checker_confidence":{},"checker_attachment_receipt_id":{},"checker_panel_receipt_id":{},"checker_blocked_reason":{},"review_panel_route":{},"source_receipt_ids":{}}}"#,
        json_string_literal(&state.status),
        state.tick_count,
        json_string_literal(&state.last_tick_id),
        json_string_literal(&state.last_tick_receipt_id),
        json_string_literal(&state.last_trigger),
        json_string_literal(&state.blocked_reason),
        state.needs_human,
        json_string_literal(&state.attached_run_id),
        json_string_literal(&state.run_admission_status),
        json_string_literal(&state.run_started_receipt_id),
        json_string_literal(&state.run_projection_route),
        json_string_literal(&state.stream_route),
        json_string_literal(&state.attached_fleet_run_id),
        json_string_literal(&state.fleet_plan_status),
        json_string_literal(&state.fleet_plan_receipt_id),
        json_string_literal(&state.fleet_run_status),
        json_string_literal(&state.fleet_started_receipt_id),
        json_string_literal(&state.fleet_plan_projection_route),
        json_string_literal(&state.fleet_projection_route),
        json_string_literal(&state.fleet_capacity_route),
        json_string_literal(&state.control_plane_route),
        state.fleet_lane_count,
        state.fleet_lanes_needing_attention,
        state.fleet_active_worker_count,
        state.fleet_queue_depth,
        state.fleet_repair_started_count,
        state.fleet_repair_finished_count,
        state.fleet_repair_target_count,
        state.fleet_repair_repaired_count,
        json_string_literal(&state.fleet_integration_state),
        json_string_literal(&state.fleet_review_verdict),
        state.fleet_wide_review_required,
        state.fleet_human_ratification_required,
        json_string_literal(&state.external_runner_profile_id),
        json_string_literal(&state.external_codex_profile_id),
        json_string_literal(&state.external_adapter_status),
        json_string_literal(&state.external_machine_status),
        json_string_literal(&state.external_provider_preflight_status),
        json_string_literal(&state.external_result_ingestion_status),
        json_string_literal(&state.external_result_receipt_id),
        json_string_literal(&state.external_output_artifact_ref),
        json_string_literal(&state.external_transcript_ref),
        json_string_literal(&state.external_machine_route),
        json_string_literal(&state.external_provider_preflight_route),
        json_string_literal(&state.external_result_ingestion_route),
        json_string_literal(&state.external_result_projection_route),
        json_string_literal(&state.external_scoreboard_route),
        state.external_output_quarantined,
        state.external_output_consumable,
        state.external_mdx_quality_gates_required,
        state.external_mdx_quality_gates_passed,
        state.external_accepted_for_scoreboard,
        json_string_literal(&state.attached_mission_id),
        json_string_literal(&state.attached_review_packet_id),
        json_string_literal(&state.review_packet_route),
        json_string_literal(&state.checker_status),
        json_string_literal(&state.checker_verdict),
        json_string_literal(&state.checker_confidence),
        json_string_literal(&state.checker_attachment_receipt_id),
        json_string_literal(&state.checker_panel_receipt_id),
        json_string_literal(&state.checker_blocked_reason),
        json_string_literal(&state.review_panel_route),
        json_array(
            &state
                .source_receipt_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        ),
    )
}

fn json_array(values: &[&str]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string_literal(value))
            .collect::<Vec<_>>()
            .join(",")
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

fn json_bool_field(body: &str, key: &str) -> Option<bool> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?.trim_start();
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        json_string_field(body, key).and_then(|value| match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
    }
}

fn json_u32_field(body: &str, key: &str) -> Option<u32> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?.trim_start();
    let number = after
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if number.is_empty() {
        json_string_field(body, key).and_then(|value| value.parse().ok())
    } else {
        number.parse().ok()
    }
}

fn json_value(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

fn json_string_path(value: &Value, path: &[&str]) -> String {
    let mut current = value;
    for key in path {
        current = current.get(*key).unwrap_or(&Value::Null);
    }
    current.as_str().unwrap_or("").to_string()
}

fn json_bool_path(value: &Value, path: &[&str]) -> bool {
    let mut current = value;
    for key in path {
        current = current.get(*key).unwrap_or(&Value::Null);
    }
    current.as_bool().unwrap_or(false)
}

fn json_u32_path(value: &Value, path: &[&str]) -> u32 {
    let mut current = value;
    for key in path {
        current = current.get(*key).unwrap_or(&Value::Null);
    }
    current.as_u64().unwrap_or_default() as u32
}

#[derive(Clone, Debug, Default)]
struct FleetBuilderLoopRollup {
    lane_count: u32,
    lanes_needing_attention: u32,
    active_worker_count: u32,
    queue_depth: u32,
    repair_started_count: u32,
    repair_finished_count: u32,
    repair_target_count: u32,
    repair_repaired_count: u32,
    integration_state: String,
    review_verdict: String,
}

fn fleet_rollup(
    fleet_id: &str,
    fleet_projection: &Value,
    fleet_capacity: &Value,
    control_plane: &Value,
) -> FleetBuilderLoopRollup {
    let mut rollup = FleetBuilderLoopRollup {
        active_worker_count: json_u32_path(fleet_capacity, &["active_now"]),
        queue_depth: json_u32_path(fleet_capacity, &["queue_depth"]),
        ..FleetBuilderLoopRollup::default()
    };
    if let Some(fleet) = fleet_projection
        .get("fleet_runs")
        .and_then(Value::as_array)
        .and_then(|fleets| {
            fleets
                .iter()
                .find(|fleet| fleet.get("fleet_id").and_then(Value::as_str) == Some(fleet_id))
        })
    {
        let lanes = fleet
            .get("lanes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        rollup.lane_count = lanes.len() as u32;
        rollup.lanes_needing_attention = lanes
            .iter()
            .filter(|lane| lane.get("state").and_then(Value::as_str) != Some("done"))
            .count() as u32;
        rollup.repair_started_count = json_u32_path(fleet, &["runtime_repair", "started_count"]);
        rollup.repair_finished_count = json_u32_path(fleet, &["runtime_repair", "finished_count"]);
        rollup.repair_target_count = json_u32_path(fleet, &["runtime_repair", "target_count"]);
        rollup.repair_repaired_count = json_u32_path(fleet, &["runtime_repair", "repaired_count"]);
        rollup.integration_state = json_string_path(fleet, &["integration_state"]);
        rollup.review_verdict = json_string_path(fleet, &["review_verdict"]);
    }
    if rollup.active_worker_count == 0 {
        rollup.active_worker_count =
            json_u32_path(control_plane, &["capacity_pulse", "active_now"]);
    }
    if rollup.queue_depth == 0 {
        rollup.queue_depth = json_u32_path(control_plane, &["capacity_pulse", "queue_depth"]);
    }
    rollup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_loop_catalog_route_projects_templates_and_state() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response("GET", "/forge/builder-loops.json", "", &kernel)
            .expect("route")
            .expect("response");
        assert!(response.body.contains("\"template_count\":5"));
        assert!(response.body.contains("\"id\":\"quality_sweep\""));
        assert!(response.body.contains("\"id\":\"fleet_quality_sweep\""));
        assert!(response.body.contains("\"id\":\"external_machine_trial\""));
        assert!(response.body.contains("\"status\":\"READY_FOR_TICK\""));
        assert!(
            response
                .body
                .contains("\"legacy_recipes_route\":\"/forge/recipes.json\"")
        );
        assert!(
            response
                .body
                .contains("\"status\":\"LIVE-LOCAL-BUILDER-LOOP-EXTERNAL-TRIAL\"")
        );
        assert!(
            response
                .body
                .contains("\"golden_loop_launch_allowed\":true")
        );
        assert!(response.body.contains("\"checker_gate_required\":true"));
        assert!(response.body.contains("\"fleet_loop_launch_allowed\":true"));
        assert!(response.body.contains("\"external_trial_allowed\":true"));
        assert!(response.body.contains("\"maker_can_self_accept\":false"));
        assert!(response.body.contains("\"launch_execution_allowed\":true"));
    }

    #[test]
    fn builder_loop_tick_route_records_receipt_and_projection_folds_it() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/builder-loops/tick.json",
            r#"{"loop_id":"local_check_recovery","trigger":"manual","reason":"route smoke"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(
            response
                .body
                .contains("\"status\":\"BUILDER_LOOP_TICK_RECORDED_EXECUTION_BLOCKED\"")
        );
        assert!(
            response
                .body
                .contains("\"golden_loop_launch_attempted\":true")
        );
        assert!(
            response
                .body
                .contains("\"run_admission_status\":\"RUN_ADMISSION_REFUSED\"")
        );
        assert!(response.body.contains("\"production_write_allowed\":false"));
        let projection = route_response("GET", "/forge/builder-loops.json", "", &kernel)
            .expect("route")
            .expect("response");
        assert!(projection.body.contains("\"tick_count\":1"));
        assert!(
            projection
                .body
                .contains("\"status\":\"RUN_ADMISSION_REFUSED\"")
        );
        assert!(
            projection
                .body
                .contains("\"run_projection_route\":\"/forge/runs/projection.json\"")
        );
    }

    #[test]
    fn builder_loop_projection_exposes_separate_checker_verdict() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            let tick = kernel
                .record_builder_loop_tick(BuilderLoopTick {
                    tenant_id: "tenant_local",
                    actor_id: "maker",
                    loop_id: "quality_sweep",
                    trigger: "manual",
                    reason: "route projection",
                })
                .expect("tick");
            kernel
                .record_builder_loop_run_attachment(BuilderLoopRunAttachment {
                    tenant_id: "tenant_local",
                    actor_id: "maker",
                    loop_id: "quality_sweep",
                    tick_receipt_id: &tick.tick_receipt_id,
                    run_admission_status: "RUN_STARTED",
                    run_id: "forge_run_builder_loop_route",
                    run_started_receipt_id: "receipt_run_started",
                    blocked_reason: "",
                    run_projection_route: "/forge/runs/projection.json",
                    stream_route: "/forge/runs/forge_run_builder_loop_route/stream",
                    review_packet_route: "/forge/review-packets/forge_run_builder_loop_route.json",
                })
                .expect("attachment");
            let members = vec![mdx_core::PanelMember {
                model: "reviewer-model".to_string(),
                stance: "fresh".to_string(),
                verdict: "needs work".to_string(),
                concern: "missing regression proof".to_string(),
            }];
            let panel = kernel
                .record_forge_review_panel(mdx_core::ForgeReviewPanel {
                    tenant_id: "tenant_local",
                    actor_id: "checker",
                    run_id: "forge_run_builder_loop_route",
                    consensus: "needs_work",
                    confidence: "medium",
                    residency: "local",
                    members: &members,
                    dissent: &[],
                    blind_spots: "missing regression proof",
                })
                .expect("panel");
            kernel
                .record_builder_loop_checker_attachment(BuilderLoopCheckerAttachment {
                    tenant_id: "tenant_local",
                    actor_id: "checker",
                    loop_id: "quality_sweep",
                    tick_receipt_id: &tick.tick_receipt_id,
                    run_id: "forge_run_builder_loop_route",
                    maker_actor_id: "maker",
                    checker_actor_id: "checker",
                    checker_status: "CHECKER_NEEDS_WORK",
                    checker_verdict: "needs work",
                    checker_confidence: "medium",
                    checker_panel_receipt_id: &panel.receipt_id,
                    blocked_reason: "missing regression proof",
                    review_panel_route: "/forge/review-panel.json",
                })
                .expect("checker");
        }
        let projection = route_response("GET", "/forge/builder-loops.json", "", &kernel)
            .expect("route")
            .expect("response");
        assert!(
            projection
                .body
                .contains("\"status\":\"CHECKER_NEEDS_WORK_REPAIR_OR_HUMAN\"")
        );
        assert!(
            projection
                .body
                .contains("\"checker_status\":\"CHECKER_NEEDS_WORK\"")
        );
        assert!(
            projection
                .body
                .contains("\"checker_verdict\":\"needs work\"")
        );
        assert!(
            projection
                .body
                .contains("\"checker_blocked_reason\":\"missing regression proof\"")
        );
    }

    #[test]
    fn builder_loop_tick_route_can_attach_existing_fleet_path() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/builder-loops/tick.json",
            r#"{"loop_id":"fleet_quality_sweep","trigger":"manual","reason":"route fleet smoke","requested_width":2}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(
            response
                .body
                .contains("\"status\":\"BUILDER_LOOP_TICK_RECORDED_EXECUTION_BLOCKED\"")
        );
        assert!(
            response
                .body
                .contains("\"fleet_run_status\":\"FLEET_STARTED\"")
        );
        assert!(
            response
                .body
                .contains("\"fleet_projection_route\":\"/forge/fleet-runs/projection.json\"")
        );
        assert!(
            response
                .body
                .contains("\"control_plane_route\":\"/forge/control-plane/projection.json\"")
        );
        let projection = route_response("GET", "/forge/builder-loops.json", "", &kernel)
            .expect("route")
            .expect("response");
        assert!(
            projection
                .body
                .contains("\"status\":\"FLEET_ATTACHED_WAITING_FOR_CONDUCTOR\"")
        );
        assert!(
            projection
                .body
                .contains("\"fleet_capacity_route\":\"/forge/fleet-capacity.json\"")
        );
        assert!(projection.body.contains("\"fleet_lane_count\":2"));
    }

    #[test]
    fn builder_loop_tick_route_can_quarantine_external_trial_result() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/builder-loops/tick.json",
            r#"{"loop_id":"external_machine_trial","trigger":"manual","reason":"route external trial smoke"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(
            response
                .body
                .contains("\"status\":\"BUILDER_LOOP_TICK_RECORDED_EXECUTION_BLOCKED\"")
        );
        assert!(
            response
                .body
                .contains("\"runner_profile_id\":\"codex_cli_external_worker\"")
        );
        assert!(
            response
                .body
                .contains("\"external_machine_status\":\"EXTERNAL_MACHINE_BLOCKED\"")
        );
        assert!(response.body.contains(
            "\"result_ingestion_status\":\"FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES\""
        ));
        assert!(response.body.contains("\"output_quarantined\":true"));
        assert!(
            response
                .body
                .contains("\"external_output_consumable\":false")
        );
        let projection = route_response("GET", "/forge/builder-loops.json", "", &kernel)
            .expect("route")
            .expect("response");
        assert!(
            projection
                .body
                .contains("\"status\":\"EXTERNAL_TRIAL_QUARANTINED_PENDING_MDX_GATES\"")
        );
        assert!(
            projection.body.contains(
                "\"external_machine_route\":\"/forge/fleet-eval-provider-preflight.json\""
            )
        );
        assert!(
            projection
                .body
                .contains("\"external_result_ingestion_route\":\"/forge/fleet-eval-results.json\"")
        );
        assert!(
            projection
                .body
                .contains("\"external_output_quarantined\":true")
        );
        assert!(
            projection
                .body
                .contains("\"external_accepted_for_scoreboard\":false")
        );
    }

    #[test]
    fn builder_loop_tick_route_refuses_unknown_template() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/builder-loops/tick.json",
            r#"{"loop_id":"not_real"}"#,
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert!(
            response
                .body
                .contains("\"status\":\"BUILDER_LOOP_TICK_REFUSED\"")
        );
        assert!(
            response
                .body
                .contains("\"known_loop_ids\":[\"quality_sweep\"")
        );
    }
}
