CREATE TABLE IF NOT EXISTS world_facts (
  fact_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  subject TEXT NOT NULL,
  predicate TEXT NOT NULL,
  object TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE world_facts ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS world_facts_tenant_access ON world_facts;
CREATE POLICY world_facts_tenant_access ON world_facts
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS memory_records (
  memory_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  consolidation_decision TEXT NOT NULL CHECK (consolidation_decision IN ('RETAIN', 'SKIP')),
  content TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE memory_records ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_records_tenant_access ON memory_records;
CREATE POLICY memory_records_tenant_access ON memory_records
  USING (tenant_id = current_setting('mdx.tenant_id', true));
