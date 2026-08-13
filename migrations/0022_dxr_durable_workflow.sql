CREATE TABLE IF NOT EXISTS dxr_workflow_runs (
  workflow_run_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL,
  workflow_type TEXT NOT NULL,
  workflow_runner_driver TEXT NOT NULL,
  workflow_runner_provider TEXT NOT NULL,
  workflow_runtime TEXT NOT NULL,
  task_queue TEXT NOT NULL,
  namespace TEXT NOT NULL,
  retry_policy TEXT NOT NULL,
  timeout_seconds INTEGER NOT NULL,
  status TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  source_job_id TEXT NOT NULL,
  source_run_id TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_workflow_runs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_workflow_runs_tenant_access ON dxr_workflow_runs;
CREATE POLICY dxr_workflow_runs_tenant_access ON dxr_workflow_runs
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_workflow_runs_tenant_status_idx
  ON dxr_workflow_runs(tenant_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS dxr_workflow_runs_source_run_idx
  ON dxr_workflow_runs(tenant_id, source_run_id);

CREATE TABLE IF NOT EXISTS dxr_workflow_events (
  workflow_event_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  workflow_run_id TEXT NOT NULL REFERENCES dxr_workflow_runs(workflow_run_id),
  sequence INTEGER NOT NULL,
  event_type TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_workflow_events ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_workflow_events_tenant_access ON dxr_workflow_events;
CREATE POLICY dxr_workflow_events_tenant_access ON dxr_workflow_events
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE UNIQUE INDEX IF NOT EXISTS dxr_workflow_events_run_sequence_idx
  ON dxr_workflow_events(tenant_id, workflow_run_id, sequence);
CREATE INDEX IF NOT EXISTS dxr_workflow_events_tenant_type_idx
  ON dxr_workflow_events(tenant_id, event_type, created_at DESC);
