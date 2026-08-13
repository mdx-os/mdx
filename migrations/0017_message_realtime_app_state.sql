CREATE TABLE IF NOT EXISTS message_realtime_cutover_preflights (
  realtime_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  presence_request_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  thread_id TEXT NOT NULL,
  channel_id TEXT NOT NULL,
  requested_realtime_scope TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  realtime_provider_turn_on_observed BOOLEAN NOT NULL DEFAULT false,
  production_delivery_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE message_realtime_cutover_preflights ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS message_realtime_cutover_preflights_tenant_access ON message_realtime_cutover_preflights;
CREATE POLICY message_realtime_cutover_preflights_tenant_access ON message_realtime_cutover_preflights USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS message_delivery_replay_batches (
  delivery_replay_batch_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  realtime_preflight_id TEXT NOT NULL,
  channel_id TEXT NOT NULL,
  replay_scope TEXT NOT NULL,
  replay_state TEXT NOT NULL,
  websocket_fanout_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE message_delivery_replay_batches ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS message_delivery_replay_batches_tenant_access ON message_delivery_replay_batches;
CREATE POLICY message_delivery_replay_batches_tenant_access ON message_delivery_replay_batches USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS message_subscription_isolation_checks (
  subscription_isolation_check_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  realtime_preflight_id TEXT NOT NULL,
  channel_id TEXT NOT NULL,
  isolation_status TEXT NOT NULL,
  tenant_subscription_isolation_proven BOOLEAN NOT NULL DEFAULT true,
  service_role_fanout_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE message_subscription_isolation_checks ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS message_subscription_isolation_checks_tenant_access ON message_subscription_isolation_checks;
CREATE POLICY message_subscription_isolation_checks_tenant_access ON message_subscription_isolation_checks USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS message_service_role_fanout_refusals (
  service_role_fanout_refusal_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  realtime_preflight_id TEXT NOT NULL,
  refusal_reason TEXT NOT NULL,
  service_role_fanout_refused BOOLEAN NOT NULL DEFAULT true,
  presence_mutation_allowed BOOLEAN NOT NULL DEFAULT false,
  typing_indicator_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE message_service_role_fanout_refusals ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS message_service_role_fanout_refusals_tenant_access ON message_service_role_fanout_refusals;
CREATE POLICY message_service_role_fanout_refusals_tenant_access ON message_service_role_fanout_refusals USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS message_realtime_cutover_preflights_thread_idx ON message_realtime_cutover_preflights(thread_id);
CREATE INDEX IF NOT EXISTS message_delivery_replay_batches_channel_idx ON message_delivery_replay_batches(channel_id);
CREATE INDEX IF NOT EXISTS message_subscription_isolation_checks_channel_idx ON message_subscription_isolation_checks(channel_id);
CREATE INDEX IF NOT EXISTS message_service_role_fanout_refusals_preflight_idx ON message_service_role_fanout_refusals(realtime_preflight_id);
