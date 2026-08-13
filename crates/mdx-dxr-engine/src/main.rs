use mdx_core::{
    DxrEvidenceRecord, DxrLocalRuntimePacket, DxrLocalRuntimeRequest,
    assemble_local_dxr_runtime_packet, default_local_dxr_runtime_request, json_string_literal,
    render_local_dxr_runtime_packet_json,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;

mod authority_apply;
mod build_performance;
mod capacity;
mod ctx_context;
mod dispatch;
mod durable_state;
mod dynamic_workflow;
mod execution_scheduler;
mod execution_supervision;
mod forge_execution;
mod provider_memory;
mod sandbox_command;
mod sandbox_command_result;
mod sandbox_handoff;
mod sandbox_result_consumption;
mod sandbox_runner;
mod sandbox_session;
mod scale_orchestration;
mod tool_execution;
mod turn_loop;
mod worker_boundary;
mod worker_driver;
mod workflow;
mod workflow_orchestration;
use authority_apply::{
    DxrAuthorityApplyResult, DxrAuthorityApplyRuntime, DxrAuthorityApplyRuntimeEvent,
};
use build_performance::{
    DxrBuildPerformanceResult, DxrBuildPerformanceRuntime, DxrBuildPerformanceRuntimeEvent,
};
use capacity::{DxrCapacityResult, DxrCapacityRuntime, DxrCapacityRuntimeEvent};
use ctx_context::{DxrCtxContextResult, DxrCtxContextRuntime, DxrCtxContextRuntimeEvent};
use dispatch::{DxrDispatchResult, DxrDispatchRuntime, DxrDispatchRuntimeEvent};
use durable_state::DxrDurableStore;
use dynamic_workflow::{
    DxrDynamicWorkflowResult, DxrDynamicWorkflowRuntime, DxrDynamicWorkflowRuntimeEvent,
};
use execution_scheduler::{
    DxrExecutionSchedulerResult, DxrExecutionSchedulerRuntime, DxrExecutionSchedulerRuntimeEvent,
};
use execution_supervision::{
    DxrExecutionSupervisionResult, DxrExecutionSupervisionRuntime,
    DxrExecutionSupervisionRuntimeEvent,
};
use forge_execution::{
    DxrForgeExecutionResult, DxrForgeExecutionRuntime, DxrForgeExecutionRuntimeEvent,
};
use provider_memory::{DxrProviderMemoryEvent, DxrProviderMemoryResult, DxrProviderMemoryRuntime};
use sandbox_command::{
    DxrSandboxCommandResult, DxrSandboxCommandRuntime, DxrSandboxCommandRuntimeEvent,
};
use sandbox_command_result::{
    DxrSandboxCommandResultOutcome, DxrSandboxCommandResultRuntime,
    DxrSandboxCommandResultRuntimeEvent,
};
use sandbox_handoff::{
    DxrSandboxHandoffResult, DxrSandboxHandoffRuntime, DxrSandboxHandoffRuntimeEvent,
};
use sandbox_result_consumption::{
    DxrSandboxResultConsumptionOutcome, DxrSandboxResultConsumptionRuntime,
    DxrSandboxResultConsumptionRuntimeEvent,
};
use sandbox_runner::{
    DxrSandboxRunnerResult, DxrSandboxRunnerRuntime, DxrSandboxRunnerRuntimeEvent,
};
use sandbox_session::{
    DxrSandboxSessionResult, DxrSandboxSessionRuntime, DxrSandboxSessionRuntimeEvent,
};
use scale_orchestration::{
    DxrScaleOrchestrationResult, DxrScaleOrchestrationRuntime, DxrScaleOrchestrationRuntimeEvent,
};
use tool_execution::{
    DxrToolExecutionResult, DxrToolExecutionRuntime, DxrToolExecutionRuntimeEvent,
};
use turn_loop::{DxrWorkerTurnLoopResult, DxrWorkerTurnLoopRuntime, DxrWorkerTurnLoopRuntimeEvent};
use worker_boundary::{
    DxrLiveWorkerExecutionPreflight, DxrWorkerBoundary, live_worker_preflight_event_types,
    parse_live_worker_execution_preflight, parse_worker_boundary,
    render_live_worker_preflight_json, render_worker_boundary_json,
    render_worker_boundary_policy_json, worker_boundary_event_types,
};
use worker_driver::{DxrWorkerDriverResult, DxrWorkerDriverRuntime, DxrWorkerDriverRuntimeEvent};
use workflow::{DxrWorkflowResult, DxrWorkflowRuntime, DxrWorkflowRuntimeEvent};
use workflow_orchestration::{
    DxrWorkflowOrchestrationResult, DxrWorkflowOrchestrationRuntime,
    DxrWorkflowOrchestrationRuntimeEvent,
};

const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";
const LOCAL_CORS_HEADERS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Accept, Content-Type, X-Internal-Auth, X-Tenant-Id, X-User-Id";

fn main() {
    if let Err(error) = run() {
        eprintln!("mdx-dxr-engine error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("local-runtime-packet") => {
            println!("{}", render_default_packet_json()?);
            Ok(())
        }
        Some("serve") => {
            let addr = args.get(2).map(String::as_str).unwrap_or("127.0.0.1:9030");
            serve(addr)
        }
        _ => Err("usage: mdx-dxr-engine local-runtime-packet | serve 127.0.0.1:9030".to_string()),
    }
}

fn render_default_packet_json() -> Result<String, String> {
    let packet = assemble_local_dxr_runtime_packet(default_local_dxr_runtime_request())
        .map_err(|error| error.message())?;
    Ok(render_local_dxr_runtime_packet_json(&packet))
}

fn serve(addr: &str) -> Result<(), String> {
    let listener = TcpListener::bind(addr).map_err(|error| format!("bind {addr}: {error}"))?;
    println!("mdx dxr engine listening on http://{addr}");
    let mut state = DxrRuntimeState::default();
    for stream in listener.incoming() {
        let stream = stream.map_err(|error| format!("incoming connection: {error}"))?;
        handle_connection(stream, &mut state)?;
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream, state: &mut DxrRuntimeState) -> Result<(), String> {
    let mut buffer = [0_u8; 8192];
    let bytes = stream
        .read(&mut buffer)
        .map_err(|error| format!("read request: {error}"))?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let method = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("GET");
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    let response = route(method, path, body, state)?;
    let wire_response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\n{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.content_type,
        LOCAL_CORS_HEADERS,
        response.body.len(),
        response.body
    );
    stream
        .write_all(wire_response.as_bytes())
        .map_err(|error| format!("write response: {error}"))
}

fn route(
    method: &str,
    path: &str,
    body: &str,
    state: &mut DxrRuntimeState,
) -> Result<Response, String> {
    if method.eq_ignore_ascii_case("OPTIONS") {
        return Ok(Response::text("204 No Content", String::new()));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/jobs" | "/v1/dxr/jobs" | "/v1/dxr/execute-local" | "/dxr/execute-local"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_request_packet_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/model-turns" | "/dxr/model-turns")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_model_turn_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/model-provider-observations" | "/dxr/model-provider-observations"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_model_provider_observation_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/provider-failover-proofs" | "/dxr/provider-failover-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_provider_failover_proof_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/multi-judge-proofs" | "/dxr/multi-judge-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_multi_judge_proof_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/worker-boundaries" | "/dxr/worker-boundaries")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_worker_boundary_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/live-worker-execution-preflights" | "/dxr/live-worker-execution-preflights"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_live_worker_preflight_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/worker-execution-proofs" | "/dxr/worker-execution-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_worker_execution_proof_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/worker-turn-loops" | "/dxr/worker-turn-loops")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_worker_turn_loop_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/worker-driver-runs" | "/dxr/worker-driver-runs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_worker_driver_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/tool-execution-proofs" | "/dxr/tool-execution-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_tool_execution_proof_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/capacity-plans" | "/dxr/capacity-plans")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_capacity_plan_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/sandbox-admissions" | "/dxr/sandbox-admissions"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_admission_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/sandbox-adapter-turn-ons" | "/dxr/sandbox-adapter-turn-ons"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_adapter_turn_on_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/external-sandbox-preflights" | "/dxr/external-sandbox-preflights"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_external_sandbox_preflight_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/sandbox-authority-envelopes" | "/dxr/sandbox-authority-envelopes"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_authority_envelope_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/execution-admissions" | "/dxr/execution-admissions"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_execution_admission_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/execution-schedules" | "/dxr/execution-schedules"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_execution_schedule_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/execution-supervision-runs" | "/dxr/execution-supervision-runs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_execution_supervision_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/supervised-sandbox-handoffs" | "/dxr/supervised-sandbox-handoffs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_handoff_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/supervised-sandbox-runners" | "/dxr/supervised-sandbox-runners"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_runner_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/sandbox-sessions" | "/dxr/sandbox-sessions")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_session_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/sandbox-command-preflights" | "/dxr/sandbox-command-preflights"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_command_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/sandbox-command-results" | "/dxr/sandbox-command-results"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_command_result_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/sandbox-result-consumptions" | "/dxr/sandbox-result-consumptions"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_sandbox_result_consumption_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/build-performance-proofs" | "/dxr/build-performance-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_build_performance_proof_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/dynamic-workflow-plans" | "/dxr/dynamic-workflow-plans"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_dynamic_workflow_plan_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/dynamic-workflow-controls" | "/dxr/dynamic-workflow-controls"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_dynamic_workflow_control_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/scale-orchestration-proofs" | "/dxr/scale-orchestration-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_scale_orchestration_proof_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/dispatch/claims" | "/dxr/dispatch/claims")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_dispatch_claim_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/dispatch/heartbeats" | "/dxr/dispatch/heartbeats"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_dispatch_heartbeat_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/dispatch/releases" | "/dxr/dispatch/releases")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_dispatch_release_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(path, "/v1/dxr/durable-workflows" | "/dxr/durable-workflows")
    {
        return Ok(Response::json(
            "200 OK",
            state.render_workflow_submit_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/workflow-orchestrations" | "/dxr/workflow-orchestrations"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_workflow_orchestration_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/authority-apply-runs" | "/dxr/authority-apply-runs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_authority_apply_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/ctx-context-inputs" | "/dxr/ctx-context-inputs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_ctx_context_submit_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/provider-memory-integrations" | "/dxr/provider-memory-integrations"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_provider_memory_submit_json(body)?,
        ));
    }
    if method.eq_ignore_ascii_case("POST")
        && matches!(
            path,
            "/v1/dxr/forge-execution-proofs" | "/dxr/forge-execution-proofs"
        )
    {
        return Ok(Response::json(
            "200 OK",
            state.render_forge_execution_proof_json(body)?,
        ));
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(Response::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        ));
    }
    match path {
        "/health" | "/dxr/health" | "/v1/dxr/health" => {
            Ok(Response::json("200 OK", render_health_json()))
        }
        "/dxr/local-runtime-packet.json" | "/v1/dxr/local-runtime-packet.json" => {
            Ok(Response::json("200 OK", render_default_packet_json()?))
        }
        "/dxr/jobs.json" | "/v1/dxr/jobs.json" => Ok(Response::json("200 OK", state.jobs_json())),
        "/dxr/events.json" | "/v1/dxr/events.json" => {
            Ok(Response::json("200 OK", state.events_json()))
        }
        "/dxr/durable-state.json" | "/v1/dxr/durable-state.json" => {
            Ok(Response::json("200 OK", state.durable_state_json()))
        }
        "/dxr/model-turns.json" | "/v1/dxr/model-turns.json" => {
            Ok(Response::json("200 OK", state.model_turns_json()))
        }
        "/dxr/model-provider-observations.json" | "/v1/dxr/model-provider-observations.json" => Ok(
            Response::json("200 OK", state.model_provider_observations_json()),
        ),
        "/dxr/model-routing.json" | "/v1/dxr/model-routing.json" => {
            Ok(Response::json("200 OK", render_model_routing_json()))
        }
        "/dxr/model-provider-adapters.json" | "/v1/dxr/model-provider-adapters.json" => Ok(
            Response::json("200 OK", render_model_provider_adapters_json()),
        ),
        "/dxr/provider-failover.json" | "/v1/dxr/provider-failover.json" => {
            Ok(Response::json("200 OK", state.provider_failover_json()))
        }
        "/dxr/multi-judge.json" | "/v1/dxr/multi-judge.json" => {
            Ok(Response::json("200 OK", state.multi_judge_json()))
        }
        "/dxr/worker-boundary.json" | "/v1/dxr/worker-boundary.json" => Ok(Response::json(
            "200 OK",
            render_worker_boundary_policy_json(),
        )),
        "/dxr/worker-boundaries.json" | "/v1/dxr/worker-boundaries.json" => {
            Ok(Response::json("200 OK", state.worker_boundaries_json()))
        }
        "/dxr/live-worker-execution-preflights.json"
        | "/v1/dxr/live-worker-execution-preflights.json" => Ok(Response::json(
            "200 OK",
            state.live_worker_preflights_json(),
        )),
        "/dxr/worker-execution.json" | "/v1/dxr/worker-execution.json" => {
            Ok(Response::json("200 OK", state.worker_execution_json()))
        }
        "/dxr/worker-turn-loops.json" | "/v1/dxr/worker-turn-loops.json" => {
            Ok(Response::json("200 OK", state.worker_turn_loops_json()))
        }
        "/dxr/worker-driver-runs.json" | "/v1/dxr/worker-driver-runs.json" => {
            Ok(Response::json("200 OK", state.worker_driver_runs_json()))
        }
        "/dxr/tool-executions.json" | "/v1/dxr/tool-executions.json" => {
            Ok(Response::json("200 OK", state.tool_executions_json()))
        }
        "/dxr/capacity.json" | "/v1/dxr/capacity.json" => {
            Ok(Response::json("200 OK", state.capacity_json()))
        }
        "/dxr/sandbox-admissions.json" | "/v1/dxr/sandbox-admissions.json" => {
            Ok(Response::json("200 OK", state.sandbox_admissions_json()))
        }
        "/dxr/sandbox-adapter-turn-ons.json" | "/v1/dxr/sandbox-adapter-turn-ons.json" => Ok(
            Response::json("200 OK", state.sandbox_adapter_turn_ons_json()),
        ),
        "/dxr/external-sandbox-preflights.json" | "/v1/dxr/external-sandbox-preflights.json" => Ok(
            Response::json("200 OK", state.external_sandbox_preflights_json()),
        ),
        "/dxr/sandbox-authority-envelopes.json" | "/v1/dxr/sandbox-authority-envelopes.json" => Ok(
            Response::json("200 OK", state.sandbox_authority_envelopes_json()),
        ),
        "/dxr/execution-admissions.json" | "/v1/dxr/execution-admissions.json" => {
            Ok(Response::json("200 OK", state.execution_admissions_json()))
        }
        "/dxr/execution-schedules.json" | "/v1/dxr/execution-schedules.json" => {
            Ok(Response::json("200 OK", state.execution_schedules_json()))
        }
        "/dxr/execution-supervision.json" | "/v1/dxr/execution-supervision.json" => {
            Ok(Response::json("200 OK", state.execution_supervision_json()))
        }
        "/dxr/supervised-sandbox-handoffs.json" | "/v1/dxr/supervised-sandbox-handoffs.json" => {
            Ok(Response::json("200 OK", state.sandbox_handoffs_json()))
        }
        "/dxr/supervised-sandbox-runners.json" | "/v1/dxr/supervised-sandbox-runners.json" => {
            Ok(Response::json("200 OK", state.sandbox_runners_json()))
        }
        "/dxr/sandbox-sessions.json" | "/v1/dxr/sandbox-sessions.json" => {
            Ok(Response::json("200 OK", state.sandbox_sessions_json()))
        }
        "/dxr/sandbox-command-preflights.json" | "/v1/dxr/sandbox-command-preflights.json" => Ok(
            Response::json("200 OK", state.sandbox_command_preflights_json()),
        ),
        "/dxr/sandbox-command-results.json" | "/v1/dxr/sandbox-command-results.json" => Ok(
            Response::json("200 OK", state.sandbox_command_results_json()),
        ),
        "/dxr/sandbox-result-consumptions.json" | "/v1/dxr/sandbox-result-consumptions.json" => Ok(
            Response::json("200 OK", state.sandbox_result_consumptions_json()),
        ),
        "/dxr/build-performance.json" | "/v1/dxr/build-performance.json" => {
            Ok(Response::json("200 OK", state.build_performance_json()))
        }
        "/dxr/dynamic-workflows.json" | "/v1/dxr/dynamic-workflows.json" => {
            Ok(Response::json("200 OK", state.dynamic_workflows_json()))
        }
        "/dxr/dynamic-workflow-controls.json" | "/v1/dxr/dynamic-workflow-controls.json" => Ok(
            Response::json("200 OK", state.dynamic_workflow_controls_json()),
        ),
        "/dxr/dynamic-workflow-control-recovery.json"
        | "/v1/dxr/dynamic-workflow-control-recovery.json" => Ok(Response::json(
            "200 OK",
            state.dynamic_workflow_control_recovery_json(),
        )),
        "/dxr/forge-execution-proof-recovery.json"
        | "/v1/dxr/forge-execution-proof-recovery.json" => Ok(Response::json(
            "200 OK",
            state.forge_execution_proof_recovery_json(),
        )),
        "/dxr/forge-execution-supervisor.json" | "/v1/dxr/forge-execution-supervisor.json" => Ok(
            Response::json("200 OK", state.forge_execution_supervisor_json()),
        ),
        "/dxr/scale-orchestration.json" | "/v1/dxr/scale-orchestration.json" => {
            Ok(Response::json("200 OK", state.scale_orchestration_json()))
        }
        "/dxr/dispatch/readiness.json" | "/v1/dxr/dispatch/readiness.json" => {
            Ok(Response::json("200 OK", state.dispatch_readiness_json()))
        }
        "/dxr/dispatch/claims.json" | "/v1/dxr/dispatch/claims.json" => {
            Ok(Response::json("200 OK", state.dispatch_claims_json()))
        }
        "/dxr/dispatch/recovery-plan.json" | "/v1/dxr/dispatch/recovery-plan.json" => Ok(
            Response::json("200 OK", state.dispatch_recovery_plan_json()),
        ),
        "/dxr/dispatch-worker-sandbox-handoff.json"
        | "/v1/dxr/dispatch-worker-sandbox-handoff.json" => Ok(Response::json(
            "200 OK",
            state.dispatch_worker_sandbox_handoff_json(),
        )),
        "/dxr/live-execution-sandbox-readiness.json"
        | "/v1/dxr/live-execution-sandbox-readiness.json" => Ok(Response::json(
            "200 OK",
            state.live_execution_sandbox_readiness_json(),
        )),
        "/dxr/ctx-dxr-forge-interface.json" | "/v1/dxr/ctx-dxr-forge-interface.json" => Ok(
            Response::json("200 OK", state.ctx_dxr_forge_interface_json()),
        ),
        "/dxr/forge-local-execution-rehearsal.json"
        | "/v1/dxr/forge-local-execution-rehearsal.json" => Ok(Response::json(
            "200 OK",
            state.forge_local_execution_rehearsal_json(),
        )),
        "/dxr/durable-workflows.json" | "/v1/dxr/durable-workflows.json" => {
            Ok(Response::json("200 OK", state.workflow_runs_json()))
        }
        "/dxr/workflow-orchestrations.json" | "/v1/dxr/workflow-orchestrations.json" => Ok(
            Response::json("200 OK", state.workflow_orchestrations_json()),
        ),
        "/dxr/authority-apply.json" | "/v1/dxr/authority-apply.json" => {
            Ok(Response::json("200 OK", state.authority_apply_json()))
        }
        "/dxr/ctx-context-inputs.json" | "/v1/dxr/ctx-context-inputs.json" => {
            Ok(Response::json("200 OK", state.ctx_context_inputs_json()))
        }
        "/dxr/provider-memory-integrations.json" | "/v1/dxr/provider-memory-integrations.json" => {
            Ok(Response::json(
                "200 OK",
                state.provider_memory_integrations_json(),
            ))
        }
        "/dxr/forge-execution-proofs.json" | "/v1/dxr/forge-execution-proofs.json" => Ok(
            Response::json("200 OK", state.forge_execution_proofs_json()),
        ),
        _ => Ok(Response::text("404 Not Found", "not found\n".to_string())),
    }
}

