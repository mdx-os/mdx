CREATE TABLE IF NOT EXISTS ledger_entries (
  receipt_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  trace_id TEXT NOT NULL,
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  loop_id TEXT,
  workflow_id TEXT,
  kind TEXT NOT NULL,
  policy_decision_id TEXT,
  payload JSONB NOT NULL,
  previous_hash TEXT,
  receipt_timestamp TEXT,
  hash_version INTEGER NOT NULL DEFAULT 1,
  hash TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE ledger_entries ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS ledger_entries_tenant_access ON ledger_entries;
CREATE POLICY ledger_entries_tenant_access ON ledger_entries
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS ledger_chain_heads (
  tenant_id TEXT PRIMARY KEY REFERENCES tenants(tenant_id),
  head_hash TEXT NOT NULL,
  receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE ledger_chain_heads ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS ledger_chain_heads_tenant_access ON ledger_chain_heads;
CREATE POLICY ledger_chain_heads_tenant_access ON ledger_chain_heads
  USING (tenant_id = current_setting('mdx.tenant_id', true));
