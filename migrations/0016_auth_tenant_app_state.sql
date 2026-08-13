CREATE TABLE IF NOT EXISTS auth_tenant_orgs (
  auth_tenant_org_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  tenant_key TEXT NOT NULL,
  org_state TEXT NOT NULL,
  production_auth_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_tenant_orgs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_tenant_orgs_tenant_access ON auth_tenant_orgs;
CREATE POLICY auth_tenant_orgs_tenant_access ON auth_tenant_orgs USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS auth_tenant_memberships (
  auth_tenant_membership_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  actor_id TEXT NOT NULL,
  tenant_membership_scope TEXT NOT NULL,
  membership_state TEXT NOT NULL,
  service_role_shortcut_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_tenant_memberships ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_tenant_memberships_tenant_access ON auth_tenant_memberships;
CREATE POLICY auth_tenant_memberships_tenant_access ON auth_tenant_memberships USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS auth_role_mappings (
  auth_role_mapping_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  actor_id TEXT NOT NULL,
  role_mapping_scope TEXT NOT NULL,
  mapped_role TEXT NOT NULL,
  role_escalation_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_role_mappings ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_role_mappings_tenant_access ON auth_role_mappings;
CREATE POLICY auth_role_mappings_tenant_access ON auth_role_mappings USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS auth_invite_states (
  auth_invite_state_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  invite_state_scope TEXT NOT NULL,
  invite_state TEXT NOT NULL,
  production_role_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_invite_states ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_invite_states_tenant_access ON auth_invite_states;
CREATE POLICY auth_invite_states_tenant_access ON auth_invite_states USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS auth_visibility_policies (
  auth_visibility_policy_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  visibility_scope TEXT NOT NULL,
  policy_state TEXT NOT NULL,
  user_metadata_authorization_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_visibility_policies ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_visibility_policies_tenant_access ON auth_visibility_policies;
CREATE POLICY auth_visibility_policies_tenant_access ON auth_visibility_policies USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS auth_approved_model_policies (
  auth_approved_model_policy_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  approved_model_scope TEXT NOT NULL,
  approved_model_id TEXT NOT NULL,
  provider_call_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_approved_model_policies ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_approved_model_policies_tenant_access ON auth_approved_model_policies;
CREATE POLICY auth_approved_model_policies_tenant_access ON auth_approved_model_policies USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS auth_session_evidence (
  auth_session_evidence_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  actor_id TEXT NOT NULL,
  source_auth_session_evidence_id TEXT NOT NULL,
  auth_session_status TEXT NOT NULL,
  production_auth_provider_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_session_evidence ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_session_evidence_tenant_access ON auth_session_evidence;
CREATE POLICY auth_session_evidence_tenant_access ON auth_session_evidence USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS auth_tenant_policy_preflights (
  auth_tenant_policy_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  preflight_id TEXT NOT NULL,
  policy_decision_id TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  service_role_shortcut_allowed BOOLEAN NOT NULL DEFAULT false,
  implicit_tenant_inference_allowed BOOLEAN NOT NULL DEFAULT false,
  supabase_auth_claim_authority_allowed BOOLEAN NOT NULL DEFAULT false,
  cutover_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE auth_tenant_policy_preflights ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS auth_tenant_policy_preflights_tenant_access ON auth_tenant_policy_preflights;
CREATE POLICY auth_tenant_policy_preflights_tenant_access ON auth_tenant_policy_preflights USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS auth_tenant_orgs_source_idx ON auth_tenant_orgs(source_receipt_id);
CREATE INDEX IF NOT EXISTS auth_tenant_memberships_actor_idx ON auth_tenant_memberships(actor_id);
CREATE INDEX IF NOT EXISTS auth_role_mappings_actor_idx ON auth_role_mappings(actor_id);
CREATE INDEX IF NOT EXISTS auth_invite_states_scope_idx ON auth_invite_states(invite_state_scope);
CREATE INDEX IF NOT EXISTS auth_visibility_policies_scope_idx ON auth_visibility_policies(visibility_scope);
CREATE INDEX IF NOT EXISTS auth_approved_model_policies_scope_idx ON auth_approved_model_policies(approved_model_scope);
CREATE INDEX IF NOT EXISTS auth_session_evidence_source_idx ON auth_session_evidence(source_auth_session_evidence_id);
CREATE INDEX IF NOT EXISTS auth_tenant_policy_preflights_preflight_idx ON auth_tenant_policy_preflights(preflight_id);
