CREATE TABLE IF NOT EXISTS dxr_runtime_jobs (
  job_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  run_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  status TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  ctx_assembly_receipt_id TEXT NOT NULL,
  evidence_record_count INTEGER NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_runtime_jobs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_runtime_jobs_tenant_access ON dxr_runtime_jobs;
CREATE POLICY dxr_runtime_jobs_tenant_access ON dxr_runtime_jobs
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE UNIQUE INDEX IF NOT EXISTS dxr_runtime_jobs_tenant_idempotency_idx
  ON dxr_runtime_jobs(tenant_id, idempotency_key);
CREATE INDEX IF NOT EXISTS dxr_runtime_jobs_tenant_status_idx
  ON dxr_runtime_jobs(tenant_id, status, updated_at DESC);

CREATE TABLE IF NOT EXISTS dxr_runtime_events (
  event_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  event_type TEXT NOT NULL,
  job_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  stream_id TEXT NOT NULL,
  topic TEXT NOT NULL,
  relay_status TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_runtime_events ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_runtime_events_tenant_access ON dxr_runtime_events;
CREATE POLICY dxr_runtime_events_tenant_access ON dxr_runtime_events
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_runtime_events_tenant_sequence_idx
  ON dxr_runtime_events(tenant_id, created_at, event_id);
CREATE INDEX IF NOT EXISTS dxr_runtime_events_tenant_type_idx
  ON dxr_runtime_events(tenant_id, event_type, created_at DESC);

CREATE TABLE IF NOT EXISTS dxr_model_turns (
  turn_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL,
  selected_provider TEXT NOT NULL,
  selected_model TEXT NOT NULL,
  routing_status TEXT NOT NULL,
  streaming_status TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  fallback_used BOOLEAN NOT NULL,
  chunk_count INTEGER NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_model_turns ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_model_turns_tenant_access ON dxr_model_turns;
CREATE POLICY dxr_model_turns_tenant_access ON dxr_model_turns
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_model_turns_tenant_created_idx
  ON dxr_model_turns(tenant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS dxr_worker_boundaries (
  boundary_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL,
  worker_run_id TEXT NOT NULL,
  runtime_status TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  live_worker_execution_allowed BOOLEAN NOT NULL DEFAULT false,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_worker_boundaries ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_worker_boundaries_tenant_access ON dxr_worker_boundaries;
CREATE POLICY dxr_worker_boundaries_tenant_access ON dxr_worker_boundaries
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_worker_boundaries_tenant_created_idx
  ON dxr_worker_boundaries(tenant_id, created_at DESC);
