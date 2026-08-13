use mdx_core::{DxrLocalRuntimePacket, json_string_literal};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::ctx_context::DxrCtxContextInput;
use crate::forge_execution::DxrForgeExecutionDurableRecord;
use crate::workflow::{DxrWorkflowEvent, DxrWorkflowRun};

pub struct DxrDurableStore {
    database_url: Option<String>,
}

impl DxrDurableStore {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    pub fn status(&self) -> &'static str {
        if self.database_url.is_some() {
            "POSTGRES_DURABLE_DXR_CONFIGURED"
        } else {
            "LOCAL_PROCESS_DXR_ONLY"
        }
    }

    pub fn record_job(
        &self,
        tenant_id: &str,
        actor_id: &str,
        packet: &DxrLocalRuntimePacket,
        payload_json: &str,
    ) {
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO dxr_runtime_jobs (job_id, tenant_id, run_id, actor_id, idempotency_key, status, terminal_state, ctx_assembly_receipt_id, evidence_record_count, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (job_id) DO UPDATE SET status = EXCLUDED.status, terminal_state = EXCLUDED.terminal_state, evidence_record_count = EXCLUDED.evidence_record_count, payload = EXCLUDED.payload, updated_at = now();\n{}\nSELECT count(*) FROM dxr_runtime_jobs WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            identity_sql(tenant_id, actor_id),
            sql_string_literal(&packet.job.job_id),
            sql_string_literal(tenant_id),
            sql_string_literal(&packet.run.run_id),
            sql_string_literal(actor_id),
            sql_string_literal(&packet.job.idempotency_key),
            sql_string_literal(&packet.job.status),
            sql_string_literal(&packet.run.status),
            sql_string_literal(&packet.ctx_assembly_receipt_id),
            packet.evidence_records.len(),
            sql_string_literal(payload_json),
            runtime_evidence_insert_sql(tenant_id, actor_id, packet),
            sql_string_literal(tenant_id)
        );
        self.run_count(&sql);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_event(
        &self,
        tenant_id: &str,
        actor_id: &str,
        event_id: &str,
        event_type: &str,
        job_id: &str,
        run_id: &str,
        topic: &str,
        relay_status: &str,
        payload_json: &str,
    ) {
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO dxr_runtime_events (event_id, tenant_id, event_type, job_id, run_id, actor_id, stream_id, topic, relay_status, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (event_id) DO UPDATE SET relay_status = EXCLUDED.relay_status, payload = EXCLUDED.payload;\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            identity_sql(tenant_id, actor_id),
            sql_string_literal(event_id),
            sql_string_literal(tenant_id),
            sql_string_literal(event_type),
            sql_string_literal(job_id),
            sql_string_literal(run_id),
            sql_string_literal(actor_id),
            sql_string_literal("dxr_event_stream"),
            sql_string_literal(topic),
            sql_string_literal(relay_status),
            sql_string_literal(payload_json),
            sql_string_literal(tenant_id)
        );
        self.run_count(&sql);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_model_turn(
        &self,
        tenant_id: &str,
        actor_id: &str,
        turn_id: &str,
        selected_provider: &str,
        selected_model: &str,
        routing_status: &str,
        streaming_status: &str,
        terminal_state: &str,
        fallback_used: bool,
        chunk_count: usize,
        payload_json: &str,
    ) {
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO dxr_model_turns (turn_id, tenant_id, actor_id, selected_provider, selected_model, routing_status, streaming_status, terminal_state, fallback_used, chunk_count, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (turn_id) DO UPDATE SET routing_status = EXCLUDED.routing_status, streaming_status = EXCLUDED.streaming_status, terminal_state = EXCLUDED.terminal_state, fallback_used = EXCLUDED.fallback_used, chunk_count = EXCLUDED.chunk_count, payload = EXCLUDED.payload;\nSELECT count(*) FROM dxr_model_turns WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            identity_sql(tenant_id, actor_id),
            sql_string_literal(turn_id),
            sql_string_literal(tenant_id),
            sql_string_literal(actor_id),
            sql_string_literal(selected_provider),
            sql_string_literal(selected_model),
            sql_string_literal(routing_status),
            sql_string_literal(streaming_status),
            sql_string_literal(terminal_state),
            fallback_used,
            chunk_count,
            sql_string_literal(payload_json),
            sql_string_literal(tenant_id)
        );
        self.run_count(&sql);
    }

    pub fn record_worker_boundary(
        &self,
        tenant_id: &str,
        actor_id: &str,
        boundary_id: &str,
        worker_run_id: &str,
        payload_json: &str,
    ) {
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO dxr_worker_boundaries (boundary_id, tenant_id, actor_id, worker_run_id, runtime_status, terminal_state, live_worker_execution_allowed, payload) VALUES ({}, {}, {}, {}, {}, {}, false, {}::jsonb) ON CONFLICT (boundary_id) DO UPDATE SET terminal_state = EXCLUDED.terminal_state, payload = EXCLUDED.payload;\nSELECT count(*) FROM dxr_worker_boundaries WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            identity_sql(tenant_id, actor_id),
            sql_string_literal(boundary_id),
            sql_string_literal(tenant_id),
            sql_string_literal(actor_id),
            sql_string_literal(worker_run_id),
            sql_string_literal("LIVE-LOCAL-DXR-WORKER-LIFECYCLE"),
            sql_string_literal("WORKER_LIFECYCLE_RECORDED_LIVE_EXECUTION_BLOCKED"),
            sql_string_literal(payload_json),
            sql_string_literal(tenant_id)
        );
        self.run_count(&sql);
    }

    pub fn record_workflow_run(&self, run: &DxrWorkflowRun, payload_json: &str) {
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO dxr_workflow_runs (workflow_run_id, tenant_id, actor_id, workflow_type, workflow_runner_driver, workflow_runner_provider, workflow_runtime, task_queue, namespace, retry_policy, timeout_seconds, status, terminal_state, source_job_id, source_run_id, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (workflow_run_id) DO UPDATE SET status = EXCLUDED.status, terminal_state = EXCLUDED.terminal_state, payload = EXCLUDED.payload, updated_at = now();\n{}\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(&run.tenant_id),
            identity_sql(&run.tenant_id, &run.actor_id),
            sql_string_literal(&run.workflow_run_id),
            sql_string_literal(&run.tenant_id),
            sql_string_literal(&run.actor_id),
            sql_string_literal(&run.workflow_type),
            sql_string_literal(&run.workflow_runner_driver),
            sql_string_literal(&run.workflow_runner_provider),
            sql_string_literal(&run.workflow_runtime),
            sql_string_literal(&run.task_queue),
            sql_string_literal(&run.namespace),
            sql_string_literal(&run.retry_policy),
            run.timeout_seconds,
            sql_string_literal(&run.status),
            sql_string_literal(&run.terminal_state),
            sql_string_literal(&run.source_job_id),
            sql_string_literal(&run.source_run_id),
            sql_string_literal(payload_json),
            run.events
                .iter()
                .map(|event| workflow_event_insert_sql(run, event))
                .collect::<Vec<_>>()
                .join("\n"),
            sql_string_literal(&run.tenant_id)
        );
        self.run_count(&sql);
    }

    pub fn record_ctx_context_input(&self, input: &DxrCtxContextInput, payload_json: &str) {
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO dxr_ctx_context_inputs (ctx_context_input_id, tenant_id, actor_id, job_id, run_id, ctx_assembly_receipt_id, ctx_session_id, ctx_handoff_id, ctx_state_keys, ctx_background_job_id, ctx_memory_tiers, context_payload, context_integrity_hash, status, terminal_state) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb, {}, {}::jsonb, {}::jsonb, {}, {}, {}) ON CONFLICT (ctx_context_input_id) DO UPDATE SET context_payload = EXCLUDED.context_payload, context_integrity_hash = EXCLUDED.context_integrity_hash, status = EXCLUDED.status, terminal_state = EXCLUDED.terminal_state;\nINSERT INTO dxr_runtime_events (event_id, tenant_id, event_type, job_id, run_id, actor_id, stream_id, topic, relay_status, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (event_id) DO UPDATE SET relay_status = EXCLUDED.relay_status, payload = EXCLUDED.payload;\nSELECT count(*) FROM dxr_ctx_context_inputs WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(&input.tenant_id),
            identity_sql(&input.tenant_id, &input.actor_id),
            sql_string_literal(&input.ctx_context_input_id),
            sql_string_literal(&input.tenant_id),
            sql_string_literal(&input.actor_id),
            sql_string_literal(&input.job_id),
            sql_string_literal(&input.run_id),
            sql_string_literal(&input.ctx_assembly_receipt_id),
            sql_string_literal(&input.ctx_session_id),
            sql_string_literal(&input.ctx_handoff_id),
            sql_string_literal(&json_array_literal(&input.ctx_state_keys)),
            sql_string_literal(&input.ctx_background_job_id),
            sql_string_literal(&json_array_literal(&input.ctx_memory_tiers)),
            sql_string_literal(&input.context_payload_json),
            sql_string_literal(&input.context_integrity_hash),
            sql_string_literal(&input.status),
            sql_string_literal(&input.terminal_state),
            sql_string_literal(&format!("{}_durable_event", input.ctx_context_input_id)),
            sql_string_literal(&input.tenant_id),
            sql_string_literal("ctx_operational_context_durable_recorded"),
            sql_string_literal(&input.job_id),
            sql_string_literal(&input.run_id),
            sql_string_literal(&input.actor_id),
            sql_string_literal("dxr_event_stream"),
            sql_string_literal(&format!("mdx:dxr:{}:events", input.tenant_id)),
            sql_string_literal("POSTGRES_DURABLE_DXR_CTX_CONTEXT_RECORDED"),
            sql_string_literal(payload_json),
            sql_string_literal(&input.tenant_id)
        );
        self.run_count(&sql);
    }

    pub fn record_forge_execution_proof(
        &self,
        proof: &DxrForgeExecutionDurableRecord,
        payload_json: &str,
    ) {
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO dxr_workflow_runs (workflow_run_id, tenant_id, actor_id, workflow_type, workflow_runner_driver, workflow_runner_provider, workflow_runtime, task_queue, namespace, retry_policy, timeout_seconds, status, terminal_state, source_job_id, source_run_id, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (workflow_run_id) DO UPDATE SET status = EXCLUDED.status, terminal_state = EXCLUDED.terminal_state, payload = EXCLUDED.payload, updated_at = now();\nINSERT INTO dxr_workflow_events (workflow_event_id, tenant_id, workflow_run_id, sequence, event_type, terminal_state, payload) VALUES ({}, {}, {}, 1, {}, {}, {}::jsonb) ON CONFLICT (workflow_event_id) DO UPDATE SET terminal_state = EXCLUDED.terminal_state, payload = EXCLUDED.payload;\n{}\n{}\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof';\nCOMMIT;\n",
            sql_string_literal(&proof.tenant_id),
            identity_sql(&proof.tenant_id, &proof.actor_id),
            sql_string_literal(&proof.proof_id),
            sql_string_literal(&proof.tenant_id),
            sql_string_literal(&proof.actor_id),
            sql_string_literal("dxr_forge_execution_proof"),
            sql_string_literal("local_dxr_forge_execution_proof"),
            sql_string_literal("mdx-dxr-engine"),
            sql_string_literal("mdx-dxr-engine"),
            sql_string_literal("mdx-dxr-local-forge-execution-proofs"),
            sql_string_literal("mdx-local"),
            sql_string_literal("local_durable_recovery"),
            86_400,
            sql_string_literal(&proof.status),
            sql_string_literal(&proof.terminal_state),
            sql_string_literal(&proof.job_id),
            sql_string_literal(&proof.run_id),
            sql_string_literal(payload_json),
            sql_string_literal(&format!("{}_recorded", proof.proof_id)),
            sql_string_literal(&proof.tenant_id),
            sql_string_literal(&proof.proof_id),
            sql_string_literal("forge_execution_proof_durable_recorded"),
            sql_string_literal(&proof.terminal_state),
            sql_string_literal(&format!(
                r#"{{"proof_id":{},"idempotency_key":{},"execution_decision":{},"terminal_state":{},"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
                json_string_literal(&proof.proof_id),
                json_string_literal(&proof.idempotency_key),
                json_string_literal(&proof.execution_decision),
                json_string_literal(&proof.terminal_state)
            )),
            forge_supervisor_event_insert_sql(proof),
            forge_supervisor_evidence_insert_sql(proof),
            sql_string_literal(&proof.tenant_id)
        );
        self.run_count(&sql);
    }

    pub fn render_summary_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-durable-state","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/durable-state.json","database_url_configured":false,"durable_runtime_ready":false,"job_count":0,"event_count":0,"model_turn_count":0,"worker_boundary_count":0,"workflow_run_count":0,"workflow_event_count":0,"dynamic_workflow_plan_run_count":0,"dynamic_workflow_control_run_count":0,"dynamic_workflow_control_event_count":0,"dynamic_workflow_control_rejected_count":0,"durable_dynamic_workflow_control_status":"LOCAL_PROCESS_DXR_ONLY","ctx_context_input_count":0,"evidence_ledger_count":0,"run_dependency_count":0,"tool_execution_count":0,"harness_verdict_count":0,"production_writes_allowed":false}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_runtime_jobs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {};\nSELECT count(*) FROM dxr_model_turns WHERE tenant_id = {};\nSELECT count(*) FROM dxr_worker_boundaries WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_events WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_plan';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control';\nSELECT count(*) FROM dxr_workflow_events e JOIN dxr_workflow_runs r ON r.workflow_run_id = e.workflow_run_id WHERE e.tenant_id = {} AND r.tenant_id = {} AND r.workflow_type = 'dxr_dynamic_workflow_control';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' AND terminal_state = 'DXR_DYNAMIC_WORKFLOW_CONTROL_REJECTED';\nSELECT count(*) FROM dxr_ctx_context_inputs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_evidence_ledger WHERE tenant_id = {};\nSELECT count(*) FROM dxr_run_dependencies WHERE tenant_id = {};\nSELECT count(*) FROM dxr_tool_executions WHERE tenant_id = {};\nSELECT count(*) FROM dxr_harness_verdicts WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 15 => format!(
                r#"{{"name":"mdx-dxr-durable-state","status":"POSTGRES_DURABLE_DXR_RECORDED","runtime":"mdx-dxr-engine","route":"/dxr/durable-state.json","database_url_configured":true,"durable_runtime_ready":true,"durable_workflow_status":"POSTGRES_DURABLE_DXR_WORKFLOW_RECORDED","durable_dynamic_workflow_control_status":"POSTGRES_DURABLE_DXR_DYNAMIC_WORKFLOW_CONTROL_RECORDED","durable_ctx_operational_context_status":"POSTGRES_DURABLE_DXR_CTX_CONTEXT_RECORDED","durable_execution_evidence_status":"POSTGRES_DURABLE_DXR_EXECUTION_EVIDENCE_RECORDED","tables":["dxr_runtime_jobs","dxr_runtime_events","dxr_model_turns","dxr_worker_boundaries","dxr_workflow_runs","dxr_workflow_events","dxr_ctx_context_inputs","dxr_evidence_ledger","dxr_run_dependencies","dxr_tool_executions","dxr_harness_verdicts"],"job_count":{},"event_count":{},"model_turn_count":{},"worker_boundary_count":{},"workflow_run_count":{},"workflow_event_count":{},"dynamic_workflow_plan_run_count":{},"dynamic_workflow_control_run_count":{},"dynamic_workflow_control_event_count":{},"dynamic_workflow_control_rejected_count":{},"ctx_context_input_count":{},"evidence_ledger_count":{},"run_dependency_count":{},"tool_execution_count":{},"harness_verdict_count":{},"production_writes_allowed":false}}"#,
                parse_count(&rows, 0),
                parse_count(&rows, 1),
                parse_count(&rows, 2),
                parse_count(&rows, 3),
                parse_count(&rows, 4),
                parse_count(&rows, 5),
                parse_count(&rows, 6),
                parse_count(&rows, 7),
                parse_count(&rows, 8),
                parse_count(&rows, 9),
                parse_count(&rows, 10),
                parse_count(&rows, 11),
                parse_count(&rows, 12),
                parse_count(&rows, 13),
                parse_count(&rows, 14)
            ),
            Ok(_) => self.render_error_json("missing durable count rows"),
            Err(error) => self.render_error_json(&error),
        }
    }

    pub fn render_forge_execution_recovery_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-forge-execution-proof-recovery","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/forge-execution-proof-recovery.json","database_url_configured":false,"durable_replay_ready":false,"proof_run_count":0,"human_ratified_count":0,"security_rejected_count":0,"missing_evidence_count":0,"latest_proof":null,"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_REJECTED_SECURITY_BOUNDARY';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_REJECTED_MISSING_EVIDENCE';\nSELECT coalesce((SELECT concat_ws(E'\\t', coalesce(workflow_run_id, ''), coalesce(source_job_id, ''), coalesce(source_run_id, ''), coalesce(status, ''), coalesce(terminal_state, ''), coalesce(payload->>'execution_decision', ''), coalesce(payload->'proof'->>'provider_memory_integration_id', ''), coalesce(payload->'proof'->>'workflow_orchestration_id', ''), coalesce(payload->'proof'->>'authority_apply_id', '')) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' ORDER BY updated_at DESC, workflow_run_id DESC LIMIT 1), '');\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 5 => format!(
                r#"{{"name":"mdx-dxr-forge-execution-proof-recovery","status":"POSTGRES_DURABLE_DXR_FORGE_EXECUTION_PROOF_REPLAY_READY","runtime":"mdx-dxr-engine","route":"/dxr/forge-execution-proof-recovery.json","database_url_configured":true,"durable_replay_ready":true,"replay_source":"dxr_workflow_runs+dxr_workflow_events","proof_run_count":{},"human_ratified_count":{},"security_rejected_count":{},"missing_evidence_count":{},"latest_proof":{},"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
                parse_count(&rows, 0),
                parse_count(&rows, 1),
                parse_count(&rows, 2),
                parse_count(&rows, 3),
                render_latest_forge_execution_proof_json(rows.get(4))
            ),
            Ok(_) => self.render_forge_execution_recovery_error_json(
                "missing forge execution recovery rows",
            ),
            Err(error) => self.render_forge_execution_recovery_error_json(&error),
        }
    }

    pub fn render_forge_execution_supervisor_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-forge-execution-supervisor","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/forge-execution-supervisor.json","recovery_route":"/dxr/forge-execution-proof-recovery.json","database_url_configured":false,"durable_replay_ready":false,"supervisor_ready":false,"proof_run_count":0,"supervisor_action_count":0,"resume_ready_count":0,"await_ci_count":0,"await_human_ratification_count":0,"hold_apply_blocked_count":0,"close_rejected_count":0,"requeue_missing_evidence_count":0,"quarantine_security_count":0,"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"actions":[]}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_READY_AUTHORITY_BLOCKED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_CI_READY';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_REJECTED_MISSING_EVIDENCE';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_REJECTED_SECURITY_BOUNDARY';\nSELECT count(*) FROM dxr_workflow_events e JOIN dxr_workflow_runs r ON r.workflow_run_id = e.workflow_run_id WHERE e.tenant_id = {} AND r.tenant_id = {} AND r.workflow_type = 'dxr_forge_execution_proof' AND e.event_type = 'forge_execution_supervisor_action_recorded';\nSELECT count(*) FROM dxr_evidence_ledger WHERE tenant_id = {} AND event_type = 'forge_execution_supervisor_action_recorded';\nSELECT coalesce((SELECT string_agg(concat_ws(E'\\t', workflow_run_id, source_job_id, source_run_id, status, terminal_state, coalesce(payload->>'execution_decision', ''), coalesce(payload->'proof'->>'workflow_orchestration_id', ''), coalesce(payload->'proof'->>'authority_apply_id', ''), coalesce(payload->'proof'->>'retained_backpressure_count', '0')), E'\\x1e') FROM (SELECT workflow_run_id, source_job_id, source_run_id, status, terminal_state, payload, updated_at FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' ORDER BY updated_at DESC, workflow_run_id DESC LIMIT 25) recent), '');\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 10 => {
                let proof_run_count = parse_count(&rows, 0);
                let ready_count = parse_count(&rows, 1);
                let ci_ready_count = parse_count(&rows, 2);
                let human_ratified_count = parse_count(&rows, 3);
                let human_rejected_count = parse_count(&rows, 4);
                let missing_evidence_count = parse_count(&rows, 5);
                let security_rejected_count = parse_count(&rows, 6);
                let durable_supervisor_event_count = parse_count(&rows, 7);
                let durable_supervisor_evidence_count = parse_count(&rows, 8);
                let supervisor_action_count = ready_count
                    + ci_ready_count
                    + human_ratified_count
                    + human_rejected_count
                    + missing_evidence_count
                    + security_rejected_count;
                format!(
                    r#"{{"name":"mdx-dxr-forge-execution-supervisor","status":"POSTGRES_DURABLE_DXR_FORGE_EXECUTION_SUPERVISOR_READY","runtime":"mdx-dxr-engine","route":"/dxr/forge-execution-supervisor.json","recovery_route":"/dxr/forge-execution-proof-recovery.json","dispatch_recovery_route":"/dxr/dispatch/recovery-plan.json","dynamic_workflow_control_recovery_route":"/dxr/dynamic-workflow-control-recovery.json","database_url_configured":true,"durable_replay_ready":true,"supervisor_ready":true,"supervisor_mode":"local_postgres_replay_fail_closed","proof_run_count":{},"supervisor_action_count":{},"durable_supervisor_event_count":{},"durable_supervisor_evidence_count":{},"resume_ready_count":{},"await_ci_count":{},"await_human_ratification_count":{},"hold_apply_blocked_count":{},"close_rejected_count":{},"requeue_missing_evidence_count":{},"quarantine_security_count":{},"lease_duration_ms":30000,"heartbeat_interval_ms":5000,"retry_budget":3,"replay_window_seconds":86400,"workflow_dispatch_batch_size":320,"provider_stream_slots":512,"sandbox_pool_size":2048,"target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"scale_backpressure_policy":"tenant_weighted_fair_queue_with_retained_backpressure","idempotency_required":true,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"durable_replay_before_resume_required":true,"human_ratification_before_apply_required":true,"security_rejection_quarantine_required":true,"durable_supervisor_event_required":true,"durable_supervisor_evidence_required":true,"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"actions":[{}]}}"#,
                    proof_run_count,
                    supervisor_action_count,
                    durable_supervisor_event_count,
                    durable_supervisor_evidence_count,
                    ready_count,
                    ready_count,
                    ci_ready_count,
                    human_ratified_count,
                    human_rejected_count,
                    missing_evidence_count,
                    security_rejected_count,
                    render_forge_execution_supervisor_actions_json(rows.get(9))
                )
            }
            Ok(_) => self.render_forge_execution_supervisor_error_json(
                "missing forge execution supervisor rows",
            ),
            Err(error) => self.render_forge_execution_supervisor_error_json(&error),
        }
    }

    pub fn render_dispatch_worker_sandbox_handoff_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-dispatch-worker-sandbox-handoff","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/dispatch-worker-sandbox-handoff.json","database_url_configured":false,"durable_handoff_ready":false,"dispatch_lease_event_count":0,"dispatch_heartbeat_event_count":0,"dispatch_recovery_event_count":0,"worker_driver_event_count":0,"worker_driver_blocked_event_count":0,"sandbox_handoff_event_count":0,"sandbox_execution_blocked_event_count":0,"sandbox_session_ready_event_count":0,"sandbox_command_preflight_event_count":0,"sandbox_command_result_event_count":0,"sandbox_result_consumption_event_count":0,"worker_boundary_count":0,"durable_workflow_run_count":0,"durable_workflow_event_count":0,"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type IN ('dispatch_run_claimed','dispatch_claim_conflict','dispatch_claim_expired','dispatch_claim_released');\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'dispatch_heartbeat_renewed';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'dispatch_recovery_plan_recorded';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'worker_driver_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'worker_driver_authority_blocked';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'supervised_sandbox_handoff_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'supervised_sandbox_execution_blocked';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'sandbox_session_ready';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'sandbox_command_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'sandbox_command_result_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'sandbox_result_%';\nSELECT count(*) FROM dxr_worker_boundaries WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_events WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 14 => format!(
                r#"{{"name":"mdx-dxr-dispatch-worker-sandbox-handoff","status":"POSTGRES_DURABLE_DXR_DISPATCH_WORKER_SANDBOX_HANDOFF_READY","runtime":"mdx-dxr-engine","route":"/dxr/dispatch-worker-sandbox-handoff.json","database_url_configured":true,"durable_handoff_ready":true,"durable_replay_ready":true,"handoff_policy":"lease_backed_worker_sandbox_handoff_fail_closed_before_execution","dispatch_route":"/dxr/dispatch/recovery-plan.json","worker_driver_route":"/dxr/worker-driver-runs.json","sandbox_handoff_route":"/dxr/supervised-sandbox-handoffs.json","sandbox_session_route":"/dxr/sandbox-sessions.json","sandbox_command_route":"/dxr/sandbox-command-preflights.json","sandbox_result_consumption_route":"/dxr/sandbox-result-consumptions.json","dispatch_lease_event_count":{},"dispatch_heartbeat_event_count":{},"dispatch_recovery_event_count":{},"worker_driver_event_count":{},"worker_driver_blocked_event_count":{},"sandbox_handoff_event_count":{},"sandbox_execution_blocked_event_count":{},"sandbox_session_ready_event_count":{},"sandbox_command_preflight_event_count":{},"sandbox_command_result_event_count":{},"sandbox_result_consumption_event_count":{},"worker_boundary_count":{},"durable_workflow_run_count":{},"durable_workflow_event_count":{},"lease_duration_ms":30000,"heartbeat_interval_ms":5000,"hot_path_budget_ms":20,"target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"sandbox_pool_size":2048,"workflow_dispatch_batch_size":320,"provider_stream_slots":512,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"dispatch_recovery_required":true,"worker_driver_required":true,"sandbox_handoff_required":true,"sandbox_session_required":true,"sandbox_command_preflight_required":true,"sandbox_result_consumption_required":true,"durable_workflow_required":true,"durable_event_replay_required":true,"tenant_fairness_required":true,"backpressure_required":true,"kill_switch_required":true,"human_ratification_before_apply_required":true,"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
                parse_count(&rows, 0),
                parse_count(&rows, 1),
                parse_count(&rows, 2),
                parse_count(&rows, 3),
                parse_count(&rows, 4),
                parse_count(&rows, 5),
                parse_count(&rows, 6),
                parse_count(&rows, 7),
                parse_count(&rows, 8),
                parse_count(&rows, 9),
                parse_count(&rows, 10),
                parse_count(&rows, 11),
                parse_count(&rows, 12),
                parse_count(&rows, 13)
            ),
            Ok(_) => {
                self.render_dispatch_worker_sandbox_handoff_error_json("missing handoff count rows")
            }
            Err(error) => self.render_dispatch_worker_sandbox_handoff_error_json(&error),
        }
    }

    pub fn render_live_execution_sandbox_readiness_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-live-execution-sandbox-readiness","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/live-execution-sandbox-readiness.json","database_url_configured":false,"durable_readiness_ready":false,"durable_replay_ready":false,"dispatch_lease_event_count":0,"dispatch_heartbeat_event_count":0,"dispatch_recovery_event_count":0,"worker_driver_event_count":0,"worker_driver_blocked_event_count":0,"sandbox_handoff_event_count":0,"sandbox_execution_blocked_event_count":0,"sandbox_runner_ready_event_count":0,"sandbox_runner_blocked_event_count":0,"sandbox_session_ready_event_count":0,"sandbox_session_blocked_event_count":0,"sandbox_command_preflight_event_count":0,"sandbox_command_result_event_count":0,"sandbox_result_consumption_event_count":0,"authority_blocked_event_count":0,"worker_boundary_count":0,"durable_workflow_run_count":0,"durable_workflow_event_count":0,"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type IN ('dispatch_run_claimed','dispatch_claim_conflict','dispatch_claim_expired','dispatch_claim_released');\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'dispatch_heartbeat_renewed';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'dispatch_recovery_plan_recorded';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'worker_driver_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'worker_driver_authority_blocked';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'supervised_sandbox_handoff_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'supervised_sandbox_execution_blocked';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'supervised_sandbox_runner_ready';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'supervised_sandbox_runner_execution_blocked';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'sandbox_session_ready';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'sandbox_session_execution_blocked';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'sandbox_command_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'sandbox_command_result_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'sandbox_result_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND (event_type LIKE '%authority_blocked' OR event_type LIKE '%execution_blocked');\nSELECT count(*) FROM dxr_worker_boundaries WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_events WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 18 => format!(
                r#"{{"name":"mdx-dxr-live-execution-sandbox-readiness","status":"POSTGRES_DURABLE_DXR_LIVE_EXECUTION_SANDBOX_READINESS_READY","runtime":"mdx-dxr-engine","route":"/dxr/live-execution-sandbox-readiness.json","database_url_configured":true,"durable_readiness_ready":true,"durable_replay_ready":true,"readiness_policy":"live_execution_sandbox_handoff_fail_closed_before_process_start","dispatch_handoff_route":"/dxr/dispatch-worker-sandbox-handoff.json","worker_driver_route":"/dxr/worker-driver-runs.json","sandbox_handoff_route":"/dxr/supervised-sandbox-handoffs.json","sandbox_runner_route":"/dxr/supervised-sandbox-runners.json","sandbox_session_route":"/dxr/sandbox-sessions.json","sandbox_command_preflight_route":"/dxr/sandbox-command-preflights.json","sandbox_command_result_route":"/dxr/sandbox-command-results.json","sandbox_result_consumption_route":"/dxr/sandbox-result-consumptions.json","workflow_orchestration_route":"/dxr/workflow-orchestrations.json","dispatch_lease_event_count":{},"dispatch_heartbeat_event_count":{},"dispatch_recovery_event_count":{},"worker_driver_event_count":{},"worker_driver_blocked_event_count":{},"sandbox_handoff_event_count":{},"sandbox_execution_blocked_event_count":{},"sandbox_runner_ready_event_count":{},"sandbox_runner_blocked_event_count":{},"sandbox_session_ready_event_count":{},"sandbox_session_blocked_event_count":{},"sandbox_command_preflight_event_count":{},"sandbox_command_result_event_count":{},"sandbox_result_consumption_event_count":{},"authority_blocked_event_count":{},"worker_boundary_count":{},"durable_workflow_run_count":{},"durable_workflow_event_count":{},"lease_duration_ms":30000,"heartbeat_interval_ms":5000,"hot_path_budget_ms":20,"ready_p95_ms_ceiling":2500,"max_runtime_ms":300000,"target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"sandbox_pool_size":2048,"workflow_dispatch_batch_size":320,"provider_stream_slots":512,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"dispatch_recovery_required":true,"worker_driver_required":true,"supervised_sandbox_handoff_required":true,"supervised_sandbox_runner_required":true,"sandbox_session_required":true,"sandbox_command_preflight_required":true,"sandbox_command_result_required":true,"sandbox_result_consumption_required":true,"worker_boundary_required":true,"durable_workflow_required":true,"durable_event_replay_required":true,"tenant_fairness_required":true,"backpressure_required":true,"kill_switch_required":true,"quarantine_required":true,"artifact_sink_required":true,"log_stream_required":true,"human_ratification_before_apply_required":true,"provider_turn_on_before_live_worker_execution_required":true,"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"sandbox_command_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
                parse_count(&rows, 0),
                parse_count(&rows, 1),
                parse_count(&rows, 2),
                parse_count(&rows, 3),
                parse_count(&rows, 4),
                parse_count(&rows, 5),
                parse_count(&rows, 6),
                parse_count(&rows, 7),
                parse_count(&rows, 8),
                parse_count(&rows, 9),
                parse_count(&rows, 10),
                parse_count(&rows, 11),
                parse_count(&rows, 12),
                parse_count(&rows, 13),
                parse_count(&rows, 14),
                parse_count(&rows, 15),
                parse_count(&rows, 16),
                parse_count(&rows, 17)
            ),
            Ok(_) => self
                .render_live_execution_sandbox_readiness_error_json("missing readiness count rows"),
            Err(error) => self.render_live_execution_sandbox_readiness_error_json(&error),
        }
    }

    pub fn render_ctx_dxr_forge_interface_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-ctx-forge-interface","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/ctx-dxr-forge-interface.json","database_url_configured":false,"durable_ctx_forge_interface_ready":false,"ctx_context_input_count":0,"ctx_context_durable_event_count":0,"ctx_context_runtime_event_count":0,"ctx_retrieval_dependency_event_count":0,"provider_memory_event_count":0,"provider_memory_dependency_event_count":0,"workflow_ctx_bound_event_count":0,"workflow_provider_memory_bound_event_count":0,"forge_provider_memory_bound_event_count":0,"forge_workflow_bound_event_count":0,"forge_execution_proof_run_count":0,"durable_supervisor_event_count":0,"durable_workflow_run_count":0,"durable_workflow_event_count":0,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_ctx_context_inputs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'ctx_operational_context_durable_recorded';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type IN ('ctx_operational_context_recorded','ctx_context_bound_to_run','ctx_world_model_retrieval_dependency_recorded','ctx_world_model_source_mix_bound');\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'ctx_world_model_retrieval_dependency_recorded';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'provider_memory_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type IN ('ctx_memory_admission_dependency_bound','ctx_retrieval_dependency_bound','local_driver_conformance_bound','provider_memory_turn_on_boundary_retained');\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'workflow_orchestration_ctx_bound';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'workflow_orchestration_provider_memory_bound';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'forge_execution_provider_memory_bound';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'forge_execution_workflow_orchestration_bound';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof';\nSELECT count(*) FROM dxr_workflow_events e JOIN dxr_workflow_runs r ON r.workflow_run_id = e.workflow_run_id WHERE e.tenant_id = {} AND r.tenant_id = {} AND r.workflow_type = 'dxr_forge_execution_proof' AND e.event_type = 'forge_execution_supervisor_action_recorded';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_events WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 14 => format!(
                r#"{{"name":"mdx-dxr-ctx-forge-interface","status":"POSTGRES_DURABLE_DXR_CTX_FORGE_INTERFACE_READY","runtime":"mdx-dxr-engine","route":"/dxr/ctx-dxr-forge-interface.json","database_url_configured":true,"durable_ctx_forge_interface_ready":true,"durable_replay_ready":true,"interface_policy":"ctx_world_model_to_dxr_provider_memory_to_forge_execution_fail_closed","ctx_route":"/dxr/ctx-context-inputs.json","provider_memory_route":"/dxr/provider-memory-integrations.json","workflow_orchestration_route":"/dxr/workflow-orchestrations.json","forge_execution_route":"/dxr/forge-execution-proofs.json","forge_supervisor_route":"/dxr/forge-execution-supervisor.json","ctx_context_input_count":{},"ctx_context_durable_event_count":{},"ctx_context_runtime_event_count":{},"ctx_retrieval_dependency_event_count":{},"provider_memory_event_count":{},"provider_memory_dependency_event_count":{},"workflow_ctx_bound_event_count":{},"workflow_provider_memory_bound_event_count":{},"forge_provider_memory_bound_event_count":{},"forge_workflow_bound_event_count":{},"forge_execution_proof_run_count":{},"durable_supervisor_event_count":{},"durable_workflow_run_count":{},"durable_workflow_event_count":{},"ctx_world_model_retrieval_required":true,"ctx_memory_admission_required":true,"provider_memory_integration_required":true,"workflow_orchestration_required":true,"forge_execution_required":true,"durable_replay_required":true,"context_hash_required":true,"source_receipts_required":true,"local_memory_store_required":true,"provider_secret_safe_required":true,"human_ratification_before_apply_required":true,"tenant_fairness_required":true,"backpressure_required":true,"target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"workflow_dispatch_batch_size":320,"provider_stream_slots":512,"sandbox_pool_size":2048,"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"credential_values_recorded":false,"provider_secret_values_recorded":false,"embedding_values_recorded":false,"output_text_recorded":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"worker_spawn_allowed":false,"sandbox_start_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
                parse_count(&rows, 0),
                parse_count(&rows, 1),
                parse_count(&rows, 2),
                parse_count(&rows, 3),
                parse_count(&rows, 4),
                parse_count(&rows, 5),
                parse_count(&rows, 6),
                parse_count(&rows, 7),
                parse_count(&rows, 8),
                parse_count(&rows, 9),
                parse_count(&rows, 10),
                parse_count(&rows, 11),
                parse_count(&rows, 12),
                parse_count(&rows, 13)
            ),
            Ok(_) => self.render_ctx_dxr_forge_interface_error_json(
                "missing CTX DXR Forge interface count rows",
            ),
            Err(error) => self.render_ctx_dxr_forge_interface_error_json(&error),
        }
    }

    pub fn render_forge_local_execution_rehearsal_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-forge-local-execution-rehearsal","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/forge-local-execution-rehearsal.json","database_url_configured":false,"durable_rehearsal_ready":false,"ctx_context_input_count":0,"provider_memory_dependency_event_count":0,"workflow_orchestration_event_count":0,"authority_apply_event_count":0,"authority_apply_ci_event_count":0,"authority_apply_patch_event_count":0,"authority_apply_human_event_count":0,"authority_apply_guarded_event_count":0,"authority_apply_blocked_event_count":0,"forge_execution_proof_run_count":0,"forge_execution_ready_count":0,"forge_execution_ci_ready_count":0,"forge_execution_human_ratified_count":0,"forge_execution_human_rejected_count":0,"forge_execution_missing_evidence_count":0,"forge_execution_security_rejected_count":0,"forge_execution_authority_apply_event_count":0,"forge_execution_ci_event_count":0,"forge_execution_human_event_count":0,"forge_execution_blocked_event_count":0,"durable_supervisor_event_count":0,"dispatch_handoff_event_count":0,"sandbox_result_consumption_event_count":0,"durable_workflow_run_count":0,"durable_workflow_event_count":0,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_ctx_context_inputs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type IN ('ctx_memory_admission_dependency_bound','ctx_retrieval_dependency_bound','local_driver_conformance_bound','provider_memory_turn_on_boundary_retained');\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'workflow_orchestration_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'authority_apply_%';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'authority_apply_ci_evidence_consumed';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'authority_apply_patch_proposal_mediated';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'authority_apply_human_ratification_bound';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'authority_apply_guarded_apply_preflight_recorded';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'authority_apply_execution_authority_blocked';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_READY_AUTHORITY_BLOCKED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_CI_READY';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_REJECTED_MISSING_EVIDENCE';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_forge_execution_proof' AND terminal_state = 'DXR_FORGE_EXECUTION_PROOF_REJECTED_SECURITY_BOUNDARY';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'forge_execution_authority_apply_bound';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'forge_execution_ci_evidence_bound';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'forge_execution_human_ratification_bound';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type = 'forge_execution_authority_blocked';\nSELECT count(*) FROM dxr_workflow_events e JOIN dxr_workflow_runs r ON r.workflow_run_id = e.workflow_run_id WHERE e.tenant_id = {} AND r.tenant_id = {} AND r.workflow_type = 'dxr_forge_execution_proof' AND e.event_type = 'forge_execution_supervisor_action_recorded';\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type IN ('dispatch_run_claimed','dispatch_heartbeat_renewed','dispatch_recovery_plan_recorded','worker_driver_authority_blocked','supervised_sandbox_execution_blocked','sandbox_session_ready','sandbox_command_preflight_recorded','sandbox_command_result_recorded');\nSELECT count(*) FROM dxr_runtime_events WHERE tenant_id = {} AND event_type LIKE 'sandbox_result_%';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {};\nSELECT count(*) FROM dxr_workflow_events WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 25 => format!(
                r#"{{"name":"mdx-dxr-forge-local-execution-rehearsal","status":"POSTGRES_DURABLE_DXR_FORGE_LOCAL_EXECUTION_REHEARSAL_READY","runtime":"mdx-dxr-engine","route":"/dxr/forge-local-execution-rehearsal.json","database_url_configured":true,"durable_rehearsal_ready":true,"durable_replay_ready":true,"rehearsal_policy":"forge_request_to_ci_human_ratification_blocked_apply_with_durable_replay","ctx_interface_route":"/dxr/ctx-dxr-forge-interface.json","dispatch_handoff_route":"/dxr/dispatch-worker-sandbox-handoff.json","supervisor_route":"/dxr/forge-execution-supervisor.json","authority_apply_route":"/dxr/authority-apply.json","forge_execution_route":"/dxr/forge-execution-proofs.json","ctx_context_input_count":{},"provider_memory_dependency_event_count":{},"workflow_orchestration_event_count":{},"authority_apply_event_count":{},"authority_apply_ci_event_count":{},"authority_apply_patch_event_count":{},"authority_apply_human_event_count":{},"authority_apply_guarded_event_count":{},"authority_apply_blocked_event_count":{},"forge_execution_proof_run_count":{},"forge_execution_ready_count":{},"forge_execution_ci_ready_count":{},"forge_execution_human_ratified_count":{},"forge_execution_human_rejected_count":{},"forge_execution_missing_evidence_count":{},"forge_execution_security_rejected_count":{},"forge_execution_authority_apply_event_count":{},"forge_execution_ci_event_count":{},"forge_execution_human_event_count":{},"forge_execution_blocked_event_count":{},"durable_supervisor_event_count":{},"dispatch_handoff_event_count":{},"sandbox_result_consumption_event_count":{},"durable_workflow_run_count":{},"durable_workflow_event_count":{},"forge_request_required":true,"ctx_context_required":true,"provider_memory_required":true,"workflow_orchestration_required":true,"authority_apply_required":true,"ci_evidence_required":true,"human_ratification_required":true,"guarded_apply_preflight_required":true,"durable_supervisor_required":true,"dispatch_worker_sandbox_handoff_required":true,"durable_replay_required":true,"human_ratification_before_apply_required":true,"blocked_apply_required":true,"security_rejection_required":true,"tenant_fairness_required":true,"backpressure_required":true,"target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"workflow_dispatch_batch_size":320,"provider_stream_slots":512,"sandbox_pool_size":2048,"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"credential_values_recorded":false,"provider_secret_values_recorded":false,"embedding_values_recorded":false,"output_text_recorded":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"patch_applied":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
                parse_count(&rows, 0),
                parse_count(&rows, 1),
                parse_count(&rows, 2),
                parse_count(&rows, 3),
                parse_count(&rows, 4),
                parse_count(&rows, 5),
                parse_count(&rows, 6),
                parse_count(&rows, 7),
                parse_count(&rows, 8),
                parse_count(&rows, 9),
                parse_count(&rows, 10),
                parse_count(&rows, 11),
                parse_count(&rows, 12),
                parse_count(&rows, 13),
                parse_count(&rows, 14),
                parse_count(&rows, 15),
                parse_count(&rows, 16),
                parse_count(&rows, 17),
                parse_count(&rows, 18),
                parse_count(&rows, 19),
                parse_count(&rows, 20),
                parse_count(&rows, 21),
                parse_count(&rows, 22),
                parse_count(&rows, 23),
                parse_count(&rows, 24)
            ),
            Ok(_) => self.render_forge_local_execution_rehearsal_error_json(
                "missing Forge local execution rehearsal count rows",
            ),
            Err(error) => self.render_forge_local_execution_rehearsal_error_json(&error),
        }
    }

    pub fn render_dynamic_workflow_control_recovery_json(&self, tenant_id: &str) -> String {
        let Some(database_url) = &self.database_url else {
            return r#"{"name":"mdx-dxr-dynamic-workflow-control-recovery","status":"LOCAL_PROCESS_DXR_ONLY","runtime":"mdx-dxr-engine","route":"/dxr/dynamic-workflow-control-recovery.json","database_url_configured":false,"durable_replay_ready":false,"control_run_count":0,"accepted_control_run_count":0,"rejected_control_run_count":0,"pause_count":0,"resume_count":0,"stop_count":0,"restart_phase_count":0,"control_event_count":0,"checkpoint_policy":"control_checkpoint_resume_cache_replay","resume_cache_replay_required":true,"production_writes_allowed":false}"#.to_string();
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' AND status = 'LIVE-LOCAL-DXR-DYNAMIC-WORKFLOW-RUN-CONTROL';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' AND terminal_state = 'DXR_DYNAMIC_WORKFLOW_CONTROL_REJECTED';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' AND payload->>'control_action' = 'pause';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' AND payload->>'control_action' = 'resume';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' AND payload->>'control_action' = 'stop';\nSELECT count(*) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' AND payload->>'control_action' = 'restart_phase';\nSELECT count(*) FROM dxr_workflow_events e JOIN dxr_workflow_runs r ON r.workflow_run_id = e.workflow_run_id WHERE e.tenant_id = {} AND r.tenant_id = {} AND r.workflow_type = 'dxr_dynamic_workflow_control';\nSELECT coalesce((SELECT concat_ws(E'\\t', coalesce(workflow_run_id, ''), coalesce(source_job_id, ''), coalesce(source_run_id, ''), coalesce(terminal_state, ''), coalesce(payload->>'control_action', ''), coalesce(payload->>'resume_cache_key', ''), coalesce(payload->>'idempotency_key', ''), coalesce(status, '')) FROM dxr_workflow_runs WHERE tenant_id = {} AND workflow_type = 'dxr_dynamic_workflow_control' ORDER BY updated_at DESC, workflow_run_id DESC LIMIT 1), '');\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id),
            sql_string_literal(tenant_id)
        );
        match run_psql_rows(database_url, &sql) {
            Ok(rows) if rows.len() >= 9 => {
                let latest = render_latest_dynamic_control_json(rows.get(8));
                format!(
                    r#"{{"name":"mdx-dxr-dynamic-workflow-control-recovery","status":"POSTGRES_DURABLE_DXR_DYNAMIC_WORKFLOW_CONTROL_REPLAY_READY","runtime":"mdx-dxr-engine","route":"/dxr/dynamic-workflow-control-recovery.json","database_url_configured":true,"durable_replay_ready":true,"replay_source":"dxr_workflow_runs+dxr_workflow_events","control_run_count":{},"accepted_control_run_count":{},"rejected_control_run_count":{},"pause_count":{},"resume_count":{},"stop_count":{},"restart_phase_count":{},"control_event_count":{},"latest_control":{},"checkpoint_policy":"control_checkpoint_resume_cache_replay","resume_cache_replay_required":true,"idempotency_key_required":true,"phase_restart_from_cache_only":true,"worker_execution_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
                    parse_count(&rows, 0),
                    parse_count(&rows, 1),
                    parse_count(&rows, 2),
                    parse_count(&rows, 3),
                    parse_count(&rows, 4),
                    parse_count(&rows, 5),
                    parse_count(&rows, 6),
                    parse_count(&rows, 7),
                    latest
                )
            }
            Ok(_) => self.render_dynamic_control_recovery_error_json("missing recovery count rows"),
            Err(error) => self.render_dynamic_control_recovery_error_json(&error),
        }
    }

    fn run_count(&self, sql: &str) {
        let Some(database_url) = &self.database_url else {
            return;
        };
        let _ = run_psql_count(database_url, sql);
    }

    fn render_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-durable-state","status":"POSTGRES_DURABLE_DXR_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/durable-state.json","database_url_configured":true,"durable_runtime_ready":false,"error":{},"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }

    fn render_dynamic_control_recovery_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-dynamic-workflow-control-recovery","status":"POSTGRES_DURABLE_DXR_DYNAMIC_WORKFLOW_CONTROL_REPLAY_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/dynamic-workflow-control-recovery.json","database_url_configured":true,"durable_replay_ready":false,"error":{},"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }

    fn render_forge_execution_recovery_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-forge-execution-proof-recovery","status":"POSTGRES_DURABLE_DXR_FORGE_EXECUTION_PROOF_REPLAY_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/forge-execution-proof-recovery.json","database_url_configured":true,"durable_replay_ready":false,"error":{},"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }

    fn render_forge_execution_supervisor_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-forge-execution-supervisor","status":"POSTGRES_DURABLE_DXR_FORGE_EXECUTION_SUPERVISOR_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/forge-execution-supervisor.json","database_url_configured":true,"durable_replay_ready":false,"supervisor_ready":false,"error":{},"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }

    fn render_dispatch_worker_sandbox_handoff_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-dispatch-worker-sandbox-handoff","status":"POSTGRES_DURABLE_DXR_DISPATCH_WORKER_SANDBOX_HANDOFF_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/dispatch-worker-sandbox-handoff.json","database_url_configured":true,"durable_handoff_ready":false,"error":{},"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }

    fn render_live_execution_sandbox_readiness_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-live-execution-sandbox-readiness","status":"POSTGRES_DURABLE_DXR_LIVE_EXECUTION_SANDBOX_READINESS_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/live-execution-sandbox-readiness.json","database_url_configured":true,"durable_readiness_ready":false,"durable_replay_ready":false,"error":{},"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }

    fn render_ctx_dxr_forge_interface_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-ctx-forge-interface","status":"POSTGRES_DURABLE_DXR_CTX_FORGE_INTERFACE_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/ctx-dxr-forge-interface.json","database_url_configured":true,"durable_ctx_forge_interface_ready":false,"error":{},"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }

    fn render_forge_local_execution_rehearsal_error_json(&self, error: &str) -> String {
        format!(
            r#"{{"name":"mdx-dxr-forge-local-execution-rehearsal","status":"POSTGRES_DURABLE_DXR_FORGE_LOCAL_EXECUTION_REHEARSAL_ERROR","runtime":"mdx-dxr-engine","route":"/dxr/forge-local-execution-rehearsal.json","database_url_configured":true,"durable_rehearsal_ready":false,"error":{},"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(error)
        )
    }
}