fn render_health_json() -> String {
    let durable_status = DxrDurableStore::from_env().status();
    format!(
        r#"{{"status":"ok","runtime":"mdx-dxr-engine","runtime_status":"LIVE-LOCAL-DXR-RUNTIME","event_log_status":"LIVE-LOCAL-DXR-EVENT-LOG","durable_state_status":{},"model_streaming_status":"LIVE-LOCAL-DXR-MODEL-STREAM","model_provider_adapter_registry_status":"LIVE-LOCAL-DXR-MODEL-PROVIDER-ADAPTER-REGISTRY","provider_failover_status":"LIVE-LOCAL-DXR-PROVIDER-FAILOVER-FLOOR","multi_judge_status":"LIVE-LOCAL-DXR-MULTI-JUDGE-FLOOR","model_provider_observation_status":"DXR_MODEL_PROVIDER_OBSERVATION_PROVIDER_BLOCKED","worker_lifecycle_status":"LIVE-LOCAL-DXR-WORKER-LIFECYCLE","live_worker_execution_preflight_status":"DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_PROVIDER_BLOCKED","worker_execution_proof_status":"LIVE-LOCAL-DXR-WORKER-EXECUTION-FLOOR","worker_turn_loop_status":"LIVE-LOCAL-DXR-WORKER-TURN-LOOP-FLOOR","worker_driver_status":"LIVE-LOCAL-DXR-WORKER-DRIVER-FLOOR","tool_execution_floor_status":"LIVE-LOCAL-DXR-TOOL-EXECUTION-FLOOR","capacity_sandbox_status":"LIVE-LOCAL-DXR-CAPACITY-SANDBOX-FLOOR","sandbox_admission_status":"LIVE-LOCAL-DXR-SANDBOX-ADMISSION-FLOOR","sandbox_adapter_turn_on_status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","external_sandbox_preflight_status":"LIVE-LOCAL-DXR-EXTERNAL-SANDBOX-PREFLIGHT","sandbox_authority_envelope_status":"LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR","execution_admission_status":"LIVE-LOCAL-DXR-EXECUTION-ADMISSION-FLOOR","execution_scheduler_status":"LIVE-LOCAL-DXR-EXECUTION-SCHEDULER-FLOOR","execution_supervision_status":"LIVE-LOCAL-DXR-EXECUTION-SUPERVISION-FLOOR","supervised_sandbox_handoff_status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-HANDOFF-FLOOR","dispatch_worker_sandbox_handoff_status":"POSTGRES-DURABLE-WHEN-CONFIGURED","live_execution_sandbox_readiness_status":"POSTGRES-DURABLE-WHEN-CONFIGURED","ctx_dxr_forge_interface_status":"POSTGRES-DURABLE-WHEN-CONFIGURED","forge_local_execution_rehearsal_status":"POSTGRES-DURABLE-WHEN-CONFIGURED","supervised_sandbox_runner_status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-RUNNER-FLOOR","sandbox_session_lease_status":"LIVE-LOCAL-DXR-SANDBOX-SESSION-LEASE-FLOOR","sandbox_command_preflight_status":"LIVE-LOCAL-DXR-SANDBOX-COMMAND-PREFLIGHT-FLOOR","sandbox_command_result_status":"LIVE-LOCAL-DXR-SANDBOX-COMMAND-RESULT-FLOOR","sandbox_result_consumption_status":"LIVE-LOCAL-DXR-SANDBOX-RESULT-CONSUMPTION-FLOOR","workflow_orchestration_status":"LIVE-LOCAL-DXR-WORKFLOW-ORCHESTRATION-FLOOR","authority_apply_status":"LIVE-LOCAL-DXR-AUTHORITY-APPLY-FLOOR","forge_execution_proof_status":"LIVE-LOCAL-DXR-FORGE-EXECUTION-PROOF-FLOOR","forge_execution_recovery_status":"POSTGRES-DURABLE-WHEN-CONFIGURED","forge_execution_supervisor_status":"POSTGRES-DURABLE-WHEN-CONFIGURED","build_performance_status":"LIVE-LOCAL-DXR-BUILD-PERFORMANCE-FLOOR","dynamic_workflow_runtime_status":"LIVE-LOCAL-DXR-DYNAMIC-WORKFLOW-PATTERN-FLOOR","scale_orchestration_status":"LIVE-LOCAL-DXR-SCALE-ORCHESTRATION-FLOOR","dispatch_runtime_status":"LIVE-LOCAL-DXR-DISPATCH-RUNTIME","dispatch_readiness_status":"LIVE-LOCAL-DXR-READINESS-SCAN","claim_lease_status":"LIVE-LOCAL-DXR-CLAIM-LEASE","durable_workflow_status":"LIVE-LOCAL-DXR-DURABLE-WORKFLOW","ctx_operational_context_status":"LIVE-LOCAL-DXR-CTX-OPERATIONAL-CONTEXT","provider_memory_integration_status":"LIVE-LOCAL-DXR-PROVIDER-MEMORY-INTEGRATION-FLOOR","temporal_status":"PENDING-LIVE-RUN","sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(durable_status)
    )
}

struct OwnedDxrRequest {
    tenant_id: String,
    actor_id: String,
    intent: String,
    workspace: String,
    branch: String,
    idempotency_key: String,
    max_turns: usize,
    max_cost_cents: usize,
    requested_tools: Vec<String>,
    quality_gates: Vec<String>,
}

struct OwnedModelTurnRequest {
    tenant_id: String,
    actor_id: String,
    role: String,
    trust_boundary: String,
    prompt: String,
    primary_model: String,
    fallback_model: String,
    simulate_primary_failure: bool,
}

struct DxrRuntimeState {
    packets: Vec<DxrLocalRuntimePacket>,
    events: Vec<DxrRuntimeEvent>,
    model_turns: Vec<DxrModelTurn>,
    model_provider_observations: Vec<DxrModelProviderObservation>,
    provider_failover_proofs: Vec<DxrProviderFailoverProof>,
    multi_judge_proofs: Vec<DxrMultiJudgeProof>,
    worker_boundaries: Vec<DxrWorkerBoundary>,
    live_worker_preflights: Vec<DxrLiveWorkerExecutionPreflight>,
    worker_execution_proofs: Vec<DxrWorkerExecutionProof>,
    worker_turn_loop_runtime: DxrWorkerTurnLoopRuntime,
    worker_driver_runtime: DxrWorkerDriverRuntime,
    tool_execution_runtime: DxrToolExecutionRuntime,
    capacity_runtime: DxrCapacityRuntime,
    execution_scheduler_runtime: DxrExecutionSchedulerRuntime,
    execution_supervision_runtime: DxrExecutionSupervisionRuntime,
    sandbox_handoff_runtime: DxrSandboxHandoffRuntime,
    sandbox_runner_runtime: DxrSandboxRunnerRuntime,
    sandbox_session_runtime: DxrSandboxSessionRuntime,
    sandbox_command_runtime: DxrSandboxCommandRuntime,
    sandbox_command_result_runtime: DxrSandboxCommandResultRuntime,
    sandbox_result_consumption_runtime: DxrSandboxResultConsumptionRuntime,
    build_performance_runtime: DxrBuildPerformanceRuntime,
    dynamic_workflow_runtime: DxrDynamicWorkflowRuntime,
    scale_orchestration_runtime: DxrScaleOrchestrationRuntime,
    dispatch_runtime: DxrDispatchRuntime,
    workflow_runtime: DxrWorkflowRuntime,
    workflow_orchestration_runtime: DxrWorkflowOrchestrationRuntime,
    authority_apply_runtime: DxrAuthorityApplyRuntime,
    forge_execution_runtime: DxrForgeExecutionRuntime,
    ctx_context_runtime: DxrCtxContextRuntime,
    provider_memory_runtime: DxrProviderMemoryRuntime,
    event_relay: DxrEventRelay,
    durable_store: DxrDurableStore,
    next_event: usize,
    next_model_turn: usize,
    next_model_provider_observation: usize,
    next_provider_failover_proof: usize,
    next_multi_judge_proof: usize,
    next_worker_boundary: usize,
    next_live_worker_preflight: usize,
    next_worker_execution_proof: usize,
}

struct DxrRuntimeEvent {
    sequence: usize,
    event_id: String,
    event_type: String,
    tenant_id: String,
    job_id: String,
    run_id: String,
    actor_id: String,
    topic: String,
    relay_status: String,
}

struct DxrModelTurn {
    sequence: usize,
    turn_id: String,
    tenant_id: String,
    actor_id: String,
    role: String,
    trust_boundary: String,
    prompt_hash: String,
    primary_model: String,
    primary_provider: String,
    fallback_model: String,
    fallback_provider: String,
    selected_model: String,
    selected_provider: String,
    routing_status: String,
    streaming_status: String,
    terminal_state: String,
    fallback_used: bool,
    retry_count: usize,
    chunk_count: usize,
    chunks: Vec<DxrModelStreamChunk>,
}

