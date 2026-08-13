CREATE TABLE IF NOT EXISTS forge_intake_plan_requests (
  forge_intake_plan_request_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  requested_change TEXT NOT NULL,
  linked_message_thread_id TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  execution_authority TEXT NOT NULL CHECK (execution_authority IN ('BLOCKED', 'APPROVED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_intake_plan_requests ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_intake_plan_requests_tenant_access ON forge_intake_plan_requests;
CREATE POLICY forge_intake_plan_requests_tenant_access ON forge_intake_plan_requests
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS forge_intake_plan_requests_tenant_created_idx
  ON forge_intake_plan_requests (tenant_id, created_at DESC);
