CREATE INDEX IF NOT EXISTS ledger_entries_tenant_loop_created_idx
  ON ledger_entries (tenant_id, loop_id, created_at DESC)
  WHERE loop_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS ledger_entries_tenant_workflow_created_idx
  ON ledger_entries (tenant_id, workflow_id, created_at DESC)
  WHERE workflow_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS ledger_chain_heads_updated_idx
  ON ledger_chain_heads (updated_at DESC);

CREATE INDEX IF NOT EXISTS outbox_events_tenant_topic_pending_created_idx
  ON outbox_events (tenant_id, topic, created_at)
  WHERE delivered_at IS NULL;

CREATE INDEX IF NOT EXISTS outbox_events_tenant_delivered_created_idx
  ON outbox_events (tenant_id, delivered_at DESC)
  WHERE delivered_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS dxr_runtime_jobs_tenant_actor_updated_idx
  ON dxr_runtime_jobs (tenant_id, actor_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS dxr_runtime_jobs_tenant_run_idx
  ON dxr_runtime_jobs (tenant_id, run_id);

CREATE INDEX IF NOT EXISTS dxr_runtime_events_tenant_topic_created_idx
  ON dxr_runtime_events (tenant_id, topic, created_at DESC, event_id);

CREATE INDEX IF NOT EXISTS dxr_runtime_events_tenant_run_created_idx
  ON dxr_runtime_events (tenant_id, run_id, created_at DESC);

CREATE INDEX IF NOT EXISTS dxr_runtime_events_tenant_actor_created_idx
  ON dxr_runtime_events (tenant_id, actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS dxr_model_turns_tenant_provider_created_idx
  ON dxr_model_turns (tenant_id, selected_provider, created_at DESC);

CREATE INDEX IF NOT EXISTS dxr_worker_boundaries_worker_run_idx
  ON dxr_worker_boundaries (tenant_id, worker_run_id, created_at DESC);

CREATE INDEX IF NOT EXISTS dxr_workflow_runs_tenant_actor_updated_idx
  ON dxr_workflow_runs (tenant_id, actor_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS dxr_workflow_runs_task_queue_status_idx
  ON dxr_workflow_runs (tenant_id, task_queue, status, updated_at DESC);

CREATE INDEX IF NOT EXISTS dxr_ctx_context_inputs_handoff_idx
  ON dxr_ctx_context_inputs (tenant_id, ctx_handoff_id, created_at DESC)
  WHERE ctx_handoff_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS dxr_evidence_ledger_job_idx
  ON dxr_evidence_ledger (tenant_id, job_id, sequence_number)
  WHERE job_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS dxr_evidence_ledger_actor_created_idx
  ON dxr_evidence_ledger (tenant_id, actor_id, created_at DESC)
  WHERE actor_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS dxr_run_dependencies_job_status_idx
  ON dxr_run_dependencies (tenant_id, job_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS dxr_tool_executions_actor_created_idx
  ON dxr_tool_executions (tenant_id, actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS dxr_tool_executions_policy_created_idx
  ON dxr_tool_executions (tenant_id, policy_decision, created_at DESC);

CREATE INDEX IF NOT EXISTS dxr_harness_verdicts_failed_severity_idx
  ON dxr_harness_verdicts (tenant_id, severity, created_at DESC)
  WHERE passed = false;

CREATE INDEX IF NOT EXISTS dxr_harness_verdicts_job_idx
  ON dxr_harness_verdicts (tenant_id, job_id, created_at DESC);
