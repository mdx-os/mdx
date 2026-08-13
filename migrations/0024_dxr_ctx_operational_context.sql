CREATE TABLE IF NOT EXISTS dxr_ctx_context_inputs (
  ctx_context_input_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  ctx_assembly_receipt_id TEXT NOT NULL,
  ctx_session_id TEXT,
  ctx_handoff_id TEXT,
  ctx_state_keys JSONB NOT NULL,
  ctx_background_job_id TEXT,
  ctx_memory_tiers JSONB NOT NULL,
  context_payload JSONB NOT NULL,
  context_integrity_hash TEXT NOT NULL,
  status TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_ctx_context_inputs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_ctx_context_inputs_tenant_access ON dxr_ctx_context_inputs;
CREATE POLICY dxr_ctx_context_inputs_tenant_access ON dxr_ctx_context_inputs
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_ctx_context_inputs_job_run_idx
  ON dxr_ctx_context_inputs(tenant_id, job_id, run_id, created_at DESC);
CREATE INDEX IF NOT EXISTS dxr_ctx_context_inputs_session_idx
  ON dxr_ctx_context_inputs(tenant_id, ctx_session_id, created_at DESC);