fn render_latest_dynamic_control_json(row: Option<&String>) -> String {
    let parts = row
        .map(|value| value.split('\t').collect::<Vec<_>>())
        .unwrap_or_default();
    if parts.len() < 8 {
        return "null".to_string();
    }
    format!(
        r#"{{"control_id":{},"workflow_plan_id":{},"phase_id":{},"terminal_state":{},"control_action":{},"resume_cache_key":{},"idempotency_key":{},"status":{}}}"#,
        json_string_literal(parts[0]),
        json_string_literal(parts[1]),
        json_string_literal(parts[2]),
        json_string_literal(parts[3]),
        json_string_literal(parts[4]),
        json_string_literal(parts[5]),
        json_string_literal(parts[6]),
        json_string_literal(parts[7])
    )
}

fn render_latest_forge_execution_proof_json(row: Option<&String>) -> String {
    let parts = row
        .map(|value| value.split('\t').collect::<Vec<_>>())
        .unwrap_or_default();
    if parts.len() < 9 {
        return "null".to_string();
    }
    format!(
        r#"{{"proof_id":{},"job_id":{},"run_id":{},"status":{},"terminal_state":{},"execution_decision":{},"provider_memory_integration_id":{},"workflow_orchestration_id":{},"authority_apply_id":{}}}"#,
        json_string_literal(parts[0]),
        json_string_literal(parts[1]),
        json_string_literal(parts[2]),
        json_string_literal(parts[3]),
        json_string_literal(parts[4]),
        json_string_literal(parts[5]),
        json_string_literal(parts[6]),
        json_string_literal(parts[7]),
        json_string_literal(parts[8])
    )
}

