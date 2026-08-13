use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_WORKFLOW_TYPE: &str = "forge_governed_build";
const DEFAULT_TASK_QUEUE: &str = "mdx-dxr-local-durable-workflows";
const DEFAULT_NAMESPACE: &str = "mdx-local";
const DEFAULT_SOURCE_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const DEFAULT_SOURCE_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_WORKFLOW_BATCH_SIZE: usize = 320;
const DEFAULT_ACTIVE_TENANT_COUNT: usize = 10;
const DEFAULT_MAX_CONCURRENT_WORKFLOWS_PER_TENANT: usize = 500;
const DEFAULT_GLOBAL_WORKFLOW_LIMIT: usize = 5000;
const DEFAULT_REPLAY_WINDOW_SECONDS: usize = 86_400;
const DEFAULT_RESUME_CHECKPOINT_INTERVAL_SECONDS: usize = 30;

#[derive(Default)]
pub struct DxrWorkflowRuntime {
    runs: Vec<DxrWorkflowRun>,
    next_run: usize,
}

pub struct DxrWorkflowResult {
    pub body: String,
    pub run: DxrWorkflowRun,
    pub events: Vec<DxrWorkflowRuntimeEvent>,
}

pub struct DxrWorkflowRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
pub struct DxrWorkflowRun {
    pub sequence: usize,
    pub workflow_run_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub workflow_type: String,
    pub workflow_runner_driver: String,
    pub workflow_runner_provider: String,
    pub workflow_runtime: String,
    pub task_queue: String,
    pub namespace: String,
    pub retry_policy: String,
    pub timeout_seconds: usize,
    pub source_job_id: String,
    pub source_run_id: String,
    pub idempotency_key: String,
    pub target_concurrent_engineers: usize,
    pub peak_parallel_forge_builds: usize,
    pub requested_workflow_count: usize,
    pub accepted_workflow_count: usize,
    pub rejected_workflow_count: usize,
    pub active_tenant_count: usize,
    pub max_concurrent_workflows_per_tenant: usize,
    pub global_workflow_limit: usize,
    pub workflow_batch_size: usize,
    pub workflow_batches_required: usize,
    pub durable_checkpoint_count: usize,
    pub replay_window_seconds: usize,
    pub resume_checkpoint_interval_seconds: usize,
    pub resume_strategy: String,
    pub reviewer_context_mode: String,
    pub status: String,
    pub terminal_state: String,
    pub live_worker_execution_allowed: bool,
    pub temporal_durability_claimed: bool,
    pub events: Vec<DxrWorkflowEvent>,
    pub shards: Vec<DxrWorkflowShard>,
}

#[derive(Clone)]
pub struct DxrWorkflowEvent {
    pub sequence: usize,
    pub workflow_event_id: String,
    pub event_type: String,
    pub terminal_state: String,
}

#[derive(Clone)]
pub struct DxrWorkflowShard {
    pub shard_index: usize,
    pub shard_id: String,
    pub admitted_workflow_count: usize,
    pub checkpoint_id: String,
    pub resume_cursor: String,
    pub lease_policy: String,
    pub reviewer_context_mode: String,
    pub terminal_state: String,
}

struct WorkflowRequest {
    tenant_id: String,
    actor_id: String,
    workflow_type: String,
    task_queue: String,
    namespace: String,
    retry_policy: String,
    timeout_seconds: usize,
    source_job_id: String,
    source_run_id: String,
    idempotency_key: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_workflow_count: usize,
    active_tenant_count: usize,
    max_concurrent_workflows_per_tenant: usize,
    global_workflow_limit: usize,
    workflow_batch_size: usize,
    replay_window_seconds: usize,
    resume_checkpoint_interval_seconds: usize,
    resume_strategy: String,
    reviewer_context_mode: String,
    builder_can_self_accept: bool,
    provider_calls_allowed: bool,
    tool_execution_allowed: bool,
    worker_execution_allowed: bool,
    production_writes_allowed: bool,
}

