CREATE TABLE IF NOT EXISTS loop_runs (
  run_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  loop_id TEXT NOT NULL,
  agent_id TEXT NOT NULL REFERENCES actors(actor_id),
  workflow_id TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ
);

ALTER TABLE loop_runs ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS loop_runs_tenant_access ON loop_runs;
CREATE POLICY loop_runs_tenant_access ON loop_runs
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS loop_transitions (
  transition_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  run_id TEXT NOT NULL REFERENCES loop_runs(run_id),
  transition TEXT NOT NULL CHECK (transition IN ('TRIGGER', 'ACTION', 'OUTCOME', 'SIGNAL', 'ADJUSTMENT')),
  policy_decision_id TEXT,
  receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE loop_transitions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS loop_transitions_tenant_access ON loop_transitions;
CREATE POLICY loop_transitions_tenant_access ON loop_transitions
  USING (tenant_id = current_setting('mdx.tenant_id', true));
