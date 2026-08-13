CREATE TABLE IF NOT EXISTS strategy_ratification_snapshots (
  strategy_ratification_snapshot_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  proposal_id TEXT NOT NULL,
  runtime_status TEXT NOT NULL,
  ratification_required BOOLEAN NOT NULL DEFAULT true,
  direction_setting_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE strategy_ratification_snapshots ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS strategy_ratification_snapshots_tenant_access ON strategy_ratification_snapshots;
CREATE POLICY strategy_ratification_snapshots_tenant_access ON strategy_ratification_snapshots USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS talent_sponsor_chain_authorities (
  sponsor_chain_authority_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  parent_loop_id TEXT NOT NULL,
  requested_by_loop_id TEXT NOT NULL,
  human_sponsor_chain TEXT NOT NULL,
  scope TEXT NOT NULL,
  live_worker_execution_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE talent_sponsor_chain_authorities ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS talent_sponsor_chain_authorities_tenant_access ON talent_sponsor_chain_authorities;
CREATE POLICY talent_sponsor_chain_authorities_tenant_access ON talent_sponsor_chain_authorities USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS talent_worker_lease_authorities (
  worker_lease_authority_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  parent_loop_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  lease_state TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  live_worker_execution_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE talent_worker_lease_authorities ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS talent_worker_lease_authorities_tenant_access ON talent_worker_lease_authorities;
CREATE POLICY talent_worker_lease_authorities_tenant_access ON talent_worker_lease_authorities USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS talent_budget_authorities (
  budget_authority_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  parent_loop_id TEXT NOT NULL,
  budget_limit INTEGER NOT NULL,
  budget_unit TEXT NOT NULL,
  treasury_status TEXT NOT NULL,
  live_spend_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE talent_budget_authorities ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS talent_budget_authorities_tenant_access ON talent_budget_authorities;
CREATE POLICY talent_budget_authorities_tenant_access ON talent_budget_authorities USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS talent_tool_allowlist_authorities (
  tool_allowlist_authority_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  worker_template_id TEXT NOT NULL,
  tool_allowlist TEXT NOT NULL,
  forbidden_tools TEXT NOT NULL,
  shell_execution_allowed BOOLEAN NOT NULL DEFAULT false,
  patch_application_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE talent_tool_allowlist_authorities ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS talent_tool_allowlist_authorities_tenant_access ON talent_tool_allowlist_authorities;
CREATE POLICY talent_tool_allowlist_authorities_tenant_access ON talent_tool_allowlist_authorities USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS talent_worker_spawn_requests (
  worker_spawn_request_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  worker_template_id TEXT NOT NULL,
  parent_id TEXT NOT NULL,
  runtime_status TEXT NOT NULL,
  live_worker_execution_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE talent_worker_spawn_requests ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS talent_worker_spawn_requests_tenant_access ON talent_worker_spawn_requests;
CREATE POLICY talent_worker_spawn_requests_tenant_access ON talent_worker_spawn_requests USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS worker_runtime_handoffs (
  worker_handoff_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  spawn_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  credential_check_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  worker_template_id TEXT NOT NULL,
  worker_run_id TEXT NOT NULL,
  next_owner TEXT NOT NULL,
  production_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE worker_runtime_handoffs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS worker_runtime_handoffs_tenant_access ON worker_runtime_handoffs;
CREATE POLICY worker_runtime_handoffs_tenant_access ON worker_runtime_handoffs USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS worker_runtime_retirements (
  worker_retirement_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  spawn_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  handoff_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  worker_template_id TEXT NOT NULL,
  worker_run_id TEXT NOT NULL,
  active_runtime_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE worker_runtime_retirements ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS worker_runtime_retirements_tenant_access ON worker_runtime_retirements;
CREATE POLICY worker_runtime_retirements_tenant_access ON worker_runtime_retirements USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS observatory_role_view_snapshots (
  observatory_role_view_snapshot_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  role_count INTEGER NOT NULL,
  declared_source_count INTEGER NOT NULL,
  latest_run_status TEXT NOT NULL,
  hidden_authority_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE observatory_role_view_snapshots ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS observatory_role_view_snapshots_tenant_access ON observatory_role_view_snapshots;
CREATE POLICY observatory_role_view_snapshots_tenant_access ON observatory_role_view_snapshots USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS treasury_reserve_postures (
  treasury_reserve_posture_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  budget_authority_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  reserve_status TEXT NOT NULL,
  budget_limit INTEGER NOT NULL,
  budget_unit TEXT NOT NULL,
  live_spend_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE treasury_reserve_postures ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS treasury_reserve_postures_tenant_access ON treasury_reserve_postures;
CREATE POLICY treasury_reserve_postures_tenant_access ON treasury_reserve_postures USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS strategy_ratification_snapshots_source_idx ON strategy_ratification_snapshots(source_receipt_id);
CREATE INDEX IF NOT EXISTS talent_sponsor_chain_authorities_scope_idx ON talent_sponsor_chain_authorities(scope);
CREATE INDEX IF NOT EXISTS talent_worker_lease_authorities_scope_idx ON talent_worker_lease_authorities(scope);
CREATE INDEX IF NOT EXISTS talent_budget_authorities_parent_idx ON talent_budget_authorities(parent_loop_id);
CREATE INDEX IF NOT EXISTS talent_tool_allowlist_authorities_worker_idx ON talent_tool_allowlist_authorities(worker_template_id);
CREATE INDEX IF NOT EXISTS talent_worker_spawn_requests_parent_idx ON talent_worker_spawn_requests(parent_id);
CREATE INDEX IF NOT EXISTS worker_runtime_handoffs_run_idx ON worker_runtime_handoffs(worker_run_id);
CREATE INDEX IF NOT EXISTS worker_runtime_retirements_run_idx ON worker_runtime_retirements(worker_run_id);
CREATE INDEX IF NOT EXISTS observatory_role_view_snapshots_source_idx ON observatory_role_view_snapshots(source_receipt_id);
CREATE INDEX IF NOT EXISTS treasury_reserve_postures_budget_idx ON treasury_reserve_postures(budget_authority_receipt_id);