impl DxrWorkflowRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrWorkflowResult, String> {
        let request = parse_workflow_request(body)?;
        self.next_run += 1;
        let workflow_run_id = format!("dxr_workflow_run_{:06}", self.next_run);
        let tenant_capacity = request
            .active_tenant_count
            .saturating_mul(request.max_concurrent_workflows_per_tenant);
        let accepted_workflow_count = request
            .requested_workflow_count
            .min(request.peak_parallel_forge_builds)
            .min(request.global_workflow_limit)
            .min(tenant_capacity);
        let rejected_workflow_count = request
            .requested_workflow_count
            .saturating_sub(accepted_workflow_count);
        let workflow_batches_required =
            ceil_div(accepted_workflow_count, request.workflow_batch_size.max(1));
        let durable_checkpoint_count = workflow_batches_required.saturating_add(2);
        let unsafe_authority_requested = request.builder_can_self_accept
            || request.provider_calls_allowed
            || request.tool_execution_allowed
            || request.worker_execution_allowed
            || request.production_writes_allowed;
        let terminal_state = if unsafe_authority_requested {
            "DXR_DURABLE_WORKFLOW_RECORDED_AUTHORITY_BLOCKED"
        } else if rejected_workflow_count > 0 {
            "DXR_DURABLE_WORKFLOW_RECORDED_ADMISSION_BACKPRESSURE"
        } else {
            "DXR_DURABLE_WORKFLOW_RECORDED_LIVE_WORKER_BLOCKED"
        };
        let shards = build_workflow_shards(
            &workflow_run_id,
            accepted_workflow_count,
            request.workflow_batch_size,
            &request.reviewer_context_mode,
            terminal_state,
        );
        let events = vec![
            DxrWorkflowEvent {
                sequence: 1,
                workflow_event_id: format!("{workflow_run_id}_event_001"),
                event_type: "workflow_submitted".to_string(),
                terminal_state: "DXR_DURABLE_WORKFLOW_SUBMITTED".to_string(),
            },
            DxrWorkflowEvent {
                sequence: 2,
                workflow_event_id: format!("{workflow_run_id}_event_002"),
                event_type: "workflow_started".to_string(),
                terminal_state: "DXR_DURABLE_WORKFLOW_STARTED".to_string(),
            },
            DxrWorkflowEvent {
                sequence: 3,
                workflow_event_id: format!("{workflow_run_id}_event_003"),
                event_type: "workflow_batch_admission_recorded".to_string(),
                terminal_state: "DXR_DURABLE_WORKFLOW_BATCH_ADMISSION_RECORDED".to_string(),
            },
            DxrWorkflowEvent {
                sequence: 4,
                workflow_event_id: format!("{workflow_run_id}_event_004"),
                event_type: "workflow_replay_checkpoint_recorded".to_string(),
                terminal_state: "DXR_DURABLE_WORKFLOW_REPLAY_CHECKPOINT_RECORDED".to_string(),
            },
            DxrWorkflowEvent {
                sequence: 5,
                workflow_event_id: format!("{workflow_run_id}_event_005"),
                event_type: "workflow_resume_cursor_recorded".to_string(),
                terminal_state: "DXR_DURABLE_WORKFLOW_RESUME_CURSOR_RECORDED".to_string(),
            },
            DxrWorkflowEvent {
                sequence: 6,
                workflow_event_id: format!("{workflow_run_id}_event_006"),
                event_type: "workflow_reviewer_separation_required".to_string(),
                terminal_state: "DXR_DURABLE_WORKFLOW_REVIEWER_SEPARATION_REQUIRED".to_string(),
            },
            DxrWorkflowEvent {
                sequence: 7,
                workflow_event_id: format!("{workflow_run_id}_event_007"),
                event_type: "workflow_shard_schedule_recorded".to_string(),
                terminal_state: "DXR_DURABLE_WORKFLOW_SHARD_SCHEDULE_RECORDED".to_string(),
            },
            DxrWorkflowEvent {
                sequence: 8,
                workflow_event_id: format!("{workflow_run_id}_event_008"),
                event_type: "workflow_completed_live_worker_blocked".to_string(),
                terminal_state: terminal_state.to_string(),
            },
        ];
        let run = DxrWorkflowRun {
            sequence: self.next_run,
            workflow_run_id,
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            workflow_type: request.workflow_type,
            workflow_runner_driver: "postgres_durable_workflow_runner".to_string(),
            workflow_runner_provider: "PostgresDurableWorkflowRunner".to_string(),
            workflow_runtime: "postgres_local_durable_queue".to_string(),
            task_queue: request.task_queue,
            namespace: request.namespace,
            retry_policy: request.retry_policy,
            timeout_seconds: request.timeout_seconds,
            source_job_id: request.source_job_id,
            source_run_id: request.source_run_id,
            idempotency_key: request.idempotency_key,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            requested_workflow_count: request.requested_workflow_count,
            accepted_workflow_count,
            rejected_workflow_count,
            active_tenant_count: request.active_tenant_count,
            max_concurrent_workflows_per_tenant: request.max_concurrent_workflows_per_tenant,
            global_workflow_limit: request.global_workflow_limit,
            workflow_batch_size: request.workflow_batch_size,
            workflow_batches_required,
            durable_checkpoint_count,
            replay_window_seconds: request.replay_window_seconds,
            resume_checkpoint_interval_seconds: request.resume_checkpoint_interval_seconds,
            resume_strategy: request.resume_strategy,
            reviewer_context_mode: request.reviewer_context_mode,
            status: "completed".to_string(),
            terminal_state: terminal_state.to_string(),
            live_worker_execution_allowed: false,
            temporal_durability_claimed: false,
            events,
            shards,
        };
        let relay_events = workflow_runtime_events(&run);
        let body = render_workflow_run_response_json(&run);
        self.runs.push(run.clone());
        Ok(DxrWorkflowResult {
            body,
            run,
            events: relay_events,
        })
    }

    pub fn runs_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-dxr-durable-workflows","status":"LIVE-LOCAL-DXR-DURABLE-WORKFLOW","runtime":"mdx-dxr-engine","route":"/dxr/durable-workflows.json","submit_route":"/v1/dxr/durable-workflows","workflow_runner_driver":"postgres_durable_workflow_runner","workflow_runner_provider":"PostgresDurableWorkflowRunner","temporal_status":"PENDING-LIVE-RUN","admission_policy":"tenant_fair_durable_batch_admission","resume_strategy":"checkpointed_replay_with_idempotency_key","shard_schedule_policy":"tenant_fair_batch_shards_with_resume_cursors","target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"workflow_batch_size":{},"global_workflow_limit":{},"replay_window_seconds":{},"resume_checkpoint_interval_seconds":{},"fresh_reviewer_context_required":true,"builder_can_self_accept":false,"run_count":{},"runs":[{}],"live_worker_execution_allowed":false,"temporal_durability_claimed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
            DEFAULT_TARGET_ENGINEERS,
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
            DEFAULT_WORKFLOW_BATCH_SIZE,
            DEFAULT_GLOBAL_WORKFLOW_LIMIT,
            DEFAULT_REPLAY_WINDOW_SECONDS,
            DEFAULT_RESUME_CHECKPOINT_INTERVAL_SECONDS,
            self.runs.len(),
            self.runs
                .iter()
                .map(render_workflow_run_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_workflow_request(body: &str) -> Result<WorkflowRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR workflow json: {error}"))?;
    Ok(WorkflowRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        workflow_type: string_value(&value, "workflow_type", DEFAULT_WORKFLOW_TYPE),
        task_queue: string_value(&value, "task_queue", DEFAULT_TASK_QUEUE),
        namespace: string_value(&value, "namespace", DEFAULT_NAMESPACE),
        retry_policy: string_value(&value, "retry_policy", "bounded_retry_max_3"),
        timeout_seconds: value
            .get("timeout_seconds")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(30),
        source_job_id: string_value(&value, "source_job_id", DEFAULT_SOURCE_JOB_ID),
        source_run_id: string_value(&value, "source_run_id", DEFAULT_SOURCE_RUN_ID),
        idempotency_key: string_value(&value, "idempotency_key", "dxr-workflow-idempotency-001"),
        target_concurrent_engineers: usize_value(
            &value,
            "target_concurrent_engineers",
            DEFAULT_TARGET_ENGINEERS,
        ),
        peak_parallel_forge_builds: usize_value(
            &value,
            "peak_parallel_forge_builds",
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
        ),
        requested_workflow_count: usize_value(
            &value,
            "requested_workflow_count",
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
        ),
        active_tenant_count: usize_value(
            &value,
            "active_tenant_count",
            DEFAULT_ACTIVE_TENANT_COUNT,
        ),
        max_concurrent_workflows_per_tenant: usize_value(
            &value,
            "max_concurrent_workflows_per_tenant",
            DEFAULT_MAX_CONCURRENT_WORKFLOWS_PER_TENANT,
        ),
        global_workflow_limit: usize_value(
            &value,
            "global_workflow_limit",
            DEFAULT_GLOBAL_WORKFLOW_LIMIT,
        ),
        workflow_batch_size: usize_value(
            &value,
            "workflow_batch_size",
            DEFAULT_WORKFLOW_BATCH_SIZE,
        ),
        replay_window_seconds: usize_value(
            &value,
            "replay_window_seconds",
            DEFAULT_REPLAY_WINDOW_SECONDS,
        ),
        resume_checkpoint_interval_seconds: usize_value(
            &value,
            "resume_checkpoint_interval_seconds",
            DEFAULT_RESUME_CHECKPOINT_INTERVAL_SECONDS,
        ),
        resume_strategy: string_value(
            &value,
            "resume_strategy",
            "checkpointed_replay_with_idempotency_key",
        ),
        reviewer_context_mode: string_value(
            &value,
            "reviewer_context_mode",
            "fresh_context_required",
        ),
        builder_can_self_accept: bool_value(&value, "builder_can_self_accept", false),
        provider_calls_allowed: bool_value(&value, "provider_calls_allowed", false),
        tool_execution_allowed: bool_value(&value, "tool_execution_allowed", false),
        worker_execution_allowed: bool_value(&value, "worker_execution_allowed", false),
        production_writes_allowed: bool_value(&value, "production_writes_allowed", false),
    })
}