fn render_forge_execution_supervisor_actions_json(row: Option<&String>) -> String {
    row.map(|value| {
        value
            .split('\u{1e}')
            .filter(|entry| !entry.trim().is_empty())
            .enumerate()
            .map(|(index, entry)| render_forge_execution_supervisor_action_json(index + 1, entry))
            .collect::<Vec<_>>()
            .join(",")
    })
    .unwrap_or_default()
}

fn render_forge_execution_supervisor_action_json(sequence: usize, row: &str) -> String {
    let parts = row.split('\t').collect::<Vec<_>>();
    let proof_id = parts.first().copied().unwrap_or_default();
    let job_id = parts.get(1).copied().unwrap_or_default();
    let run_id = parts.get(2).copied().unwrap_or_default();
    let status = parts.get(3).copied().unwrap_or_default();
    let terminal_state = parts.get(4).copied().unwrap_or_default();
    let execution_decision = parts.get(5).copied().unwrap_or_default();
    let workflow_orchestration_id = parts.get(6).copied().unwrap_or_default();
    let authority_apply_id = parts.get(7).copied().unwrap_or_default();
    let retained_backpressure_count = parts
        .get(8)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let decision = forge_supervisor_decision(terminal_state);
    format!(
        r#"{{"sequence":{},"proof_id":{},"job_id":{},"run_id":{},"status":{},"terminal_state":{},"execution_decision":{},"workflow_orchestration_id":{},"authority_apply_id":{},"supervisor_action":{},"recovery_action":{},"action_terminal_state":{},"reason":{},"idempotency_key":{},"lease_checked":true,"heartbeat_checked":true,"retry_budget_checked":true,"durable_replay_checked":true,"dispatch_recovery_bound":true,"dynamic_workflow_control_recovery_bound":true,"retained_backpressure_count":{},"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        sequence,
        json_string_literal(proof_id),
        json_string_literal(job_id),
        json_string_literal(run_id),
        json_string_literal(status),
        json_string_literal(terminal_state),
        json_string_literal(execution_decision),
        json_string_literal(workflow_orchestration_id),
        json_string_literal(authority_apply_id),
        json_string_literal(decision.supervisor_action),
        json_string_literal(decision.recovery_action),
        json_string_literal(decision.action_terminal_state),
        json_string_literal(decision.reason),
        json_string_literal(&format!(
            "dxr_forge_execution_supervisor_{}",
            stable_key(&format!("{}-{}", proof_id, decision.supervisor_action))
        )),
        retained_backpressure_count
    )
}

