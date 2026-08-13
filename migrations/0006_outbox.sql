CREATE TABLE IF NOT EXISTS outbox_events (
  event_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  topic TEXT NOT NULL,
  payload JSONB NOT NULL,
  delivered_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE outbox_events ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS outbox_events_tenant_access ON outbox_events;
CREATE POLICY outbox_events_tenant_access ON outbox_events
  USING (tenant_id = current_setting('mdx.tenant_id', true));