fn workflow_runtime_events(run: &DxrWorkflowRun) -> Vec<DxrWorkflowRuntimeEvent> {
    run.events
        .iter()
        .map(|event| DxrWorkflowRuntimeEvent {
            event_type: event.event_type.clone(),
            tenant_id: run.tenant_id.clone(),
            job_id: run.source_job_id.clone(),
            run_id: run.source_run_id.clone(),
            actor_id: run.actor_id.clone(),
        })
        .collect()
}

fn build_workflow_shards(
    workflow_run_id: &str,
    accepted_workflow_count: usize,
    workflow_batch_size: usize,
    reviewer_context_mode: &str,
    terminal_state: &str,
) -> Vec<DxrWorkflowShard> {
    let batch_size = workflow_batch_size.max(1);
    let shard_count = ceil_div(accepted_workflow_count, batch_size);
    (0..shard_count)
        .map(|index| {
            let remaining = accepted_workflow_count.saturating_sub(index * batch_size);
            let admitted_workflow_count = remaining.min(batch_size);
            DxrWorkflowShard {
                shard_index: index + 1,
                shard_id: format!("{workflow_run_id}_shard_{:03}", index + 1),
                admitted_workflow_count,
                checkpoint_id: format!("{workflow_run_id}_checkpoint_{:03}", index + 1),
                resume_cursor: format!("{workflow_run_id}:resume:{:03}", index + 1),
                lease_policy: "lease_with_heartbeat_and_idempotency_key".to_string(),
                reviewer_context_mode: reviewer_context_mode.to_string(),
                terminal_state: terminal_state.to_string(),
            }
        })
        .collect()
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
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn ceil_div(value: usize, divisor: usize) -> usize {
    if value == 0 {
        0
    } else {
        value.saturating_add(divisor).saturating_sub(1) / divisor
    }
}

pub fn render_workflow_run_response_json(run: &DxrWorkflowRun) -> String {
    format!(
        r#"{{"name":"mdx-dxr-durable-workflow","status":"LIVE-LOCAL-DXR-DURABLE-WORKFLOW","runtime":"mdx-dxr-engine","workflow":{},"workflow_run_id":{},"tenant_id":{},"actor_id":{},"workflow_runner_driver":{},"workflow_runner_provider":{},"workflow_runtime":{},"task_queue":{},"namespace":{},"retry_policy":{},"timeout_seconds":{},"source_job_id":{},"source_run_id":{},"idempotency_key":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_workflow_count":{},"accepted_workflow_count":{},"rejected_workflow_count":{},"workflow_batch_size":{},"workflow_batches_required":{},"durable_checkpoint_count":{},"workflow_shard_count":{},"workflow_shards":[{}],"replay_window_seconds":{},"resume_checkpoint_interval_seconds":{},"resume_strategy":{},"reviewer_context_mode":{},"terminal_state":{},"event_count":{},"events":[{}],"fresh_reviewer_context_required":true,"builder_can_self_accept":false,"live_worker_execution_allowed":false,"temporal_durability_claimed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
        render_workflow_run_json(run),
        json_string_literal(&run.workflow_run_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.workflow_runner_driver),
        json_string_literal(&run.workflow_runner_provider),
        json_string_literal(&run.workflow_runtime),
        json_string_literal(&run.task_queue),
        json_string_literal(&run.namespace),
        json_string_literal(&run.retry_policy),
        run.timeout_seconds,
        json_string_literal(&run.source_job_id),
        json_string_literal(&run.source_run_id),
        json_string_literal(&run.idempotency_key),
        run.target_concurrent_engineers,
        run.peak_parallel_forge_builds,
        run.requested_workflow_count,
        run.accepted_workflow_count,
        run.rejected_workflow_count,
        run.workflow_batch_size,
        run.workflow_batches_required,
        run.durable_checkpoint_count,
        run.shards.len(),
        render_workflow_shards_json(&run.shards),
        run.replay_window_seconds,
        run.resume_checkpoint_interval_seconds,
        json_string_literal(&run.resume_strategy),
        json_string_literal(&run.reviewer_context_mode),
        json_string_literal(&run.terminal_state),
        run.events.len(),
        run.events
            .iter()
            .map(render_workflow_event_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub fn render_workflow_run_json(run: &DxrWorkflowRun) -> String {
    format!(
        r#"{{"sequence":{},"workflow_run_id":{},"tenant_id":{},"actor_id":{},"workflow_type":{},"workflow_runner_driver":{},"workflow_runner_provider":{},"workflow_runtime":{},"task_queue":{},"namespace":{},"retry_policy":{},"timeout_seconds":{},"status":{},"terminal_state":{},"source_job_id":{},"source_run_id":{},"idempotency_key":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_workflow_count":{},"accepted_workflow_count":{},"rejected_workflow_count":{},"active_tenant_count":{},"max_concurrent_workflows_per_tenant":{},"global_workflow_limit":{},"workflow_batch_size":{},"workflow_batches_required":{},"durable_checkpoint_count":{},"workflow_shard_count":{},"workflow_shards":[{}],"replay_window_seconds":{},"resume_checkpoint_interval_seconds":{},"resume_strategy":{},"reviewer_context_mode":{},"event_count":{},"events":[{}],"fresh_reviewer_context_required":true,"builder_can_self_accept":false,"live_worker_execution_allowed":{},"temporal_durability_claimed":{},"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
        run.sequence,
        json_string_literal(&run.workflow_run_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.workflow_type),
        json_string_literal(&run.workflow_runner_driver),
        json_string_literal(&run.workflow_runner_provider),
        json_string_literal(&run.workflow_runtime),
        json_string_literal(&run.task_queue),
        json_string_literal(&run.namespace),
        json_string_literal(&run.retry_policy),
        run.timeout_seconds,
        json_string_literal(&run.status),
        json_string_literal(&run.terminal_state),
        json_string_literal(&run.source_job_id),
        json_string_literal(&run.source_run_id),
        json_string_literal(&run.idempotency_key),
        run.target_concurrent_engineers,
        run.peak_parallel_forge_builds,
        run.requested_workflow_count,
        run.accepted_workflow_count,
        run.rejected_workflow_count,
        run.active_tenant_count,
        run.max_concurrent_workflows_per_tenant,
        run.global_workflow_limit,
        run.workflow_batch_size,
        run.workflow_batches_required,
        run.durable_checkpoint_count,
        run.shards.len(),
        render_workflow_shards_json(&run.shards),
        run.replay_window_seconds,
        run.resume_checkpoint_interval_seconds,
        json_string_literal(&run.resume_strategy),
        json_string_literal(&run.reviewer_context_mode),
        run.events.len(),
        run.events
            .iter()
            .map(render_workflow_event_json)
            .collect::<Vec<_>>()
            .join(","),
        run.live_worker_execution_allowed,
        run.temporal_durability_claimed
    )
}

fn render_workflow_shards_json(shards: &[DxrWorkflowShard]) -> String {
    shards
        .iter()
        .map(render_workflow_shard_json)
        .collect::<Vec<_>>()
        .join(",")
}

fn render_workflow_shard_json(shard: &DxrWorkflowShard) -> String {
    format!(
        r#"{{"shard_index":{},"shard_id":{},"admitted_workflow_count":{},"checkpoint_id":{},"resume_cursor":{},"lease_policy":{},"reviewer_context_mode":{},"terminal_state":{},"live_worker_execution_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
        shard.shard_index,
        json_string_literal(&shard.shard_id),
        shard.admitted_workflow_count,
        json_string_literal(&shard.checkpoint_id),
        json_string_literal(&shard.resume_cursor),
        json_string_literal(&shard.lease_policy),
        json_string_literal(&shard.reviewer_context_mode),
        json_string_literal(&shard.terminal_state)
    )
}

fn render_workflow_event_json(event: &DxrWorkflowEvent) -> String {
    format!(
        r#"{{"sequence":{},"workflow_event_id":{},"event_type":{},"terminal_state":{},"durable_workflow_status":"LIVE-LOCAL-DXR-DURABLE-WORKFLOW"}}"#,
        event.sequence,
        json_string_literal(&event.workflow_event_id),
        json_string_literal(&event.event_type),
        json_string_literal(&event.terminal_state)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_records_durable_local_boundary() {
        let mut runtime = DxrWorkflowRuntime::new();
        let result = runtime.submit_json("{}").expect("workflow");
        assert!(result.body.contains("LIVE-LOCAL-DXR-DURABLE-WORKFLOW"));
        assert!(result.body.contains("PostgresDurableWorkflowRunner"));
        assert!(result.body.contains("\"accepted_workflow_count\":5000"));
        assert!(result.body.contains("\"workflow_batches_required\":16"));
        assert!(result.body.contains("\"workflow_shard_count\":16"));
        assert!(
            result
                .body
                .contains("lease_with_heartbeat_and_idempotency_key")
        );
        assert!(
            result
                .body
                .contains("checkpointed_replay_with_idempotency_key")
        );
        assert!(result.body.contains("workflow_replay_checkpoint_recorded"));
        assert!(
            result
                .body
                .contains("workflow_completed_live_worker_blocked")
        );
        assert_eq!(result.events.len(), 8);
    }
}