struct ForgeSupervisorDecision {
    supervisor_action: &'static str,
    recovery_action: &'static str,
    action_terminal_state: &'static str,
    reason: &'static str,
}

fn forge_supervisor_decision(terminal_state: &str) -> ForgeSupervisorDecision {
    match terminal_state {
        "DXR_FORGE_EXECUTION_PROOF_RECORDED_READY_AUTHORITY_BLOCKED" => ForgeSupervisorDecision {
            supervisor_action: "resume_to_ci_evidence_gate",
            recovery_action: "claim_ready_run_after_durable_replay",
            action_terminal_state: "DXR_FORGE_EXECUTION_SUPERVISOR_RESUME_READY",
            reason: "forge_execution_ready_but_ci_and_human_ratification_not_bound",
        },
        "DXR_FORGE_EXECUTION_PROOF_RECORDED_CI_READY" => ForgeSupervisorDecision {
            supervisor_action: "await_human_ratification",
            recovery_action: "hold_at_human_decision_gate",
            action_terminal_state: "DXR_FORGE_EXECUTION_SUPERVISOR_AWAITING_HUMAN",
            reason: "ci_evidence_ready_but_human_ratification_required_before_apply",
        },
        "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED" => {
            ForgeSupervisorDecision {
                supervisor_action: "hold_apply_authority_blocked",
                recovery_action: "prepare_guarded_apply_preflight_after_human",
                action_terminal_state: "DXR_FORGE_EXECUTION_SUPERVISOR_APPLY_HELD_BLOCKED",
                reason: "human_ratified_but_mutation_authority_remains_closed",
            }
        }
        "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED" => ForgeSupervisorDecision {
            supervisor_action: "close_rejected_execution",
            recovery_action: "record_closed_terminal_rejection",
            action_terminal_state: "DXR_FORGE_EXECUTION_SUPERVISOR_CLOSED_REJECTED",
            reason: "human_rejected_the_execution_so_no_resume_or_apply_is_allowed",
        },
        "DXR_FORGE_EXECUTION_PROOF_REJECTED_MISSING_EVIDENCE" => ForgeSupervisorDecision {
            supervisor_action: "requeue_evidence_repair",
            recovery_action: "requeue_missing_evidence_without_worker_start",
            action_terminal_state: "DXR_FORGE_EXECUTION_SUPERVISOR_REQUEUE_EVIDENCE",
            reason: "required_forge_or_dxr_evidence_missing",
        },
        "DXR_FORGE_EXECUTION_PROOF_REJECTED_SECURITY_BOUNDARY" => ForgeSupervisorDecision {
            supervisor_action: "quarantine_security_rejection",
            recovery_action: "stop_and_quarantine_without_retry",
            action_terminal_state: "DXR_FORGE_EXECUTION_SUPERVISOR_SECURITY_QUARANTINED",
            reason: "request_attempted_forbidden_runtime_authority",
        },
        _ => ForgeSupervisorDecision {
            supervisor_action: "inspect_unknown_terminal_state",
            recovery_action: "manual_recovery_inspection_required",
            action_terminal_state: "DXR_FORGE_EXECUTION_SUPERVISOR_INSPECTION_REQUIRED",
            reason: "unknown_terminal_state_requires_manual_review",
        },
    }
}

