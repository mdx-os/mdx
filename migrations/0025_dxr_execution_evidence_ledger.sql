CREATE TABLE IF NOT EXISTS dxr_evidence_ledger (
  evidence_record_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  sequence_number BIGINT NOT NULL,
  previous_hash TEXT NOT NULL,
  record_hash TEXT NOT NULL,
  event_type TEXT NOT NULL,
  job_id TEXT,
  run_id TEXT,
  actor_id TEXT,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_evidence_ledger ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_evidence_ledger_tenant_access ON dxr_evidence_ledger;
CREATE POLICY dxr_evidence_ledger_tenant_access ON dxr_evidence_ledger
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE UNIQUE INDEX IF NOT EXISTS dxr_evidence_ledger_sequence_idx
  ON dxr_evidence_ledger(tenant_id, sequence_number);
CREATE INDEX IF NOT EXISTS dxr_evidence_ledger_run_idx
  ON dxr_evidence_ledger(tenant_id, run_id, sequence_number);
CREATE INDEX IF NOT EXISTS dxr_evidence_ledger_type_idx
  ON dxr_evidence_ledger(tenant_id, event_type, created_at DESC);

CREATE TABLE IF NOT EXISTS dxr_run_dependencies (
  dependency_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  job_id TEXT NOT NULL,
  parent_run_id TEXT NOT NULL,
  child_run_id TEXT NOT NULL,
  status TEXT NOT NULL,
  wait_condition TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_run_dependencies ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_run_dependencies_tenant_access ON dxr_run_dependencies;
CREATE POLICY dxr_run_dependencies_tenant_access ON dxr_run_dependencies
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_run_dependencies_parent_idx
  ON dxr_run_dependencies(tenant_id, parent_run_id, status);
CREATE INDEX IF NOT EXISTS dxr_run_dependencies_child_idx
  ON dxr_run_dependencies(tenant_id, child_run_id);

CREATE TABLE IF NOT EXISTS dxr_tool_executions (
  execution_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  job_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  status TEXT NOT NULL,
  outcome TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  policy_decision TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_tool_executions ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_tool_executions_tenant_access ON dxr_tool_executions;
CREATE POLICY dxr_tool_executions_tenant_access ON dxr_tool_executions
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_tool_executions_run_idx
  ON dxr_tool_executions(tenant_id, run_id, created_at DESC);
CREATE INDEX IF NOT EXISTS dxr_tool_executions_tool_outcome_idx
  ON dxr_tool_executions(tenant_id, tool_name, outcome, created_at DESC);

CREATE TABLE IF NOT EXISTS dxr_harness_verdicts (
  verdict_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  job_id TEXT NOT NULL,
  run_id TEXT NOT NULL,
  check_name TEXT NOT NULL,
  passed BOOLEAN NOT NULL,
  severity TEXT NOT NULL,
  evidence TEXT NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE dxr_harness_verdicts ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS dxr_harness_verdicts_tenant_access ON dxr_harness_verdicts;
CREATE POLICY dxr_harness_verdicts_tenant_access ON dxr_harness_verdicts
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS dxr_harness_verdicts_run_idx
  ON dxr_harness_verdicts(tenant_id, run_id, created_at DESC);
CREATE INDEX IF NOT EXISTS dxr_harness_verdicts_check_idx
  ON dxr_harness_verdicts(tenant_id, check_name, passed);
