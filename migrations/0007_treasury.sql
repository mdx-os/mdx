CREATE TABLE IF NOT EXISTS treasury_authorizations (
  treasury_authorization_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  policy_decision_id TEXT NOT NULL REFERENCES policy_decisions(policy_decision_id),
  max_amount_cents INTEGER NOT NULL CHECK (max_amount_cents >= 0),
  counterparty TEXT NOT NULL,
  purpose TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'REVOKED', 'EXPIRED')),
  receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE treasury_authorizations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS treasury_authorizations_tenant_access ON treasury_authorizations;
CREATE POLICY treasury_authorizations_tenant_access ON treasury_authorizations
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS charter_records (
  charter_record_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  obligation TEXT NOT NULL,
  evidence TEXT NOT NULL,
  receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE charter_records ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS charter_records_tenant_access ON charter_records;
CREATE POLICY charter_records_tenant_access ON charter_records
  USING (tenant_id = current_setting('mdx.tenant_id', true));