fn forge_supervisor_event_insert_sql(proof: &DxrForgeExecutionDurableRecord) -> String {
    let decision = forge_supervisor_decision(&proof.terminal_state);
    format!(
        "INSERT INTO dxr_workflow_events (workflow_event_id, tenant_id, workflow_run_id, sequence, event_type, terminal_state, payload) VALUES ({}, {}, {}, 2, {}, {}, {}::jsonb) ON CONFLICT (workflow_event_id) DO UPDATE SET terminal_state = EXCLUDED.terminal_state, payload = EXCLUDED.payload;",
        sql_string_literal(&format!("{}_supervisor_action", proof.proof_id)),
        sql_string_literal(&proof.tenant_id),
        sql_string_literal(&proof.proof_id),
        sql_string_literal("forge_execution_supervisor_action_recorded"),
        sql_string_literal(decision.action_terminal_state),
        sql_string_literal(&forge_supervisor_payload_json(proof, &decision))
    )
}

fn forge_supervisor_evidence_insert_sql(proof: &DxrForgeExecutionDurableRecord) -> String {
    let decision = forge_supervisor_decision(&proof.terminal_state);
    let evidence_id = format!("{}_supervisor_evidence", proof.proof_id);
    format!(
        "INSERT INTO dxr_evidence_ledger (evidence_record_id, tenant_id, sequence_number, previous_hash, record_hash, event_type, job_id, run_id, actor_id, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (evidence_record_id) DO UPDATE SET previous_hash = EXCLUDED.previous_hash, record_hash = EXCLUDED.record_hash, payload = EXCLUDED.payload;",
        sql_string_literal(&evidence_id),
        sql_string_literal(&proof.tenant_id),
        900_000 + trailing_number(&proof.proof_id),
        sql_string_literal(&proof.proof_id),
        sql_string_literal(&format!("{}_supervisor_hash", proof.proof_id)),
        sql_string_literal("forge_execution_supervisor_action_recorded"),
        sql_string_literal(&proof.job_id),
        sql_string_literal(&proof.run_id),
        sql_string_literal(&proof.actor_id),
        sql_string_literal(&forge_supervisor_payload_json(proof, &decision))
    )
}