struct DxrModelStreamChunk {
    index: usize,
    event: String,
    delta: String,
    terminal: bool,
}

#[derive(Clone)]
struct DxrModelProviderObservation {
    sequence: usize,
    observation_id: String,
    tenant_id: String,
    actor_id: String,
    provider: String,
    adapter: String,
    model_id: String,
    required_receipt_kind: String,
    observed_receipt_id: String,
    approval_receipt_id: String,
    evidence_file: String,
    status: String,
    observed: bool,
    stream_chunk_count: usize,
    terminal_event_observed: bool,
    fallback_supported: bool,
    total_tokens: usize,
}

#[derive(Clone)]
struct DxrProviderFailoverProof {
    sequence: usize,
    proof_id: String,
    tenant_id: String,
    actor_id: String,
    primary_provider: String,
    fallback_provider: String,
    fault_mode: String,
    status: String,
    failover_attempted: bool,
    fallback_selected: bool,
    first_byte_latency_ms: usize,
    slo_ms: usize,
    slo_met: bool,
    attempted_providers: Vec<String>,
}

#[derive(Clone)]
struct DxrMultiJudgeProof {
    sequence: usize,
    proof_id: String,
    tenant_id: String,
    actor_id: String,
    subject_receipt_id: String,
    model_a: String,
    model_a_provider: String,
    model_a_verdict: String,
    model_a_confidence: f64,
    model_b: String,
    model_b_provider: String,
    model_b_verdict: String,
    model_b_confidence: f64,
    confidence_threshold: f64,
    combined_verdict: String,
    combined_confidence: f64,
    status: String,
    disagreement: bool,
    needs_review: bool,
    max_retries: usize,
    parallel_execution_observed: bool,
    fresh_reviewer_context_required: bool,
    builder_can_self_accept: bool,
    adversarial_review_required: bool,
    reviewer_separation_observed: bool,
}

#[derive(Clone)]
struct DxrWorkerExecutionProof {
    sequence: usize,
    proof_id: String,
    tenant_id: String,
    actor_id: String,
    worker_run_id: String,
    claim_id: String,
    spawn_receipt_id: String,
    credential_check_receipt_id: String,
    handoff_receipt_id: String,
    retirement_receipt_id: String,
    preflight_id: String,
    max_runtime_ms: usize,
    observed_runtime_ms: usize,
    max_tool_calls: usize,
    observed_tool_call_count: usize,
    ordered_receipts_observed: bool,
    claim_lease_observed: bool,
    heartbeat_observed: bool,
    bounded_runtime_observed: bool,
    policy_denied_tool_observed: bool,
    handoff_before_retirement: bool,
    status: String,
}

struct DxrEventRelay {
    container: Option<String>,
}

impl Default for DxrRuntimeState {
    fn default() -> Self {
        Self {
            packets: Vec::new(),
            events: Vec::new(),
            model_turns: Vec::new(),
            model_provider_observations: Vec::new(),
            provider_failover_proofs: Vec::new(),
            multi_judge_proofs: Vec::new(),
            worker_boundaries: Vec::new(),
            live_worker_preflights: Vec::new(),
            worker_execution_proofs: Vec::new(),
            worker_turn_loop_runtime: DxrWorkerTurnLoopRuntime::new(),
            worker_driver_runtime: DxrWorkerDriverRuntime::new(),
            tool_execution_runtime: DxrToolExecutionRuntime::new(),
            capacity_runtime: DxrCapacityRuntime::new(),
            execution_scheduler_runtime: DxrExecutionSchedulerRuntime::new(),
            execution_supervision_runtime: DxrExecutionSupervisionRuntime::new(),
            sandbox_handoff_runtime: DxrSandboxHandoffRuntime::new(),
            sandbox_runner_runtime: DxrSandboxRunnerRuntime::new(),
            sandbox_session_runtime: DxrSandboxSessionRuntime::new(),
            sandbox_command_runtime: DxrSandboxCommandRuntime::new(),
            sandbox_command_result_runtime: DxrSandboxCommandResultRuntime::new(),
            sandbox_result_consumption_runtime: DxrSandboxResultConsumptionRuntime::new(),
            build_performance_runtime: DxrBuildPerformanceRuntime::new(),
            dynamic_workflow_runtime: DxrDynamicWorkflowRuntime::new(),
            scale_orchestration_runtime: DxrScaleOrchestrationRuntime::new(),
            dispatch_runtime: DxrDispatchRuntime::new(),
            workflow_runtime: DxrWorkflowRuntime::new(),
            workflow_orchestration_runtime: DxrWorkflowOrchestrationRuntime::new(),
            authority_apply_runtime: DxrAuthorityApplyRuntime::new(),
            forge_execution_runtime: DxrForgeExecutionRuntime::new(),
            ctx_context_runtime: DxrCtxContextRuntime::new(),
            provider_memory_runtime: DxrProviderMemoryRuntime::new(),
            event_relay: DxrEventRelay::from_env(),
            durable_store: DxrDurableStore::from_env(),
            next_event: 0,
            next_model_turn: 0,
            next_model_provider_observation: 0,
            next_provider_failover_proof: 0,
            next_multi_judge_proof: 0,
            next_worker_boundary: 0,
            next_live_worker_preflight: 0,
            next_worker_execution_proof: 0,
        }
    }
}

impl DxrEventRelay {
    fn from_env() -> Self {
        Self {
            container: std::env::var("MDX_VALKEY_CONTAINER")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    fn publish(&self, event: &DxrRuntimeEvent) -> String {
        match run_valkey_command(
            self.container.as_deref(),
            &["PUBLISH", &event.topic, &render_event_envelope_json(event)],
        ) {
            Ok(_) => "LIVE-LOCAL-DXR-WEBSOCKET-STREAM-PUBLISHED".to_string(),
            Err(error) => format!("DXR_WEBSOCKET_STREAM_ERROR:{error}"),
        }
    }
}

impl DxrRuntimeState {
    fn render_request_packet_json(&mut self, body: &str) -> Result<String, String> {
        let request = parse_request(body)?;
        let requested_tools = request
            .requested_tools
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let quality_gates = request
            .quality_gates
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let packet = assemble_local_dxr_runtime_packet(DxrLocalRuntimeRequest {
            tenant_id: &request.tenant_id,
            actor_id: &request.actor_id,
            intent: &request.intent,
            workspace: &request.workspace,
            branch: &request.branch,
            idempotency_key: &request.idempotency_key,
            max_turns: request.max_turns,
            max_cost_cents: request.max_cost_cents,
            requested_tools: &requested_tools,
            quality_gates: &quality_gates,
        })
        .map_err(|error| error.message())?;
        self.append_packet_events(&packet, &request.tenant_id);
        let rendered = render_local_dxr_runtime_packet_json(&packet);
        self.durable_store
            .record_job(&request.tenant_id, &request.actor_id, &packet, &rendered);
        self.packets.push(packet);
        Ok(rendered)
    }

    fn render_model_turn_json(&mut self, body: &str) -> Result<String, String> {
        let request = parse_model_turn_request(body)?;
        self.next_model_turn += 1;
        let turn_id = format!("dxr_model_turn_{:06}", self.next_model_turn);
        let fallback_used = request.simulate_primary_failure;
        let selected_model = if fallback_used {
            request.fallback_model.clone()
        } else {
            request.primary_model.clone()
        };
        let chunks = deterministic_stream_chunks(&request, &selected_model, fallback_used);
        let turn = DxrModelTurn {
            sequence: self.next_model_turn,
            turn_id,
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            role: request.role,
            trust_boundary: request.trust_boundary,
            prompt_hash: stable_hash(&[
                &request.prompt,
                &request.primary_model,
                &request.fallback_model,
            ]),
            primary_provider: provider_for_model(&request.primary_model).to_string(),
            fallback_provider: provider_for_model(&request.fallback_model).to_string(),
            selected_provider: provider_for_model(&selected_model).to_string(),
            primary_model: request.primary_model,
            fallback_model: request.fallback_model,
            selected_model,
            routing_status: if fallback_used {
                "LIVE-LOCAL-DXR-MODEL-FALLBACK-SELECTED".to_string()
            } else {
                "LIVE-LOCAL-DXR-MODEL-PRIMARY-SELECTED".to_string()
            },
            streaming_status: "LIVE-LOCAL-DXR-MODEL-STREAM".to_string(),
            terminal_state: "MODEL_TURN_STREAMED_PROVIDER_CALL_BLOCKED".to_string(),
            fallback_used,
            retry_count: usize::from(fallback_used),
            chunk_count: chunks.len(),
            chunks,
        };
        for event_type in model_turn_event_types(&turn) {
            self.push_model_event(&turn, event_type);
        }
        let rendered = render_model_turn_json(&turn);
        self.durable_store.record_model_turn(
            &turn.tenant_id,
            &turn.actor_id,
            &turn.turn_id,
            &turn.selected_provider,
            &turn.selected_model,
            &turn.routing_status,
            &turn.streaming_status,
            &turn.terminal_state,
            turn.fallback_used,
            turn.chunk_count,
            &rendered,
        );
        self.model_turns.push(turn);
        Ok(rendered)
    }

    fn render_model_provider_observation_json(&mut self, body: &str) -> Result<String, String> {
        self.next_model_provider_observation += 1;
        let observation =
            parse_model_provider_observation(body, self.next_model_provider_observation)?;
        for event_type in model_provider_observation_event_types(&observation) {
            self.push_model_provider_observation_event(&observation, event_type);
        }
        let rendered = render_model_provider_observation_json(&observation);
        self.model_provider_observations.push(observation);
        Ok(rendered)
    }

    fn render_provider_failover_proof_json(&mut self, body: &str) -> Result<String, String> {
        self.next_provider_failover_proof += 1;
        let proof = parse_provider_failover_proof(body, self.next_provider_failover_proof)?;
        for event_type in provider_failover_event_types(&proof) {
            self.push_provider_failover_event(&proof, event_type);
        }
        let rendered = render_provider_failover_proof_json(&proof);
        self.provider_failover_proofs.push(proof);
        Ok(rendered)
    }

    fn render_multi_judge_proof_json(&mut self, body: &str) -> Result<String, String> {
        self.next_multi_judge_proof += 1;
        let proof = parse_multi_judge_proof(body, self.next_multi_judge_proof)?;
        for event_type in multi_judge_event_types(&proof) {
            self.push_multi_judge_event(&proof, event_type);
        }
        let rendered = render_multi_judge_proof_json(&proof);
        self.multi_judge_proofs.push(proof);
        Ok(rendered)
    }

    fn render_worker_boundary_json(&mut self, body: &str) -> Result<String, String> {
        self.next_worker_boundary += 1;
        let boundary = parse_worker_boundary(body, self.next_worker_boundary)?;
        for event_type in worker_boundary_event_types() {
            self.push_worker_event(&boundary, event_type);
        }
        let rendered = render_worker_boundary_json(&boundary);
        self.durable_store.record_worker_boundary(
            &boundary.tenant_id,
            &boundary.actor_id,
            &boundary.boundary_id,
            &boundary.worker_run_id,
            &rendered,
        );
        self.worker_boundaries.push(boundary);
        Ok(rendered)
    }

    fn render_live_worker_preflight_json(&mut self, body: &str) -> Result<String, String> {
        self.next_live_worker_preflight += 1;
        let preflight =
            parse_live_worker_execution_preflight(body, self.next_live_worker_preflight)?;
        for event_type in live_worker_preflight_event_types(&preflight) {
            self.push_live_worker_preflight_event(&preflight, event_type);
        }
        let rendered = render_live_worker_preflight_json(&preflight);
        self.live_worker_preflights.push(preflight);
        Ok(rendered)
    }

    fn render_worker_execution_proof_json(&mut self, body: &str) -> Result<String, String> {
        self.next_worker_execution_proof += 1;
        let proof = parse_worker_execution_proof(body, self.next_worker_execution_proof)?;
        for event_type in worker_execution_event_types(&proof) {
            self.push_worker_execution_event(&proof, event_type);
        }
        let rendered = render_worker_execution_proof_json(&proof);
        self.worker_execution_proofs.push(proof);
        Ok(rendered)
    }

    fn render_worker_turn_loop_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.worker_turn_loop_runtime.submit_json(body)?;
        Ok(self.render_worker_turn_loop_result(result))
    }

    fn worker_turn_loops_json(&self) -> String {
        self.worker_turn_loop_runtime.turn_loops_json()
    }

    fn render_worker_turn_loop_result(&mut self, result: DxrWorkerTurnLoopResult) -> String {
        for event in result.events {
            self.push_worker_turn_loop_event(&event);
        }
        result.body
    }

    fn render_worker_driver_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.worker_driver_runtime.submit_json(body)?;
        Ok(self.render_worker_driver_result(result))
    }

    fn worker_driver_runs_json(&self) -> String {
        self.worker_driver_runtime.runs_json()
    }

    fn render_worker_driver_result(&mut self, result: DxrWorkerDriverResult) -> String {
        for event in result.events {
            self.push_worker_driver_event(&event);
        }
        result.body
    }

