CREATE TABLE IF NOT EXISTS policy_decisions (
  policy_decision_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  action TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('ALLOW', 'ESCALATE', 'DENY')),
  reason TEXT NOT NULL,
  receipt_id TEXT REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE policy_decisions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS policy_decisions_tenant_access ON policy_decisions;
CREATE POLICY policy_decisions_tenant_access ON policy_decisions
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS eval_verdicts (
  eval_verdict_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  suite_id TEXT NOT NULL,
  passed BOOLEAN NOT NULL,
  score INTEGER NOT NULL CHECK (score >= 0 AND score <= 100),
  trace_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE eval_verdicts ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS eval_verdicts_tenant_access ON eval_verdicts;
CREATE POLICY eval_verdicts_tenant_access ON eval_verdicts
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS agent_credentials (
  credential_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  eval_verdict_id TEXT NOT NULL REFERENCES eval_verdicts(eval_verdict_id),
  status TEXT NOT NULL CHECK (status IN ('MINTED', 'RENEWED', 'WITHHELD')),
  receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  expires_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE agent_credentials ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS agent_credentials_tenant_access ON agent_credentials;
CREATE POLICY agent_credentials_tenant_access ON agent_credentials
  USING (tenant_id = current_setting('mdx.tenant_id', true));