fn forge_supervisor_payload_json(
    proof: &DxrForgeExecutionDurableRecord,
    decision: &ForgeSupervisorDecision,
) -> String {
    format!(
        r#"{{"proof_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"execution_decision":{},"terminal_state":{},"supervisor_action":{},"recovery_action":{},"action_terminal_state":{},"reason":{},"lease_duration_ms":30000,"heartbeat_interval_ms":5000,"retry_budget":3,"replay_window_seconds":86400,"workflow_dispatch_batch_size":320,"provider_stream_slots":512,"sandbox_pool_size":2048,"target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"durable_replay_checked":true,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"human_ratification_before_apply_required":true,"security_rejection_quarantine_required":true,"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"patch_application_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.job_id),
        json_string_literal(&proof.run_id),
        json_string_literal(&proof.idempotency_key),
        json_string_literal(&proof.execution_decision),
        json_string_literal(&proof.terminal_state),
        json_string_literal(decision.supervisor_action),
        json_string_literal(decision.recovery_action),
        json_string_literal(decision.action_terminal_state),
        json_string_literal(decision.reason)
    )
}

fn trailing_number(value: &str) -> usize {
    value
        .rsplit('_')
        .next()
        .and_then(|part| part.parse::<usize>().ok())
        .unwrap_or(0)
}

