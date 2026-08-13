CREATE TABLE IF NOT EXISTS forge_outcome_signals (
  outcome_signal_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  run_id TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  source_receipt_kind TEXT NOT NULL,
  disposition TEXT NOT NULL CHECK (disposition IN ('completed', 'blocked', 'failed', 'stopped', 'budget_exhausted', 'escalated', 'approved', 'declined', 'revised')),
  summary TEXT NOT NULL,
  capability_ids TEXT NOT NULL,
  model_or_worker TEXT NOT NULL,
  lesson_candidate TEXT NOT NULL,
  message_channel_id TEXT NOT NULL,
  learning_candidate_allowed BOOLEAN NOT NULL DEFAULT true,
  message_activity_allowed BOOLEAN NOT NULL DEFAULT true,
  active_memory_write_allowed BOOLEAN NOT NULL DEFAULT false,
  adaptation_allowed BOOLEAN NOT NULL DEFAULT false,
  execution_authority_opened BOOLEAN NOT NULL DEFAULT false,
  production_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_outcome_signals ENABLE ROW LEVEL SECURITY;
ALTER TABLE forge_outcome_signals FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_outcome_signals_tenant_access ON forge_outcome_signals;
CREATE POLICY forge_outcome_signals_tenant_access ON forge_outcome_signals
  USING (tenant_id = current_setting('mdx.tenant_id', true));

DROP POLICY IF EXISTS forge_outcome_signals_persist_plane ON forge_outcome_signals;
CREATE POLICY forge_outcome_signals_persist_plane ON forge_outcome_signals
  FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

CREATE INDEX IF NOT EXISTS forge_outcome_signals_run_idx
  ON forge_outcome_signals (tenant_id, run_id, created_at DESC);

CREATE TABLE IF NOT EXISTS marketplace_acts (
  marketplace_act_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  act TEXT NOT NULL CHECK (act IN ('install', 'approval', 'review', 'revocation', 'try', 'import_candidate', 'pack_install')),
  source_route TEXT NOT NULL,
  capability_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  decision TEXT NOT NULL,
  read_only BOOLEAN NOT NULL DEFAULT false,
  pack_id TEXT NOT NULL,
  url TEXT NOT NULL,
  note TEXT NOT NULL,
  reason TEXT NOT NULL,
  items_added TEXT NOT NULL,
  items_held TEXT NOT NULL,
  item_record_ids TEXT NOT NULL,
  actor_role TEXT NOT NULL,
  capability_execution_allowed BOOLEAN NOT NULL DEFAULT false,
  secret_access_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE marketplace_acts ENABLE ROW LEVEL SECURITY;
ALTER TABLE marketplace_acts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS marketplace_acts_tenant_access ON marketplace_acts;
CREATE POLICY marketplace_acts_tenant_access ON marketplace_acts
  USING (tenant_id = current_setting('mdx.tenant_id', true));

DROP POLICY IF EXISTS marketplace_acts_persist_plane ON marketplace_acts;
CREATE POLICY marketplace_acts_persist_plane ON marketplace_acts
  FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

CREATE INDEX IF NOT EXISTS marketplace_acts_capability_idx
  ON marketplace_acts (tenant_id, capability_id, created_at DESC);

CREATE TABLE IF NOT EXISTS marketplace_installed_capabilities (
  installed_capability_key TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  capability_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  install_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  approval_receipt_id TEXT REFERENCES ledger_entries(receipt_id),
  effective_status TEXT NOT NULL,
  capability_execution_allowed BOOLEAN NOT NULL DEFAULT false,
  secret_access_allowed BOOLEAN NOT NULL DEFAULT false,
  inherited_agent_permissions_allowed BOOLEAN NOT NULL DEFAULT false,
  untrusted_execution_isolation TEXT NOT NULL,
  production_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE marketplace_installed_capabilities ENABLE ROW LEVEL SECURITY;
ALTER TABLE marketplace_installed_capabilities FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS marketplace_installed_capabilities_tenant_access ON marketplace_installed_capabilities;
CREATE POLICY marketplace_installed_capabilities_tenant_access ON marketplace_installed_capabilities
  USING (tenant_id = current_setting('mdx.tenant_id', true));

DROP POLICY IF EXISTS marketplace_installed_capabilities_persist_plane ON marketplace_installed_capabilities;
CREATE POLICY marketplace_installed_capabilities_persist_plane ON marketplace_installed_capabilities
  FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

CREATE INDEX IF NOT EXISTS marketplace_installed_capabilities_capability_idx
  ON marketplace_installed_capabilities (tenant_id, capability_id, scope);