    fn render_tool_execution_proof_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.tool_execution_runtime.submit_json(body)?;
        Ok(self.render_tool_execution_result(result))
    }

    fn render_tool_execution_result(&mut self, result: DxrToolExecutionResult) -> String {
        for event in result.events {
            self.push_tool_execution_event(&event);
        }
        result.body
    }

    fn render_capacity_plan_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.capacity_runtime.submit_json(body)?;
        Ok(self.render_capacity_result(result))
    }

    fn capacity_json(&self) -> String {
        self.capacity_runtime.plans_json()
    }

    fn render_sandbox_admission_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.capacity_runtime.submit_sandbox_admission_json(body)?;
        Ok(self.render_capacity_result(result))
    }

    fn sandbox_admissions_json(&self) -> String {
        self.capacity_runtime.sandbox_admissions_json()
    }

    fn render_sandbox_adapter_turn_on_json(&mut self, body: &str) -> Result<String, String> {
        let result = self
            .capacity_runtime
            .submit_sandbox_adapter_turn_on_json(body)?;
        Ok(self.render_capacity_result(result))
    }

    fn sandbox_adapter_turn_ons_json(&self) -> String {
        self.capacity_runtime.sandbox_adapter_turn_ons_json()
    }

    fn render_external_sandbox_preflight_json(&mut self, body: &str) -> Result<String, String> {
        let result = self
            .capacity_runtime
            .submit_external_sandbox_preflight_json(body)?;
        Ok(self.render_capacity_result(result))
    }

    fn external_sandbox_preflights_json(&self) -> String {
        self.capacity_runtime.external_sandbox_preflights_json()
    }

    fn render_sandbox_authority_envelope_json(&mut self, body: &str) -> Result<String, String> {
        let result = self
            .capacity_runtime
            .submit_sandbox_authority_envelope_json(body)?;
        Ok(self.render_capacity_result(result))
    }

    fn sandbox_authority_envelopes_json(&self) -> String {
        self.capacity_runtime.sandbox_authority_envelopes_json()
    }

    fn render_execution_admission_json(&mut self, body: &str) -> Result<String, String> {
        let result = self
            .capacity_runtime
            .submit_execution_admission_json(body)?;
        Ok(self.render_capacity_result(result))
    }

    fn execution_admissions_json(&self) -> String {
        self.capacity_runtime.execution_admissions_json()
    }

    fn render_execution_schedule_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.execution_scheduler_runtime.submit_json(body)?;
        Ok(self.render_execution_scheduler_result(result))
    }

    fn execution_schedules_json(&self) -> String {
        self.execution_scheduler_runtime.schedules_json()
    }

    fn render_execution_scheduler_result(&mut self, result: DxrExecutionSchedulerResult) -> String {
        for event in result.events {
            self.push_execution_scheduler_event(&event);
        }
        result.body
    }

    fn render_execution_supervision_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.execution_supervision_runtime.submit_json(body)?;
        Ok(self.render_execution_supervision_result(result))
    }

    fn execution_supervision_json(&self) -> String {
        self.execution_supervision_runtime.supervision_json()
    }

    fn render_execution_supervision_result(
        &mut self,
        result: DxrExecutionSupervisionResult,
    ) -> String {
        for event in result.events {
            self.push_execution_supervision_event(&event);
        }
        result.body
    }

    fn render_sandbox_handoff_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.sandbox_handoff_runtime.submit_json(body)?;
        Ok(self.render_sandbox_handoff_result(result))
    }

    fn sandbox_handoffs_json(&self) -> String {
        self.sandbox_handoff_runtime.handoffs_json()
    }

    fn render_sandbox_handoff_result(&mut self, result: DxrSandboxHandoffResult) -> String {
        for event in result.events {
            self.push_sandbox_handoff_event(&event);
        }
        result.body
    }

    fn render_sandbox_runner_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.sandbox_runner_runtime.submit_json(body)?;
        Ok(self.render_sandbox_runner_result(result))
    }

    fn sandbox_runners_json(&self) -> String {
        self.sandbox_runner_runtime.runners_json()
    }

    fn render_sandbox_runner_result(&mut self, result: DxrSandboxRunnerResult) -> String {
        for event in result.events {
            self.push_sandbox_runner_event(&event);
        }
        result.body
    }

    fn render_sandbox_session_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.sandbox_session_runtime.submit_json(body)?;
        Ok(self.render_sandbox_session_result(result))
    }

    fn sandbox_sessions_json(&self) -> String {
        self.sandbox_session_runtime.sessions_json()
    }

    fn render_sandbox_session_result(&mut self, result: DxrSandboxSessionResult) -> String {
        for event in result.events {
            self.push_sandbox_session_event(&event);
        }
        result.body
    }

    fn render_sandbox_command_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.sandbox_command_runtime.submit_json(body)?;
        Ok(self.render_sandbox_command_result(result))
    }

    fn sandbox_command_preflights_json(&self) -> String {
        self.sandbox_command_runtime.preflights_json()
    }

    fn render_sandbox_command_result(&mut self, result: DxrSandboxCommandResult) -> String {
        for event in result.events {
            self.push_sandbox_command_event(&event);
        }
        result.body
    }

    fn render_sandbox_command_result_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.sandbox_command_result_runtime.submit_json(body)?;
        Ok(self.render_sandbox_command_result_outcome(result))
    }

    fn sandbox_command_results_json(&self) -> String {
        self.sandbox_command_result_runtime.results_json()
    }

    fn render_sandbox_command_result_outcome(
        &mut self,
        result: DxrSandboxCommandResultOutcome,
    ) -> String {
        for event in result.events {
            self.push_sandbox_command_result_event(&event);
        }
        result.body
    }

    fn render_sandbox_result_consumption_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.sandbox_result_consumption_runtime.submit_json(body)?;
        Ok(self.render_sandbox_result_consumption_outcome(result))
    }

    fn sandbox_result_consumptions_json(&self) -> String {
        self.sandbox_result_consumption_runtime.consumptions_json()
    }

    fn render_sandbox_result_consumption_outcome(
        &mut self,
        result: DxrSandboxResultConsumptionOutcome,
    ) -> String {
        for event in result.events {
            self.push_sandbox_result_consumption_event(&event);
        }
        result.body
    }

    fn render_capacity_result(&mut self, result: DxrCapacityResult) -> String {
        for event in result.events {
            self.push_capacity_event(&event);
        }
        result.body
    }

    fn render_build_performance_proof_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.build_performance_runtime.submit_json(body)?;
        Ok(self.render_build_performance_result(result))
    }

    fn build_performance_json(&self) -> String {
        self.build_performance_runtime.proofs_json()
    }

    fn render_build_performance_result(&mut self, result: DxrBuildPerformanceResult) -> String {
        for event in result.events {
            self.push_build_performance_event(&event);
        }
        result.body
    }

    fn render_dynamic_workflow_plan_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.dynamic_workflow_runtime.submit_json(body)?;
        Ok(self.render_dynamic_workflow_result(result))
    }

    fn render_dynamic_workflow_control_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.dynamic_workflow_runtime.control_json(body)?;
        Ok(self.render_dynamic_workflow_result(result))
    }

    fn dynamic_workflows_json(&self) -> String {
        self.dynamic_workflow_runtime.plans_json()
    }

    fn dynamic_workflow_controls_json(&self) -> String {
        self.dynamic_workflow_runtime.controls_json()
    }

    fn dynamic_workflow_control_recovery_json(&self) -> String {
        self.durable_store
            .render_dynamic_workflow_control_recovery_json("tenant_local")
    }

    fn forge_execution_proof_recovery_json(&self) -> String {
        self.durable_store
            .render_forge_execution_recovery_json("tenant_local")
    }

    fn forge_execution_supervisor_json(&self) -> String {
        self.durable_store
            .render_forge_execution_supervisor_json("tenant_local")
    }

    fn render_dynamic_workflow_result(&mut self, result: DxrDynamicWorkflowResult) -> String {
        self.durable_store
            .record_workflow_run(&result.durable_run, &result.body);
        for event in result.events {
            self.push_dynamic_workflow_event(&event);
        }
        result.body
    }

    fn render_scale_orchestration_proof_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.scale_orchestration_runtime.submit_json(body)?;
        Ok(self.render_scale_orchestration_result(result))
    }

    fn scale_orchestration_json(&self) -> String {
        self.scale_orchestration_runtime.proofs_json()
    }

    fn render_scale_orchestration_result(&mut self, result: DxrScaleOrchestrationResult) -> String {
        for event in result.events {
            self.push_scale_orchestration_event(&event);
        }
        result.body
    }

    fn dispatch_readiness_json(&mut self) -> String {
        let result = self.dispatch_runtime.readiness_json();
        self.render_dispatch_result(result)
    }

    fn dispatch_recovery_plan_json(&mut self) -> String {
        let result = self.dispatch_runtime.recovery_plan_json();
        self.render_dispatch_result(result)
    }

    fn render_dispatch_claim_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.dispatch_runtime.claim_json(body)?;
        Ok(self.render_dispatch_result(result))
    }

    fn render_dispatch_heartbeat_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.dispatch_runtime.heartbeat_json(body)?;
        Ok(self.render_dispatch_result(result))
    }

    fn render_dispatch_release_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.dispatch_runtime.release_json(body)?;
        Ok(self.render_dispatch_result(result))
    }

    fn render_dispatch_result(&mut self, result: DxrDispatchResult) -> String {
        for event in result.events {
            self.push_dispatch_event(&event);
        }
        result.body
    }

    fn render_workflow_submit_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.workflow_runtime.submit_json(body)?;
        Ok(self.render_workflow_result(result))
    }

    fn render_workflow_result(&mut self, result: DxrWorkflowResult) -> String {
        self.durable_store
            .record_workflow_run(&result.run, &result.body);
        for event in result.events {
            self.push_workflow_event(&event);
        }
        result.body
    }

    fn render_workflow_orchestration_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.workflow_orchestration_runtime.submit_json(body)?;
        Ok(self.render_workflow_orchestration_result(result))
    }

    fn workflow_orchestrations_json(&self) -> String {
        self.workflow_orchestration_runtime.orchestrations_json()
    }

    fn render_workflow_orchestration_result(
        &mut self,
        result: DxrWorkflowOrchestrationResult,
    ) -> String {
        for event in result.events {
            self.push_workflow_orchestration_event(&event);
        }
        result.body
    }

    fn render_authority_apply_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.authority_apply_runtime.submit_json(body)?;
        Ok(self.render_authority_apply_result(result))
    }

    fn authority_apply_json(&self) -> String {
        self.authority_apply_runtime.authority_apply_json()
    }

    fn render_authority_apply_result(&mut self, result: DxrAuthorityApplyResult) -> String {
        for event in result.events {
            self.push_authority_apply_event(&event);
        }
        result.body
    }

    fn render_forge_execution_proof_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.forge_execution_runtime.submit_json(body)?;
        Ok(self.render_forge_execution_result(result))
    }

    fn forge_execution_proofs_json(&self) -> String {
        self.forge_execution_runtime.proofs_json()
    }

    fn render_forge_execution_result(&mut self, result: DxrForgeExecutionResult) -> String {
        self.durable_store
            .record_forge_execution_proof(&result.durable_record, &result.body);
        for event in result.events {
            self.push_forge_execution_event(&event);
        }
        result.body
    }

    fn render_ctx_context_submit_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.ctx_context_runtime.submit_json(body)?;
        Ok(self.render_ctx_context_result(result))
    }

    fn render_ctx_context_result(&mut self, result: DxrCtxContextResult) -> String {
        self.durable_store
            .record_ctx_context_input(&result.input, &result.body);
        for event in result.events {
            self.push_ctx_context_event(&event);
        }
        result.body
    }

    fn render_provider_memory_submit_json(&mut self, body: &str) -> Result<String, String> {
        let result = self.provider_memory_runtime.submit_json(body)?;
        Ok(self.render_provider_memory_result(result))
    }

    fn provider_memory_integrations_json(&self) -> String {
        self.provider_memory_runtime.integrations_json()
    }

    fn render_provider_memory_result(&mut self, result: DxrProviderMemoryResult) -> String {
        for event in result.events {
            self.push_provider_memory_event(&event);
        }
        result.body
    }

    fn append_packet_events(&mut self, packet: &DxrLocalRuntimePacket, tenant_id: &str) {
        for record in &packet.evidence_records {
            self.push_event(
                record,
                tenant_id,
                &packet.job.job_id,
                &packet.run.run_id,
                &packet.run.agent_id,
            );
        }
    }

    fn push_event(
        &mut self,
        record: &DxrEvidenceRecord,
        tenant_id: &str,
        fallback_job_id: &str,
        fallback_run_id: &str,
        fallback_actor_id: &str,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: record.event_type.clone(),
            tenant_id: tenant_id.to_string(),
            job_id: record
                .job_id
                .clone()
                .unwrap_or_else(|| fallback_job_id.to_string()),
            run_id: record
                .run_id
                .clone()
                .unwrap_or_else(|| fallback_run_id.to_string()),
            actor_id: record
                .agent_id
                .clone()
                .unwrap_or_else(|| fallback_actor_id.to_string()),
            topic: dxr_event_topic(tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_model_event(&mut self, turn: &DxrModelTurn, event_type: &str) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            tenant_id: turn.tenant_id.clone(),
            job_id: turn.turn_id.clone(),
            run_id: turn.turn_id.clone(),
            actor_id: turn.actor_id.clone(),
            topic: dxr_event_topic(&turn.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_model_provider_observation_event(
        &mut self,
        observation: &DxrModelProviderObservation,
        event_type: &str,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            tenant_id: observation.tenant_id.clone(),
            job_id: observation.observation_id.clone(),
            run_id: observation.observation_id.clone(),
            actor_id: observation.actor_id.clone(),
            topic: dxr_event_topic(&observation.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_provider_failover_event(&mut self, proof: &DxrProviderFailoverProof, event_type: &str) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            tenant_id: proof.tenant_id.clone(),
            job_id: proof.proof_id.clone(),
            run_id: proof.proof_id.clone(),
            actor_id: proof.actor_id.clone(),
            topic: dxr_event_topic(&proof.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_multi_judge_event(&mut self, proof: &DxrMultiJudgeProof, event_type: &str) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            tenant_id: proof.tenant_id.clone(),
            job_id: proof.proof_id.clone(),
            run_id: proof.proof_id.clone(),
            actor_id: proof.actor_id.clone(),
            topic: dxr_event_topic(&proof.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_worker_event(&mut self, boundary: &DxrWorkerBoundary, event_type: &str) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            tenant_id: boundary.tenant_id.clone(),
            job_id: boundary.boundary_id.clone(),
            run_id: boundary.worker_run_id.clone(),
            actor_id: boundary.actor_id.clone(),
            topic: dxr_event_topic(&boundary.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_live_worker_preflight_event(
        &mut self,
        preflight: &DxrLiveWorkerExecutionPreflight,
        event_type: &str,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            tenant_id: preflight.tenant_id.clone(),
            job_id: preflight.preflight_id.clone(),
            run_id: preflight.worker_run_id.clone(),
            actor_id: preflight.actor_id.clone(),
            topic: dxr_event_topic(&preflight.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_worker_execution_event(&mut self, proof: &DxrWorkerExecutionProof, event_type: &str) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: event_type.to_string(),
            tenant_id: proof.tenant_id.clone(),
            job_id: proof.proof_id.clone(),
            run_id: proof.worker_run_id.clone(),
            actor_id: proof.actor_id.clone(),
            topic: dxr_event_topic(&proof.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_worker_turn_loop_event(&mut self, turn_loop_event: &DxrWorkerTurnLoopRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: turn_loop_event.event_type.clone(),
            tenant_id: turn_loop_event.tenant_id.clone(),
            job_id: turn_loop_event.job_id.clone(),
            run_id: turn_loop_event.run_id.clone(),
            actor_id: turn_loop_event.actor_id.clone(),
            topic: dxr_event_topic(&turn_loop_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_worker_driver_event(&mut self, driver_event: &DxrWorkerDriverRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: driver_event.event_type.clone(),
            tenant_id: driver_event.tenant_id.clone(),
            job_id: driver_event.job_id.clone(),
            run_id: driver_event.run_id.clone(),
            actor_id: driver_event.actor_id.clone(),
            topic: dxr_event_topic(&driver_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_tool_execution_event(&mut self, tool_event: &DxrToolExecutionRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: tool_event.event_type.clone(),
            tenant_id: tool_event.tenant_id.clone(),
            job_id: tool_event.job_id.clone(),
            run_id: tool_event.run_id.clone(),
            actor_id: tool_event.actor_id.clone(),
            topic: dxr_event_topic(&tool_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_capacity_event(&mut self, capacity_event: &DxrCapacityRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: capacity_event.event_type.clone(),
            tenant_id: capacity_event.tenant_id.clone(),
            job_id: capacity_event.job_id.clone(),
            run_id: capacity_event.run_id.clone(),
            actor_id: capacity_event.actor_id.clone(),
            topic: dxr_event_topic(&capacity_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_build_performance_event(
        &mut self,
        performance_event: &DxrBuildPerformanceRuntimeEvent,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: performance_event.event_type.clone(),
            tenant_id: performance_event.tenant_id.clone(),
            job_id: performance_event.job_id.clone(),
            run_id: performance_event.run_id.clone(),
            actor_id: performance_event.actor_id.clone(),
            topic: dxr_event_topic(&performance_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_dynamic_workflow_event(&mut self, workflow_event: &DxrDynamicWorkflowRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: workflow_event.event_type.clone(),
            tenant_id: workflow_event.tenant_id.clone(),
            job_id: workflow_event.job_id.clone(),
            run_id: workflow_event.run_id.clone(),
            actor_id: workflow_event.actor_id.clone(),
            topic: dxr_event_topic(&workflow_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_scale_orchestration_event(&mut self, scale_event: &DxrScaleOrchestrationRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: scale_event.event_type.clone(),
            tenant_id: scale_event.tenant_id.clone(),
            job_id: scale_event.job_id.clone(),
            run_id: scale_event.run_id.clone(),
            actor_id: scale_event.actor_id.clone(),
            topic: dxr_event_topic(&scale_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_execution_scheduler_event(
        &mut self,
        schedule_event: &DxrExecutionSchedulerRuntimeEvent,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: schedule_event.event_type.clone(),
            tenant_id: schedule_event.tenant_id.clone(),
            job_id: schedule_event.job_id.clone(),
            run_id: schedule_event.run_id.clone(),
            actor_id: schedule_event.actor_id.clone(),
            topic: dxr_event_topic(&schedule_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_execution_supervision_event(
        &mut self,
        supervision_event: &DxrExecutionSupervisionRuntimeEvent,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: supervision_event.event_type.clone(),
            tenant_id: supervision_event.tenant_id.clone(),
            job_id: supervision_event.job_id.clone(),
            run_id: supervision_event.run_id.clone(),
            actor_id: supervision_event.actor_id.clone(),
            topic: dxr_event_topic(&supervision_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_sandbox_handoff_event(&mut self, handoff_event: &DxrSandboxHandoffRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: handoff_event.event_type.clone(),
            tenant_id: handoff_event.tenant_id.clone(),
            job_id: handoff_event.job_id.clone(),
            run_id: handoff_event.run_id.clone(),
            actor_id: handoff_event.actor_id.clone(),
            topic: dxr_event_topic(&handoff_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_sandbox_runner_event(&mut self, runner_event: &DxrSandboxRunnerRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: runner_event.event_type.clone(),
            tenant_id: runner_event.tenant_id.clone(),
            job_id: runner_event.job_id.clone(),
            run_id: runner_event.run_id.clone(),
            actor_id: runner_event.actor_id.clone(),
            topic: dxr_event_topic(&runner_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_sandbox_session_event(&mut self, session_event: &DxrSandboxSessionRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: session_event.event_type.clone(),
            tenant_id: session_event.tenant_id.clone(),
            job_id: session_event.job_id.clone(),
            run_id: session_event.run_id.clone(),
            actor_id: session_event.actor_id.clone(),
            topic: dxr_event_topic(&session_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_sandbox_command_event(&mut self, command_event: &DxrSandboxCommandRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: command_event.event_type.clone(),
            tenant_id: command_event.tenant_id.clone(),
            job_id: command_event.job_id.clone(),
            run_id: command_event.run_id.clone(),
            actor_id: command_event.actor_id.clone(),
            topic: dxr_event_topic(&command_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_sandbox_command_result_event(
        &mut self,
        result_event: &DxrSandboxCommandResultRuntimeEvent,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: result_event.event_type.clone(),
            tenant_id: result_event.tenant_id.clone(),
            job_id: result_event.job_id.clone(),
            run_id: result_event.run_id.clone(),
            actor_id: result_event.actor_id.clone(),
            topic: dxr_event_topic(&result_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_sandbox_result_consumption_event(
        &mut self,
        consumption_event: &DxrSandboxResultConsumptionRuntimeEvent,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: consumption_event.event_type.clone(),
            tenant_id: consumption_event.tenant_id.clone(),
            job_id: consumption_event.job_id.clone(),
            run_id: consumption_event.run_id.clone(),
            actor_id: consumption_event.actor_id.clone(),
            topic: dxr_event_topic(&consumption_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_dispatch_event(&mut self, dispatch_event: &DxrDispatchRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: dispatch_event.event_type.clone(),
            tenant_id: dispatch_event.tenant_id.clone(),
            job_id: dispatch_event.job_id.clone(),
            run_id: dispatch_event.run_id.clone(),
            actor_id: dispatch_event.actor_id.clone(),
            topic: dxr_event_topic(&dispatch_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_workflow_event(&mut self, workflow_event: &DxrWorkflowRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: workflow_event.event_type.clone(),
            tenant_id: workflow_event.tenant_id.clone(),
            job_id: workflow_event.job_id.clone(),
            run_id: workflow_event.run_id.clone(),
            actor_id: workflow_event.actor_id.clone(),
            topic: dxr_event_topic(&workflow_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_workflow_orchestration_event(
        &mut self,
        orchestration_event: &DxrWorkflowOrchestrationRuntimeEvent,
    ) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: orchestration_event.event_type.clone(),
            tenant_id: orchestration_event.tenant_id.clone(),
            job_id: orchestration_event.job_id.clone(),
            run_id: orchestration_event.run_id.clone(),
            actor_id: orchestration_event.actor_id.clone(),
            topic: dxr_event_topic(&orchestration_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_authority_apply_event(&mut self, authority_event: &DxrAuthorityApplyRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: authority_event.event_type.clone(),
            tenant_id: authority_event.tenant_id.clone(),
            job_id: authority_event.job_id.clone(),
            run_id: authority_event.run_id.clone(),
            actor_id: authority_event.actor_id.clone(),
            topic: dxr_event_topic(&authority_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_forge_execution_event(&mut self, forge_event: &DxrForgeExecutionRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: forge_event.event_type.clone(),
            tenant_id: forge_event.tenant_id.clone(),
            job_id: forge_event.job_id.clone(),
            run_id: forge_event.run_id.clone(),
            actor_id: forge_event.actor_id.clone(),
            topic: dxr_event_topic(&forge_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_ctx_context_event(&mut self, ctx_event: &DxrCtxContextRuntimeEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: ctx_event.event_type.clone(),
            tenant_id: ctx_event.tenant_id.clone(),
            job_id: ctx_event.job_id.clone(),
            run_id: ctx_event.run_id.clone(),
            actor_id: ctx_event.actor_id.clone(),
            topic: dxr_event_topic(&ctx_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn push_provider_memory_event(&mut self, provider_memory_event: &DxrProviderMemoryEvent) {
        self.next_event += 1;
        let mut event = DxrRuntimeEvent {
            sequence: self.next_event,
            event_id: format!("dxr_event_{:06}", self.next_event),
            event_type: provider_memory_event.event_type.clone(),
            tenant_id: provider_memory_event.tenant_id.clone(),
            job_id: provider_memory_event.job_id.clone(),
            run_id: provider_memory_event.run_id.clone(),
            actor_id: provider_memory_event.actor_id.clone(),
            topic: dxr_event_topic(&provider_memory_event.tenant_id),
            relay_status: "PENDING-RELAY-PUBLISH".to_string(),
        };
        event.relay_status = self.event_relay.publish(&event);
        self.durable_store.record_event(
            &event.tenant_id,
            &event.actor_id,
            &event.event_id,
            &event.event_type,
            &event.job_id,
            &event.run_id,
            &event.topic,
            &event.relay_status,
            &render_event_json(&event),
        );
        self.events.push(event);
    }

    fn jobs_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-dxr-jobs","status":"LIVE-LOCAL-DXR-RUNTIME","runtime":"mdx-dxr-engine","job_count":{},"jobs":[{}],"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
            self.packets.len(),
            self.packets
                .iter()
                .map(render_local_dxr_runtime_packet_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn model_turns_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-dxr-model-turns","status":"LIVE-LOCAL-DXR-MODEL-STREAM","runtime":"mdx-dxr-engine","route":"/dxr/model-turns.json","submit_route":"/v1/dxr/model-turns","routing_route":"/dxr/model-routing.json","turn_count":{},"turns":[{}],"provider_calls_allowed":false,"live_provider_turn_on_required":true,"production_writes_allowed":false}}"#,
            self.model_turns.len(),
            self.model_turns
                .iter()
                .map(render_model_turn_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn model_provider_observations_json(&self) -> String {
        let observed_count = self
            .model_provider_observations
            .iter()
            .filter(|observation| observation.observed)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-model-provider-observations","status":{},"runtime":"mdx-dxr-engine","route":"/dxr/model-provider-observations.json","submit_route":"/v1/dxr/model-provider-observations","observation_count":{},"observed_provider_count":{},"provider_connected_streaming_ready":{},"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_writes_allowed":false,"observations":[{}]}}"#,
            json_string_literal(if observed_count > 0 {
                "DXR_MODEL_PROVIDER_OBSERVATIONS_RECORDED"
            } else {
                "DXR_MODEL_PROVIDER_OBSERVATIONS_EMPTY_OR_REJECTED"
            }),
            self.model_provider_observations.len(),
            observed_count,
            observed_count > 0,
            self.model_provider_observations
                .iter()
                .map(render_model_provider_observation_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn provider_failover_json(&self) -> String {
        let passing_count = self
            .provider_failover_proofs
            .iter()
            .filter(|proof| proof.slo_met && proof.fallback_selected)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-provider-failover","status":"LIVE-LOCAL-DXR-PROVIDER-FAILOVER-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/provider-failover.json","submit_route":"/v1/dxr/provider-failover-proofs","proof_count":{},"passing_proof_count":{},"provider_count":{},"fault_mode_count":5,"chaos_scenario_count":{},"failover_slo_ms":3000,"failover_signals":["provider.failover.attempted","provider.failover.succeeded","provider.failover.failed","provider.failover.slo_breach"],"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"secret_values_recorded":false,"output_text_recorded":false,"production_writes_allowed":false,"proofs":[{}]}}"#,
            self.provider_failover_proofs.len(),
            passing_count,
            MODEL_PROVIDER_ADAPTERS.len(),
            MODEL_PROVIDER_ADAPTERS.len() * 5,
            self.provider_failover_proofs
                .iter()
                .map(render_provider_failover_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn multi_judge_json(&self) -> String {
        let agreed_count = self
            .multi_judge_proofs
            .iter()
            .filter(|proof| !proof.disagreement && !proof.needs_review)
            .count();
        let needs_review_count = self
            .multi_judge_proofs
            .iter()
            .filter(|proof| proof.needs_review)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-multi-judge","status":"LIVE-LOCAL-DXR-MULTI-JUDGE-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/multi-judge.json","submit_route":"/v1/dxr/multi-judge-proofs","proof_count":{},"agreed_count":{},"needs_review_count":{},"default_model_a":"claude-sonnet-4-20250514","default_model_b":"grok-3","confidence_threshold":0.7,"max_retries":2,"parallel_execution_required":true,"fresh_reviewer_context_required":true,"builder_can_self_accept":false,"adversarial_review_required":true,"reviewer_separation_required":true,"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"secret_values_recorded":false,"judge_response_text_recorded":false,"reasoning_text_recorded":false,"production_writes_allowed":false,"proofs":[{}]}}"#,
            self.multi_judge_proofs.len(),
            agreed_count,
            needs_review_count,
            self.multi_judge_proofs
                .iter()
                .map(render_multi_judge_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn worker_boundaries_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-dxr-worker-boundaries","status":"LIVE-LOCAL-DXR-WORKER-LIFECYCLE","runtime":"mdx-dxr-engine","route":"/dxr/worker-boundaries.json","submit_route":"/v1/dxr/worker-boundaries","boundary_count":{},"boundaries":[{}],"live_worker_execution_allowed":false,"provider_turn_on_required":true,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
            self.worker_boundaries.len(),
            self.worker_boundaries
                .iter()
                .map(render_worker_boundary_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn live_worker_preflights_json(&self) -> String {
        let provider_blocked_count = self
            .live_worker_preflights
            .iter()
            .filter(|preflight| {
                preflight.status == "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_PROVIDER_BLOCKED"
            })
            .count();
        let authority_blocked_count = self
            .live_worker_preflights
            .iter()
            .filter(|preflight| {
                preflight.status
                    == "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_BLOCKED"
            })
            .count();
        let authority_staged_count = self
            .live_worker_preflights
            .iter()
            .filter(|preflight| {
                preflight.status
                    == "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_STAGED"
            })
            .count();
        format!(
            r#"{{"name":"mdx-dxr-live-worker-execution-preflights","status":"DXR_LIVE_WORKER_EXECUTION_PREFLIGHTS_RECORDED_AUTHORITY_BLOCKED","runtime":"mdx-dxr-engine","route":"/dxr/live-worker-execution-preflights.json","submit_route":"/v1/dxr/live-worker-execution-preflights","preflight_count":{},"provider_blocked_count":{},"authority_blocked_count":{},"authority_staged_count":{},"required_receipts":["worker.spawn_requested","worker.credential.checked","worker.handoff.recorded","worker.retired","provider.turn_on.observed","dispatch.claim.recorded","dispatch.heartbeat.renewed","dxr.durable_workflow.recorded","sandbox.authority.checked","external_sandbox.preflight.recorded","tool_policy.enforced","reviewer_separation.observed"],"provider_turn_on_required":true,"preflight_only":true,"live_worker_execution_allowed":false,"worker_process_started":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"preflights":[{}]}}"#,
            self.live_worker_preflights.len(),
            provider_blocked_count,
            authority_blocked_count,
            authority_staged_count,
            self.live_worker_preflights
                .iter()
                .map(render_live_worker_preflight_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn worker_execution_json(&self) -> String {
        let proven_count = self
            .worker_execution_proofs
            .iter()
            .filter(|proof| proof.status == "DXR_WORKER_EXECUTION_PROVEN_LOCAL_AUTHORITY_BLOCKED")
            .count();
        format!(
            r#"{{"name":"mdx-dxr-worker-execution","status":"LIVE-LOCAL-DXR-WORKER-EXECUTION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/worker-execution.json","submit_route":"/v1/dxr/worker-execution-proofs","proof_count":{},"proven_count":{},"required_runtime_signals":["ordered_receipts_observed","claim_lease_observed","heartbeat_observed","bounded_runtime_observed","policy_denied_tool_observed","handoff_before_retirement"],"max_runtime_ms_ceiling":300000,"provider_turn_on_required":true,"provider_turn_on_observed":false,"worker_process_started":false,"live_worker_execution_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"proofs":[{}]}}"#,
            self.worker_execution_proofs.len(),
            proven_count,
            self.worker_execution_proofs
                .iter()
                .map(render_worker_execution_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn tool_executions_json(&self) -> String {
        self.tool_execution_runtime.executions_json()
    }

    fn dispatch_claims_json(&self) -> String {
        self.dispatch_runtime.claims_json()
    }

    fn dispatch_worker_sandbox_handoff_json(&self) -> String {
        self.durable_store
            .render_dispatch_worker_sandbox_handoff_json("tenant_local")
    }

    fn live_execution_sandbox_readiness_json(&self) -> String {
        self.durable_store
            .render_live_execution_sandbox_readiness_json("tenant_local")
    }

    fn ctx_dxr_forge_interface_json(&self) -> String {
        self.durable_store
            .render_ctx_dxr_forge_interface_json("tenant_local")
    }

    fn forge_local_execution_rehearsal_json(&self) -> String {
        self.durable_store
            .render_forge_local_execution_rehearsal_json("tenant_local")
    }

    fn workflow_runs_json(&self) -> String {
        self.workflow_runtime.runs_json()
    }

    fn ctx_context_inputs_json(&self) -> String {
        self.ctx_context_runtime.inputs_json()
    }

    fn events_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-dxr-events","status":"LIVE-LOCAL-DXR-EVENT-LOG","runtime":"mdx-dxr-engine","stream":"dxr_event_stream","relay_driver":"mdx-message-relay","relay_topic_pattern":"mdx:dxr:{{tenant_id}}:events","event_count":{},"events":[{}],"websocket_status":{},"durable_state_status":{},"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
            self.events.len(),
            self.events
                .iter()
                .map(render_event_json)
                .collect::<Vec<_>>()
                .join(","),
            json_string_literal(self.event_stream_status()),
            json_string_literal(self.durable_store.status())
        )
    }

    fn durable_state_json(&self) -> String {
        self.durable_store.render_summary_json("tenant_local")
    }

    fn event_stream_status(&self) -> &str {
        if self
            .events
            .iter()
            .any(|event| event.relay_status == "LIVE-LOCAL-DXR-WEBSOCKET-STREAM-PUBLISHED")
        {
            "LIVE-LOCAL-DXR-WEBSOCKET-STREAM"
        } else {
            "PENDING-LIVE-RUN"
        }
    }
}

fn render_event_json(event: &DxrRuntimeEvent) -> String {
    format!(
        r#"{{"sequence":{},"event_id":{},"event_type":{},"tenant_id":{},"job_id":{},"run_id":{},"actor_id":{},"stream":"dxr_event_stream","topic":{},"relay_status":{},"websocket_fanout_allowed":{},"production_delivery_allowed":false}}"#,
        event.sequence,
        json_string_literal(&event.event_id),
        json_string_literal(&event.event_type),
        json_string_literal(&event.tenant_id),
        json_string_literal(&event.job_id),
        json_string_literal(&event.run_id),
        json_string_literal(&event.actor_id),
        json_string_literal(&event.topic),
        json_string_literal(&event.relay_status),
        event.relay_status == "LIVE-LOCAL-DXR-WEBSOCKET-STREAM-PUBLISHED"
    )
}

fn render_event_envelope_json(event: &DxrRuntimeEvent) -> String {
    format!(
        r#"{{"type":"envelope","domain":"dxr","stream_id":"dxr_event_stream","sequence":{},"event_id":{},"event_type":{},"tenant_id":{},"job_id":{},"run_id":{},"actor_id":{},"topic":{},"websocket_fanout_allowed":true,"production_delivery_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false}}"#,
        event.sequence,
        json_string_literal(&event.event_id),
        json_string_literal(&event.event_type),
        json_string_literal(&event.tenant_id),
        json_string_literal(&event.job_id),
        json_string_literal(&event.run_id),
        json_string_literal(&event.actor_id),
        json_string_literal(&event.topic)
    )
}

fn dxr_event_topic(tenant_id: &str) -> String {
    format!("mdx:dxr:{tenant_id}:events")
}

fn render_model_routing_json() -> String {
    format!(
        r#"{{"name":"mdx-dxr-model-routing","status":"LIVE-LOCAL-DXR-MODEL-ROUTER","runtime":"mdx-dxr-engine","route":"/dxr/model-routing.json","model_turn_submit_route":"/v1/dxr/model-turns","model_provider_adapter_registry_route":"/dxr/model-provider-adapters.json","provider_failover_route":"/dxr/provider-failover.json","provider_failover_submit_route":"/v1/dxr/provider-failover-proofs","model_provider_observation_route":"/v1/dxr/model-provider-observations","model_provider_observations_route":"/dxr/model-provider-observations.json","local_streaming_status":"LIVE-LOCAL-DXR-MODEL-STREAM","provider_connected_streaming_status":"PENDING-OBSERVED-PROVIDER-EVIDENCE","provider_adapter_registry_status":"LIVE-LOCAL-DXR-MODEL-PROVIDER-ADAPTER-REGISTRY","provider_failover_status":"LIVE-LOCAL-DXR-PROVIDER-FAILOVER-FLOOR","fallback_status":"LIVE-LOCAL-DXR-MODEL-FALLBACK","stream_parser_floor":"v1 dxr-llm SSE parser shape","normalized_stream_contract_status":"LIVE-LOCAL-DXR-NORMALIZED-STREAM-CONTRACT","retry_policy":"bounded local retry with fallback after primary failure","failover_slo_ms":3000,"failover_signals":["provider.failover.attempted","provider.failover.succeeded","provider.failover.failed","provider.failover.slo_breach"],"providers":[{}],"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_provider_turn_on_required":true,"production_writes_allowed":false}}"#,
        render_model_provider_adapter_summaries_json()
    )
}

struct DxrModelProviderAdapter {
    provider: &'static str,
    adapter: &'static str,
    env_key_name: &'static str,
    base_url_env_name: Option<&'static str>,
    default_base_url: &'static str,
    models: &'static [&'static str],
    required_receipt_kind: &'static str,
    stream_wire_format: &'static str,
    supports_tools: bool,
    supports_vision: bool,
    supports_audio: bool,
    supports_embeddings: bool,
}

const NORMALIZED_STREAM_EVENTS: &[&str] = &[
    "message_start",
    "content_delta",
    "tool_call_delta",
    "usage_delta",
    "message_stop",
];

const MODEL_PROVIDER_ADAPTERS: &[DxrModelProviderAdapter] = &[
    DxrModelProviderAdapter {
        provider: "anthropic",
        adapter: "AnthropicMessagesModelGateway",
        env_key_name: "ANTHROPIC_API_KEY",
        base_url_env_name: None,
        default_base_url: "https://api.anthropic.com",
        models: &["claude-sonnet-4-6", "claude-haiku-4-5"],
        required_receipt_kind: "anthropic.message.observed",
        stream_wire_format: "anthropic_messages_sse",
        supports_tools: true,
        supports_vision: true,
        supports_audio: false,
        supports_embeddings: false,
    },
    DxrModelProviderAdapter {
        provider: "openai",
        adapter: "OpenAIResponsesModelGateway",
        env_key_name: "OPENAI_API_KEY",
        base_url_env_name: Some("OPENAI_BASE_URL"),
        default_base_url: "https://api.openai.com/v1",
        models: &["gpt-4.1", "gpt-4.1-mini"],
        required_receipt_kind: "openai.response.observed",
        stream_wire_format: "openai_responses_sse",
        supports_tools: true,
        supports_vision: true,
        supports_audio: true,
        supports_embeddings: true,
    },
    DxrModelProviderAdapter {
        provider: "xai",
        adapter: "XaiChatCompletionsModelGateway",
        env_key_name: "XAI_API_KEY",
        base_url_env_name: Some("XAI_BASE_URL"),
        default_base_url: "https://api.x.ai/v1",
        models: &["grok-3", "grok-4-1-fast-reasoning"],
        required_receipt_kind: "xai.response.observed",
        stream_wire_format: "openai_compatible_chat_sse",
        supports_tools: true,
        supports_vision: true,
        supports_audio: false,
        supports_embeddings: false,
    },
    DxrModelProviderAdapter {
        provider: "gemini",
        adapter: "GeminiGenerateContentModelGateway",
        env_key_name: "GEMINI_API_KEY",
        base_url_env_name: None,
        default_base_url: "https://generativelanguage.googleapis.com",
        models: &["gemini-2.5-pro", "gemini-2.5-flash"],
        required_receipt_kind: "gemini.generate_content.observed",
        stream_wire_format: "gemini_stream_generate_content",
        supports_tools: true,
        supports_vision: true,
        supports_audio: true,
        supports_embeddings: true,
    },
    DxrModelProviderAdapter {
        provider: "mistral",
        adapter: "MistralChatModelGateway",
        env_key_name: "MISTRAL_API_KEY",
        base_url_env_name: Some("MISTRAL_BASE_URL"),
        default_base_url: "https://api.mistral.ai/v1",
        models: &["mistral-large-latest", "codestral-latest"],
        required_receipt_kind: "mistral.chat.observed",
        stream_wire_format: "mistral_chat_sse",
        supports_tools: true,
        supports_vision: false,
        supports_audio: false,
        supports_embeddings: true,
    },
    DxrModelProviderAdapter {
        provider: "tensorzero",
        adapter: "TensorZeroModelGateway",
        env_key_name: "TENSORZERO_GATEWAY_URL",
        base_url_env_name: Some("TENSORZERO_GATEWAY_URL"),
        default_base_url: "http://127.0.0.1:3000",
        models: &["tensorzero-function-router"],
        required_receipt_kind: "tensorzero.inference.observed",
        stream_wire_format: "tensorzero_inference_stream",
        supports_tools: true,
        supports_vision: true,
        supports_audio: false,
        supports_embeddings: false,
    },
];

fn render_model_provider_adapters_json() -> String {
    format!(
        r#"{{"name":"mdx-dxr-model-provider-adapters","status":"LIVE-LOCAL-DXR-MODEL-PROVIDER-ADAPTER-REGISTRY","runtime":"mdx-dxr-engine","route":"/dxr/model-provider-adapters.json","routing_route":"/dxr/model-routing.json","failover_route":"/dxr/provider-failover.json","failover_submit_route":"/v1/dxr/provider-failover-proofs","observation_route":"/v1/dxr/model-provider-observations","provider_count":{},"normalized_stream_events":{},"failover_slo_ms":3000,"failover_signals":["provider.failover.attempted","provider.failover.succeeded","provider.failover.failed","provider.failover.slo_breach"],"credential_source":"local environment only","credential_presence_only":true,"secret_values_recorded":false,"output_text_recorded":false,"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"production_writes_allowed":false,"adapters":[{}]}}"#,
        MODEL_PROVIDER_ADAPTERS.len(),
        render_string_array(NORMALIZED_STREAM_EVENTS),
        MODEL_PROVIDER_ADAPTERS
            .iter()
            .map(render_model_provider_adapter_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn render_model_provider_adapter_summaries_json() -> String {
    MODEL_PROVIDER_ADAPTERS
        .iter()
        .map(|adapter| {
            format!(
                r#"{{"provider":{},"adapter":{},"models":{},"live_status":"PENDING-LIVE-RUN","required_receipt_kind":{},"normalized_stream_events":{},"credential_presence_only":true,"secret_values_recorded":false,"provider_calls_allowed":false}}"#,
                json_string_literal(adapter.provider),
                json_string_literal(adapter.adapter),
                render_string_array(adapter.models),
                json_string_literal(adapter.required_receipt_kind),
                render_string_array(NORMALIZED_STREAM_EVENTS)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn render_model_provider_adapter_json(adapter: &DxrModelProviderAdapter) -> String {
    format!(
        r#"{{"provider":{},"adapter":{},"env_key_name":{},"base_url_env_name":{},"default_base_url":{},"models":{},"required_receipt_kind":{},"stream_wire_format":{},"normalized_stream_events":{},"supports_tools":{},"supports_vision":{},"supports_audio":{},"supports_embeddings":{},"credential_presence_only":true,"secret_values_recorded":false,"output_text_recorded":false,"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"production_writes_allowed":false}}"#,
        json_string_literal(adapter.provider),
        json_string_literal(adapter.adapter),
        json_string_literal(adapter.env_key_name),
        adapter
            .base_url_env_name
            .map(json_string_literal)
            .unwrap_or_else(|| "null".to_string()),
        json_string_literal(adapter.default_base_url),
        render_string_array(adapter.models),
        json_string_literal(adapter.required_receipt_kind),
        json_string_literal(adapter.stream_wire_format),
        render_string_array(NORMALIZED_STREAM_EVENTS),
        adapter.supports_tools,
        adapter.supports_vision,
        adapter.supports_audio,
        adapter.supports_embeddings
    )
}

fn render_string_array(values: &[&str]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string_literal(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn parse_request(body: &str) -> Result<OwnedDxrRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR request json: {error}"))?;
    let default_request = default_local_dxr_runtime_request();
    let tenant_id = string_value(&value, "tenant_id", default_request.tenant_id);
    let actor_id = value
        .get("actor_id")
        .or_else(|| value.get("agent_id"))
        .and_then(Value::as_str)
        .unwrap_or(default_request.actor_id)
        .to_string();
    let intent = string_value(&value, "intent", default_request.intent);
    let workspace = string_value(&value, "workspace", default_request.workspace);
    let branch = string_value(&value, "branch", default_request.branch);
    let idempotency_key = string_value(&value, "idempotency_key", default_request.idempotency_key);
    let max_turns = value
        .get("max_turns")
        .or_else(|| value.get("turn_limit"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default_request.max_turns);
    let max_cost_cents = value
        .get("max_cost_cents")
        .or_else(|| value.get("cost_ceiling_cents"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default_request.max_cost_cents);
    let requested_tools = string_array(&value, "requested_tools", default_request.requested_tools);
    let quality_gates = string_array(&value, "quality_gates", default_request.quality_gates);
    Ok(OwnedDxrRequest {
        tenant_id,
        actor_id,
        intent,
        workspace,
        branch,
        idempotency_key,
        max_turns,
        max_cost_cents,
        requested_tools,
        quality_gates,
    })
}

fn parse_model_turn_request(body: &str) -> Result<OwnedModelTurnRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR model turn json: {error}"))?;
    Ok(OwnedModelTurnRequest {
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "local_forge_operator"),
        role: string_value(&value, "role", "builder"),
        trust_boundary: string_value(&value, "trust_boundary", "supervised"),
        prompt: string_value(&value, "prompt", "stream a governed DXR model turn"),
        primary_model: string_value(&value, "primary_model", "claude-sonnet-4-6"),
        fallback_model: string_value(&value, "fallback_model", "grok-3"),
        simulate_primary_failure: value
            .get("simulate_primary_failure")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_model_provider_observation(
    body: &str,
    sequence: usize,
) -> Result<DxrModelProviderObservation, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR model provider observation json: {error}"))?;
    let provider = string_value(&value, "provider", "openai");
    let required_receipt_kind = model_provider_receipt_kind(&provider).to_string();
    let observed = bool_value(&value, "observed", false);
    let receipt_kind = string_value(&value, "receipt_kind", "");
    let approval_receipt_id = string_value(&value, "approval_receipt_id", "");
    let observed_receipt_id = string_value(
        &value,
        "observed_receipt_id",
        &format!("dxr_model_provider_observed_receipt_{sequence:06}"),
    );
    let stream_chunk_count = usize_value(&value, "stream_chunk_count", 0);
    let terminal_event_observed = bool_value(&value, "terminal_event_observed", false);
    let fallback_supported = bool_value(&value, "fallback_supported", false);
    let credential_values_recorded = bool_value(&value, "credential_values_recorded", false);
    let provider_secret_values_recorded =
        bool_value(&value, "provider_secret_values_recorded", false);
    let requested_secret_values_recorded =
        bool_value(&value, "requested_secret_values_recorded", false);
    let output_text_recorded = bool_value(&value, "output_text_recorded", false);
    let production_writes_allowed = bool_value(&value, "production_writes_allowed", false);
    let accepted = observed
        && receipt_kind == required_receipt_kind
        && !approval_receipt_id.trim().is_empty()
        && stream_chunk_count > 0
        && terminal_event_observed
        && !credential_values_recorded
        && !provider_secret_values_recorded
        && !requested_secret_values_recorded
        && !output_text_recorded
        && !production_writes_allowed;
    Ok(DxrModelProviderObservation {
        sequence,
        observation_id: format!("dxr_model_provider_observation_{sequence:06}"),
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        provider,
        adapter: string_value(&value, "adapter", "OpenAIResponsesModelGateway"),
        model_id: string_value(&value, "model_id", ""),
        required_receipt_kind,
        observed_receipt_id,
        approval_receipt_id,
        evidence_file: string_value(&value, "evidence_file", ""),
        status: if accepted {
            "DXR_MODEL_PROVIDER_OBSERVED_PROVIDER_CONNECTED_LOCAL"
        } else {
            "DXR_MODEL_PROVIDER_OBSERVATION_REJECTED_PROVIDER_BLOCKED"
        }
        .to_string(),
        observed: accepted,
        stream_chunk_count,
        terminal_event_observed,
        fallback_supported,
        total_tokens: usize_value(&value, "total_tokens", 0),
    })
}

fn parse_provider_failover_proof(
    body: &str,
    sequence: usize,
) -> Result<DxrProviderFailoverProof, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR provider failover proof json: {error}"))?;
    let primary_provider = string_value(&value, "primary_provider", "anthropic");
    let fallback_provider = string_value(&value, "fallback_provider", "xai");
    let fault_mode = string_value(&value, "fault_mode", "primary_timeout");
    let failover_attempted = bool_value(&value, "failover_attempted", false);
    let fallback_selected = bool_value(&value, "fallback_selected", false);
    let first_byte_latency_ms = usize_value(&value, "first_byte_latency_ms", 0);
    let slo_ms = usize_value(&value, "slo_ms", 3000);
    let provider_calls_allowed = bool_value(&value, "provider_calls_allowed", false);
    let runtime_provider_calls_allowed_now =
        bool_value(&value, "runtime_provider_calls_allowed_now", false);
    let live_network_calls_started_by_runtime =
        bool_value(&value, "live_network_calls_started_by_runtime", false);
    let secret_values_recorded = bool_value(&value, "secret_values_recorded", false);
    let output_text_recorded = bool_value(&value, "output_text_recorded", false);
    let production_writes_allowed = bool_value(&value, "production_writes_allowed", false);
    let known_primary = model_provider_known(&primary_provider);
    let known_fallback = model_provider_known(&fallback_provider);
    let default_attempted_providers = [primary_provider.as_str(), fallback_provider.as_str()];
    let attempted_providers =
        string_array(&value, "attempted_providers", &default_attempted_providers);
    let accepted = known_primary
        && known_fallback
        && primary_provider != fallback_provider
        && !fault_mode.trim().is_empty()
        && failover_attempted
        && fallback_selected
        && first_byte_latency_ms > 0
        && slo_ms == 3000
        && first_byte_latency_ms <= slo_ms
        && !provider_calls_allowed
        && !runtime_provider_calls_allowed_now
        && !live_network_calls_started_by_runtime
        && !secret_values_recorded
        && !output_text_recorded
        && !production_writes_allowed;
    Ok(DxrProviderFailoverProof {
        sequence,
        proof_id: format!("dxr_provider_failover_proof_{sequence:06}"),
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        primary_provider,
        fallback_provider,
        fault_mode,
        status: if accepted {
            "DXR_PROVIDER_FAILOVER_SLO_PROVEN_LOCAL"
        } else {
            "DXR_PROVIDER_FAILOVER_PROOF_REJECTED"
        }
        .to_string(),
        failover_attempted: accepted,
        fallback_selected: accepted,
        first_byte_latency_ms,
        slo_ms,
        slo_met: accepted,
        attempted_providers,
    })
}

fn parse_multi_judge_proof(body: &str, sequence: usize) -> Result<DxrMultiJudgeProof, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR multi judge proof json: {error}"))?;
    let model_a = string_value(&value, "model_a", "claude-sonnet-4-20250514");
    let model_b = string_value(&value, "model_b", "grok-3");
    let model_a_provider = provider_for_model(&model_a).to_string();
    let model_b_provider = provider_for_model(&model_b).to_string();
    let model_a_verdict = string_value(&value, "model_a_verdict", "pass");
    let model_b_verdict = string_value(&value, "model_b_verdict", "pass");
    let model_a_confidence = f64_value(&value, "model_a_confidence", 0.0);
    let model_b_confidence = f64_value(&value, "model_b_confidence", 0.0);
    let confidence_threshold = f64_value(&value, "confidence_threshold", 0.7);
    let max_retries = usize_value(&value, "max_retries", 2);
    let parallel_execution_observed = bool_value(&value, "parallel_execution_observed", false);
    let fresh_reviewer_context_required =
        bool_value(&value, "fresh_reviewer_context_required", true);
    let builder_can_self_accept = bool_value(&value, "builder_can_self_accept", false);
    let adversarial_review_required = bool_value(&value, "adversarial_review_required", true);
    let reviewer_separation_observed = bool_value(&value, "reviewer_separation_observed", true);
    let provider_calls_allowed = bool_value(&value, "provider_calls_allowed", false);
    let runtime_provider_calls_allowed_now =
        bool_value(&value, "runtime_provider_calls_allowed_now", false);
    let live_network_calls_started_by_runtime =
        bool_value(&value, "live_network_calls_started_by_runtime", false);
    let secret_values_recorded = bool_value(&value, "secret_values_recorded", false);
    let judge_response_text_recorded = bool_value(&value, "judge_response_text_recorded", false);
    let reasoning_text_recorded = bool_value(&value, "reasoning_text_recorded", false);
    let production_writes_allowed = bool_value(&value, "production_writes_allowed", false);
    let valid = model_provider_known(&model_a_provider)
        && model_provider_known(&model_b_provider)
        && model_a_provider != model_b_provider
        && judge_verdict_known(&model_a_verdict)
        && judge_verdict_known(&model_b_verdict)
        && confidence_threshold >= 0.7
        && max_retries == 2
        && parallel_execution_observed
        && fresh_reviewer_context_required
        && !builder_can_self_accept
        && adversarial_review_required
        && reviewer_separation_observed
        && !provider_calls_allowed
        && !runtime_provider_calls_allowed_now
        && !live_network_calls_started_by_runtime
        && !secret_values_recorded
        && !judge_response_text_recorded
        && !reasoning_text_recorded
        && !production_writes_allowed;
    let judges_agree = model_a_verdict == model_b_verdict;
    let combined_confidence = model_a_confidence.min(model_b_confidence);
    let consensus_passes = valid
        && judges_agree
        && model_a_verdict != "needs_review"
        && combined_confidence >= confidence_threshold;
    let needs_review = valid
        && (!judges_agree
            || model_a_verdict == "needs_review"
            || combined_confidence < confidence_threshold);
    let combined_verdict = if consensus_passes {
        model_a_verdict.clone()
    } else {
        "needs_review".to_string()
    };
    Ok(DxrMultiJudgeProof {
        sequence,
        proof_id: format!("dxr_multi_judge_proof_{sequence:06}"),
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        subject_receipt_id: string_value(
            &value,
            "subject_receipt_id",
            "dxr_harness_subject_receipt_000001",
        ),
        model_a,
        model_a_provider,
        model_a_verdict,
        model_a_confidence,
        model_b,
        model_b_provider,
        model_b_verdict,
        model_b_confidence,
        confidence_threshold,
        combined_verdict,
        combined_confidence: if consensus_passes {
            combined_confidence
        } else {
            0.0
        },
        status: if consensus_passes {
            "DXR_MULTI_JUDGE_VERDICT_AGREED_LOCAL"
        } else if needs_review {
            "DXR_MULTI_JUDGE_ESCALATED_NEEDS_REVIEW_LOCAL"
        } else {
            "DXR_MULTI_JUDGE_PROOF_REJECTED"
        }
        .to_string(),
        disagreement: valid && !judges_agree,
        needs_review: needs_review || !valid,
        max_retries,
        parallel_execution_observed,
        fresh_reviewer_context_required,
        builder_can_self_accept,
        adversarial_review_required,
        reviewer_separation_observed,
    })
}

fn parse_worker_execution_proof(
    body: &str,
    sequence: usize,
) -> Result<DxrWorkerExecutionProof, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR worker execution proof json: {error}"))?;
    let max_runtime_ms = usize_value(&value, "max_runtime_ms", 120000);
    let observed_runtime_ms = usize_value(&value, "observed_runtime_ms", 0);
    let max_tool_calls = usize_value(&value, "max_tool_calls", 0);
    let observed_tool_call_count = usize_value(&value, "observed_tool_call_count", 0);
    let ordered_receipts_observed = bool_value(&value, "ordered_receipts_observed", false);
    let claim_lease_observed = bool_value(&value, "claim_lease_observed", false);
    let heartbeat_observed = bool_value(&value, "heartbeat_observed", false);
    let bounded_runtime_observed = bool_value(&value, "bounded_runtime_observed", false);
    let policy_denied_tool_observed = bool_value(&value, "policy_denied_tool_observed", false);
    let handoff_before_retirement = bool_value(&value, "handoff_before_retirement", false);
    let provider_turn_on_observed = bool_value(&value, "provider_turn_on_observed", false);
    let worker_process_started = bool_value(&value, "worker_process_started", false);
    let live_worker_execution_allowed = bool_value(&value, "live_worker_execution_allowed", false);
    let tool_execution_allowed = bool_value(&value, "tool_execution_allowed", false);
    let shell_execution_allowed = bool_value(&value, "shell_execution_allowed", false);
    let patch_application_allowed = bool_value(&value, "patch_application_allowed", false);
    let ci_claim_allowed = bool_value(&value, "ci_claim_allowed", false);
    let deployment_allowed = bool_value(&value, "deployment_allowed", false);
    let production_writes_allowed = bool_value(&value, "production_writes_allowed", false);
    let spawn_receipt_id = string_value(
        &value,
        "spawn_receipt_id",
        &format!("worker_spawn_request_receipt_{sequence:06}"),
    );
    let credential_check_receipt_id = string_value(
        &value,
        "credential_check_receipt_id",
        &format!("worker_credential_check_receipt_{sequence:06}"),
    );
    let handoff_receipt_id = string_value(
        &value,
        "handoff_receipt_id",
        &format!("worker_handoff_receipt_{sequence:06}"),
    );
    let retirement_receipt_id = string_value(
        &value,
        "retirement_receipt_id",
        &format!("worker_retirement_receipt_{sequence:06}"),
    );
    let preflight_id = string_value(
        &value,
        "preflight_id",
        &format!("dxr_live_worker_preflight_{sequence:06}"),
    );
    let accepted = !spawn_receipt_id.trim().is_empty()
        && !credential_check_receipt_id.trim().is_empty()
        && !handoff_receipt_id.trim().is_empty()
        && !retirement_receipt_id.trim().is_empty()
        && !preflight_id.trim().is_empty()
        && max_runtime_ms > 0
        && max_runtime_ms <= 300000
        && observed_runtime_ms > 0
        && observed_runtime_ms <= max_runtime_ms
        && observed_tool_call_count <= max_tool_calls
        && ordered_receipts_observed
        && claim_lease_observed
        && heartbeat_observed
        && bounded_runtime_observed
        && policy_denied_tool_observed
        && handoff_before_retirement
        && !provider_turn_on_observed
        && !worker_process_started
        && !live_worker_execution_allowed
        && !tool_execution_allowed
        && !shell_execution_allowed
        && !patch_application_allowed
        && !ci_claim_allowed
        && !deployment_allowed
        && !production_writes_allowed;
    Ok(DxrWorkerExecutionProof {
        sequence,
        proof_id: format!("dxr_worker_execution_proof_{sequence:06}"),
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        worker_run_id: string_value(
            &value,
            "worker_run_id",
            &format!("dxr_worker_run_{sequence:06}"),
        ),
        claim_id: string_value(
            &value,
            "claim_id",
            &format!("dxr_dispatch_claim_{sequence:06}"),
        ),
        spawn_receipt_id,
        credential_check_receipt_id,
        handoff_receipt_id,
        retirement_receipt_id,
        preflight_id,
        max_runtime_ms,
        observed_runtime_ms,
        max_tool_calls,
        observed_tool_call_count,
        ordered_receipts_observed,
        claim_lease_observed,
        heartbeat_observed,
        bounded_runtime_observed,
        policy_denied_tool_observed,
        handoff_before_retirement,
        status: if accepted {
            "DXR_WORKER_EXECUTION_PROVEN_LOCAL_AUTHORITY_BLOCKED"
        } else {
            "DXR_WORKER_EXECUTION_PROOF_REJECTED"
        }
        .to_string(),
    })
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn bool_value(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_value(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}

fn f64_value(value: &Value, key: &str, default: f64) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn provider_for_model(model: &str) -> &'static str {
    if model.starts_with("claude") {
        "anthropic"
    } else if model.starts_with("gpt") {
        "openai"
    } else if model.starts_with("grok") {
        "xai"
    } else if model.starts_with("gemini") {
        "gemini"
    } else if model.starts_with("mistral") || model.starts_with("codestral") {
        "mistral"
    } else {
        "local"
    }
}

fn model_provider_receipt_kind(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "anthropic.message.observed",
        "openai" => "openai.response.observed",
        "xai" => "xai.response.observed",
        "gemini" => "gemini.generate_content.observed",
        "mistral" => "mistral.chat.observed",
        "tensorzero" => "tensorzero.inference.observed",
        _ => "model.provider.observed",
    }
}

fn model_provider_known(provider: &str) -> bool {
    MODEL_PROVIDER_ADAPTERS
        .iter()
        .any(|adapter| adapter.provider == provider)
}

fn judge_verdict_known(verdict: &str) -> bool {
    matches!(verdict, "pass" | "fail" | "needs_review")
}

fn deterministic_stream_chunks(
    request: &OwnedModelTurnRequest,
    selected_model: &str,
    fallback_used: bool,
) -> Vec<DxrModelStreamChunk> {
    let route = if fallback_used { "fallback" } else { "primary" };
    vec![
        DxrModelStreamChunk {
            index: 1,
            event: "message_start".to_string(),
            delta: format!("DXR selected {selected_model} through the {route} route."),
            terminal: false,
        },
        DxrModelStreamChunk {
            index: 2,
            event: "content_delta".to_string(),
            delta: format!(
                " Role {} stays inside trust boundary {}.",
                request.role, request.trust_boundary
            ),
            terminal: false,
        },
        DxrModelStreamChunk {
            index: 3,
            event: "content_delta".to_string(),
            delta: " Provider calls remain blocked until turn-on evidence is observed.".to_string(),
            terminal: false,
        },
        DxrModelStreamChunk {
            index: 4,
            event: "message_stop".to_string(),
            delta: " Local stream complete.".to_string(),
            terminal: true,
        },
    ]
}

fn model_turn_event_types(turn: &DxrModelTurn) -> Vec<&'static str> {
    let mut event_types = vec!["model_stream_started"];
    if turn.fallback_used {
        event_types.push("model_fallback_selected");
    }
    event_types.extend(["model_stream_chunk", "model_stream_completed"]);
    event_types
}

fn model_provider_observation_event_types(
    observation: &DxrModelProviderObservation,
) -> [&'static str; 2] {
    if observation.observed {
        [
            "model_provider_observation_recorded",
            "model_provider_connected_stream_observed",
        ]
    } else {
        [
            "model_provider_observation_recorded",
            "model_provider_observation_provider_blocked",
        ]
    }
}

fn provider_failover_event_types(proof: &DxrProviderFailoverProof) -> Vec<&'static str> {
    let mut event_types = vec!["provider.failover.attempted"];
    if proof.slo_met {
        event_types.push("provider.failover.succeeded");
    } else {
        event_types.push("provider.failover.failed");
        event_types.push("provider.failover.slo_breach");
    }
    event_types
}

fn multi_judge_event_types(proof: &DxrMultiJudgeProof) -> Vec<&'static str> {
    let mut event_types = vec!["multi_judge_proof_recorded"];
    if proof.fresh_reviewer_context_required
        && !proof.builder_can_self_accept
        && proof.adversarial_review_required
        && proof.reviewer_separation_observed
    {
        event_types.push("llm_judge.reviewer_separation_required");
    }
    match proof.status.as_str() {
        "DXR_MULTI_JUDGE_VERDICT_AGREED_LOCAL" => {
            event_types.push("llm_judge.agreed");
            event_types.push("llm_judge.verdict_recorded");
        }
        "DXR_MULTI_JUDGE_ESCALATED_NEEDS_REVIEW_LOCAL" => {
            if proof.disagreement {
                event_types.push("llm_judge.disagreed");
            }
            event_types.push("llm_judge.needs_review");
        }
        _ => event_types.push("llm_judge.rejected"),
    }
    event_types
}

fn worker_execution_event_types(proof: &DxrWorkerExecutionProof) -> Vec<&'static str> {
    let mut event_types = vec!["worker_execution_proof_recorded"];
    if proof.status == "DXR_WORKER_EXECUTION_PROVEN_LOCAL_AUTHORITY_BLOCKED" {
        event_types.extend([
            "worker_execution_runtime_bounded",
            "worker_execution_policy_denied_tool_observed",
            "worker_execution_authority_blocked",
        ]);
    } else {
        event_types.push("worker_execution_proof_rejected");
    }
    event_types
}

fn render_provider_failover_proof_json(proof: &DxrProviderFailoverProof) -> String {
    let attempted = proof
        .attempted_providers
        .iter()
        .map(|provider| json_string_literal(provider))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"sequence":{},"proof_id":{},"tenant_id":{},"actor_id":{},"primary_provider":{},"fallback_provider":{},"fault_mode":{},"status":{},"failover_attempted":{},"fallback_selected":{},"first_byte_latency_ms":{},"slo_ms":{},"slo_met":{},"attempted_providers":[{}],"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"secret_values_recorded":false,"output_text_recorded":false,"production_writes_allowed":false}}"#,
        proof.sequence,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.primary_provider),
        json_string_literal(&proof.fallback_provider),
        json_string_literal(&proof.fault_mode),
        json_string_literal(&proof.status),
        proof.failover_attempted,
        proof.fallback_selected,
        proof.first_byte_latency_ms,
        proof.slo_ms,
        proof.slo_met,
        attempted
    )
}

fn render_multi_judge_proof_json(proof: &DxrMultiJudgeProof) -> String {
    format!(
        r#"{{"sequence":{},"proof_id":{},"tenant_id":{},"actor_id":{},"subject_receipt_id":{},"model_a":{},"model_a_provider":{},"model_a_verdict":{},"model_a_confidence":{},"model_b":{},"model_b_provider":{},"model_b_verdict":{},"model_b_confidence":{},"confidence_threshold":{},"combined_verdict":{},"combined_confidence":{},"status":{},"disagreement":{},"needs_review":{},"max_retries":{},"parallel_execution_observed":{},"fresh_reviewer_context_required":{},"builder_can_self_accept":{},"adversarial_review_required":{},"reviewer_separation_observed":{},"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"secret_values_recorded":false,"judge_response_text_recorded":false,"reasoning_text_recorded":false,"production_writes_allowed":false}}"#,
        proof.sequence,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.subject_receipt_id),
        json_string_literal(&proof.model_a),
        json_string_literal(&proof.model_a_provider),
        json_string_literal(&proof.model_a_verdict),
        proof.model_a_confidence,
        json_string_literal(&proof.model_b),
        json_string_literal(&proof.model_b_provider),
        json_string_literal(&proof.model_b_verdict),
        proof.model_b_confidence,
        proof.confidence_threshold,
        json_string_literal(&proof.combined_verdict),
        proof.combined_confidence,
        json_string_literal(&proof.status),
        proof.disagreement,
        proof.needs_review,
        proof.max_retries,
        proof.parallel_execution_observed,
        proof.fresh_reviewer_context_required,
        proof.builder_can_self_accept,
        proof.adversarial_review_required,
        proof.reviewer_separation_observed
    )
}

fn render_worker_execution_proof_json(proof: &DxrWorkerExecutionProof) -> String {
    format!(
        r#"{{"sequence":{},"proof_id":{},"tenant_id":{},"actor_id":{},"worker_run_id":{},"claim_id":{},"spawn_receipt_id":{},"credential_check_receipt_id":{},"handoff_receipt_id":{},"retirement_receipt_id":{},"preflight_id":{},"status":{},"terminal_state":"DXR_WORKER_EXECUTION_LOCAL_PROOF_RECORDED_LIVE_AUTHORITY_BLOCKED","max_runtime_ms":{},"observed_runtime_ms":{},"max_tool_calls":{},"observed_tool_call_count":{},"ordered_receipts_observed":{},"claim_lease_observed":{},"heartbeat_observed":{},"bounded_runtime_observed":{},"policy_denied_tool_observed":{},"handoff_before_retirement":{},"provider_turn_on_required":true,"provider_turn_on_observed":false,"worker_process_started":false,"live_worker_execution_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"worker_execution_gate_doc":"docs/WORKER-LIVE-EXECUTION-GATE.md","v1_floor":["claim_lease","heartbeat","bounded_runtime","tool_policy","evidence_event","handoff_retirement_order"]}}"#,
        proof.sequence,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.worker_run_id),
        json_string_literal(&proof.claim_id),
        json_string_literal(&proof.spawn_receipt_id),
        json_string_literal(&proof.credential_check_receipt_id),
        json_string_literal(&proof.handoff_receipt_id),
        json_string_literal(&proof.retirement_receipt_id),
        json_string_literal(&proof.preflight_id),
        json_string_literal(&proof.status),
        proof.max_runtime_ms,
        proof.observed_runtime_ms,
        proof.max_tool_calls,
        proof.observed_tool_call_count,
        proof.ordered_receipts_observed,
        proof.claim_lease_observed,
        proof.heartbeat_observed,
        proof.bounded_runtime_observed,
        proof.policy_denied_tool_observed,
        proof.handoff_before_retirement
    )
}

fn render_model_provider_observation_json(observation: &DxrModelProviderObservation) -> String {
    format!(
        r#"{{"sequence":{},"observation_id":{},"tenant_id":{},"actor_id":{},"provider":{},"adapter":{},"model_id":{},"status":{},"observed":{},"required_receipt_kind":{},"observed_receipt_id":{},"approval_receipt_id":{},"evidence_file":{},"stream_chunk_count":{},"terminal_event_observed":{},"fallback_supported":{},"total_tokens":{},"provider_connected_streaming_ready":{},"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_writes_allowed":false}}"#,
        observation.sequence,
        json_string_literal(&observation.observation_id),
        json_string_literal(&observation.tenant_id),
        json_string_literal(&observation.actor_id),
        json_string_literal(&observation.provider),
        json_string_literal(&observation.adapter),
        json_string_literal(&observation.model_id),
        json_string_literal(&observation.status),
        observation.observed,
        json_string_literal(&observation.required_receipt_kind),
        json_string_literal(&observation.observed_receipt_id),
        json_string_literal(&observation.approval_receipt_id),
        json_string_literal(&observation.evidence_file),
        observation.stream_chunk_count,
        observation.terminal_event_observed,
        observation.fallback_supported,
        observation.total_tokens,
        observation.observed
    )
}

fn render_model_turn_json(turn: &DxrModelTurn) -> String {
    format!(
        r#"{{"sequence":{},"turn_id":{},"tenant_id":{},"actor_id":{},"role":{},"trust_boundary":{},"prompt_hash":{},"primary_model":{},"primary_provider":{},"fallback_model":{},"fallback_provider":{},"selected_model":{},"selected_provider":{},"routing_status":{},"streaming_status":{},"terminal_state":{},"fallback_used":{},"retry_count":{},"chunk_count":{},"chunks":[{}],"provider_calls_allowed":false,"live_provider_turn_on_required":true,"secret_value_recorded":false,"production_writes_allowed":false}}"#,
        turn.sequence,
        json_string_literal(&turn.turn_id),
        json_string_literal(&turn.tenant_id),
        json_string_literal(&turn.actor_id),
        json_string_literal(&turn.role),
        json_string_literal(&turn.trust_boundary),
        json_string_literal(&turn.prompt_hash),
        json_string_literal(&turn.primary_model),
        json_string_literal(&turn.primary_provider),
        json_string_literal(&turn.fallback_model),
        json_string_literal(&turn.fallback_provider),
        json_string_literal(&turn.selected_model),
        json_string_literal(&turn.selected_provider),
        json_string_literal(&turn.routing_status),
        json_string_literal(&turn.streaming_status),
        json_string_literal(&turn.terminal_state),
        turn.fallback_used,
        turn.retry_count,
        turn.chunk_count,
        turn.chunks
            .iter()
            .map(render_model_stream_chunk_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn render_model_stream_chunk_json(chunk: &DxrModelStreamChunk) -> String {
    format!(
        r#"{{"index":{},"event":{},"delta":{},"terminal":{}}}"#,
        chunk.index,
        json_string_literal(&chunk.event),
        json_string_literal(&chunk.delta),
        chunk.terminal
    )
}

fn stable_hash(parts: &[&str]) -> String {
    stable_token(parts)
}

fn stable_token(parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn string_array(value: &Value, key: &str, defaults: &[&str]) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|value| (*value).to_string()).collect())
}

fn run_valkey_command(container: Option<&str>, args: &[&str]) -> Result<String, String> {
    let candidates = container
        .map(|container| vec![container.to_string()])
        .unwrap_or_else(|| {
            vec![
                "mdx-native-ui-valkey-1".to_string(),
                "mdx-native-valkey-1".to_string(),
            ]
        });
    let mut errors = Vec::new();
    for candidate in candidates {
        let output = Command::new("docker")
            .arg("exec")
            .arg(&candidate)
            .arg("valkey-cli")
            .args(args)
            .output();
        match output {
            Ok(output) if output.status.success() => {
                return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
            Ok(output) => errors.push(format!(
                "{candidate}:{}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => errors.push(format!("{candidate}:{error}")),
        }
    }
    Err(errors.join(";"))
}

struct Response {
    status: &'static str,
    content_type: &'static str,
    body: String,
}

impl Response {
    fn text(status: &'static str, body: String) -> Self {
        Self {
            status,
            content_type: TEXT_CONTENT_TYPE,
            body,
        }
    }

    fn json(status: &'static str, body: String) -> Self {
        Self {
            status,
            content_type: JSON_CONTENT_TYPE,
            body,
        }
    }
}

#[allow(dead_code)]
fn render_error_json(message: &str) -> String {
    format!(r#"{{"error":{}}}"#, json_string_literal(message))
}