fn stable_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn json_array_literal(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string_literal(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn runtime_evidence_insert_sql(
    tenant_id: &str,
    actor_id: &str,
    packet: &DxrLocalRuntimePacket,
) -> String {
    let mut statements = Vec::new();
    for dependency in &packet.dependencies {
        statements.push(format!(
            "INSERT INTO dxr_run_dependencies (dependency_id, tenant_id, job_id, parent_run_id, child_run_id, status, wait_condition, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (dependency_id) DO UPDATE SET status = EXCLUDED.status, wait_condition = EXCLUDED.wait_condition, payload = EXCLUDED.payload;",
            sql_string_literal(&dependency.dependency_id),
            sql_string_literal(tenant_id),
            sql_string_literal(&packet.job.job_id),
            sql_string_literal(&dependency.parent_run_id),
            sql_string_literal(&dependency.child_run_id),
            sql_string_literal(&dependency.status),
            sql_string_literal(&dependency.wait_condition),
            sql_string_literal(&format!(
                r#"{{"dependency_id":{},"parent_run_id":{},"child_run_id":{},"status":{},"wait_condition":{},"execution_allowed":false}}"#,
                json_string_literal(&dependency.dependency_id),
                json_string_literal(&dependency.parent_run_id),
                json_string_literal(&dependency.child_run_id),
                json_string_literal(&dependency.status),
                json_string_literal(&dependency.wait_condition)
            ))
        ));
    }
    for tool in &packet.tool_executions {
        statements.push(format!(
            "INSERT INTO dxr_tool_executions (execution_id, tenant_id, job_id, run_id, actor_id, tool_name, status, outcome, duration_ms, policy_decision, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (execution_id) DO UPDATE SET status = EXCLUDED.status, outcome = EXCLUDED.outcome, policy_decision = EXCLUDED.policy_decision, payload = EXCLUDED.payload;",
            sql_string_literal(&tool.execution_id),
            sql_string_literal(tenant_id),
            sql_string_literal(&packet.job.job_id),
            sql_string_literal(&tool.run_id),
            sql_string_literal(actor_id),
            sql_string_literal(&tool.tool_name),
            sql_string_literal(&tool.status),
            sql_string_literal(&tool.outcome),
            tool.duration_ms,
            sql_string_literal(&tool.policy_decision),
            sql_string_literal(&format!(
                r#"{{"execution_id":{},"run_id":{},"tool_name":{},"status":{},"outcome":{},"duration_ms":{},"policy_decision":{},"tool_execution_allowed":false}}"#,
                json_string_literal(&tool.execution_id),
                json_string_literal(&tool.run_id),
                json_string_literal(&tool.tool_name),
                json_string_literal(&tool.status),
                json_string_literal(&tool.outcome),
                tool.duration_ms,
                json_string_literal(&tool.policy_decision)
            ))
        ));
    }
    for verdict in &packet.harness_verdicts {
        statements.push(format!(
            "INSERT INTO dxr_harness_verdicts (verdict_id, tenant_id, job_id, run_id, check_name, passed, severity, evidence, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (verdict_id) DO UPDATE SET passed = EXCLUDED.passed, severity = EXCLUDED.severity, evidence = EXCLUDED.evidence, payload = EXCLUDED.payload;",
            sql_string_literal(&verdict.verdict_id),
            sql_string_literal(tenant_id),
            sql_string_literal(&packet.job.job_id),
            sql_string_literal(&verdict.run_id),
            sql_string_literal(&verdict.check_name),
            verdict.passed,
            sql_string_literal(&verdict.severity),
            sql_string_literal(&verdict.evidence),
            sql_string_literal(&format!(
                r#"{{"verdict_id":{},"run_id":{},"check_name":{},"passed":{},"severity":{},"evidence":{},"harness_driver":{},"execution_allowed":false}}"#,
                json_string_literal(&verdict.verdict_id),
                json_string_literal(&verdict.run_id),
                json_string_literal(&verdict.check_name),
                verdict.passed,
                json_string_literal(&verdict.severity),
                json_string_literal(&verdict.evidence),
                json_string_literal(&packet.harness_driver)
            ))
        ));
    }
    for evidence in &packet.evidence_records {
        statements.push(format!(
            "INSERT INTO dxr_evidence_ledger (evidence_record_id, tenant_id, sequence_number, previous_hash, record_hash, event_type, job_id, run_id, actor_id, payload) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (evidence_record_id) DO UPDATE SET previous_hash = EXCLUDED.previous_hash, record_hash = EXCLUDED.record_hash, payload = EXCLUDED.payload;",
            sql_string_literal(&evidence.evidence_id),
            sql_string_literal(tenant_id),
            evidence.sequence_number,
            sql_string_literal(&evidence.previous_hash),
            sql_string_literal(&evidence.record_hash),
            sql_string_literal(&evidence.event_type),
            sql_optional_string_literal(evidence.job_id.as_deref()),
            sql_optional_string_literal(evidence.run_id.as_deref()),
            sql_optional_string_literal(evidence.agent_id.as_deref().or(Some(actor_id))),
            sql_string_literal(&json_map_literal(
                &evidence.payload
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str()))
                    .collect::<Vec<_>>()
            ))
        ));
    }
    statements.join("\n")
}

fn json_map_literal(values: &[(&str, &str)]) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| format!(
                "{}:{}",
                json_string_literal(key),
                json_string_literal(value)
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn workflow_event_insert_sql(run: &DxrWorkflowRun, event: &DxrWorkflowEvent) -> String {
    format!(
        "INSERT INTO dxr_workflow_events (workflow_event_id, tenant_id, workflow_run_id, sequence, event_type, terminal_state, payload) VALUES ({}, {}, {}, {}, {}, {}, {}::jsonb) ON CONFLICT (workflow_event_id) DO UPDATE SET terminal_state = EXCLUDED.terminal_state, payload = EXCLUDED.payload;",
        sql_string_literal(&event.workflow_event_id),
        sql_string_literal(&run.tenant_id),
        sql_string_literal(&run.workflow_run_id),
        event.sequence,
        sql_string_literal(&event.event_type),
        sql_string_literal(&event.terminal_state),
        sql_string_literal(&format!(
            r#"{{"workflow_run_id":{},"event_type":{},"terminal_state":{},"live_worker_execution_allowed":false,"temporal_durability_claimed":false}}"#,
            json_string_literal(&run.workflow_run_id),
            json_string_literal(&event.event_type),
            json_string_literal(&event.terminal_state)
        ))
    )
}

fn identity_sql(tenant_id: &str, actor_id: &str) -> String {
    format!(
        "INSERT INTO tenants (tenant_id, name) VALUES ({}, {}) ON CONFLICT (tenant_id) DO NOTHING;\nINSERT INTO actors (actor_id, tenant_id, display_name, role) VALUES ({}, {}, {}, {}) ON CONFLICT (actor_id) DO NOTHING;",
        sql_string_literal(tenant_id),
        sql_string_literal("MDx local tenant"),
        sql_string_literal(actor_id),
        sql_string_literal(tenant_id),
        sql_string_literal(actor_id),
        sql_string_literal("operator")
    )
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_optional_string_literal(value: Option<&str>) -> String {
    value
        .map(sql_string_literal)
        .unwrap_or_else(|| "NULL".to_string())
}

fn parse_count(rows: &[String], index: usize) -> usize {
    rows.get(index)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

fn run_psql_count(database_url: &str, sql: &str) -> Result<Option<usize>, String> {
    Ok(run_psql_rows(database_url, sql)?
        .iter()
        .rev()
        .find_map(|line| line.trim().parse::<usize>().ok()))
}

fn run_psql_rows(database_url: &str, sql: &str) -> Result<Vec<String>, String> {
    let mut child = Command::new("psql")
        .arg(database_url)
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .arg("-At")
        .arg("-q")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn psql: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "psql stdin unavailable".to_string())?
        .write_all(sql.as_bytes())
        .map_err(|error| format!("write psql stdin: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wait psql: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect())
}
