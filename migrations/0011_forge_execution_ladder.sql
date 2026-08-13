CREATE TABLE IF NOT EXISTS forge_talent_authorizations (
  talent_authorization_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  worker_authority_request_id TEXT NOT NULL REFERENCES forge_worker_authority_requests(worker_authority_request_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  requested_worker_profile TEXT NOT NULL,
  talent_scope TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  worker_credential_issued BOOLEAN NOT NULL DEFAULT false,
  worker_spawn_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_talent_authorizations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_talent_authorizations_tenant_access ON forge_talent_authorizations;
CREATE POLICY forge_talent_authorizations_tenant_access ON forge_talent_authorizations
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS forge_talent_authorizations_worker_authority_idx ON forge_talent_authorizations(tenant_id, worker_authority_request_id, created_at);

CREATE TABLE IF NOT EXISTS forge_worker_credential_checks (
  credential_check_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  talent_authorization_id TEXT NOT NULL REFERENCES forge_talent_authorizations(talent_authorization_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  worker_template_id TEXT NOT NULL,
  eval_credential_id TEXT NOT NULL,
  capability_scope TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  credential_status TEXT NOT NULL,
  worker_spawn_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_worker_credential_checks ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_worker_credential_checks_tenant_access ON forge_worker_credential_checks;
CREATE POLICY forge_worker_credential_checks_tenant_access ON forge_worker_credential_checks
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS forge_worker_credential_checks_talent_idx ON forge_worker_credential_checks(tenant_id, talent_authorization_id, created_at);

CREATE TABLE IF NOT EXISTS forge_worker_spawn_preflights (
  worker_spawn_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  credential_check_id TEXT NOT NULL REFERENCES forge_worker_credential_checks(credential_check_id),
  talent_authorization_id TEXT NOT NULL REFERENCES forge_talent_authorizations(talent_authorization_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  worker_template_id TEXT NOT NULL,
  parent_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  spawn_request_shape_valid BOOLEAN NOT NULL DEFAULT true,
  worker_spawn_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_worker_spawn_preflights ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_worker_spawn_preflights_tenant_access ON forge_worker_spawn_preflights;
CREATE POLICY forge_worker_spawn_preflights_tenant_access ON forge_worker_spawn_preflights
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS forge_worker_spawn_preflights_credential_idx ON forge_worker_spawn_preflights(tenant_id, credential_check_id, created_at);

CREATE TABLE IF NOT EXISTS forge_ci_evidence_preflights (
  ci_evidence_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  worker_spawn_preflight_id TEXT NOT NULL REFERENCES forge_worker_spawn_preflights(worker_spawn_preflight_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  workflow_run_id TEXT NOT NULL,
  job_id TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  branch TEXT NOT NULL,
  conclusion TEXT NOT NULL,
  evidence_url TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  ci_claim_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_ci_evidence_preflights ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_ci_evidence_preflights_tenant_access ON forge_ci_evidence_preflights;
CREATE POLICY forge_ci_evidence_preflights_tenant_access ON forge_ci_evidence_preflights
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS forge_ci_evidence_preflights_worker_spawn_idx ON forge_ci_evidence_preflights(tenant_id, worker_spawn_preflight_id, created_at);

CREATE TABLE IF NOT EXISTS forge_human_ratification_preflights (
  human_ratification_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  ci_evidence_preflight_id TEXT NOT NULL REFERENCES forge_ci_evidence_preflights(ci_evidence_preflight_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  ratifier_id TEXT NOT NULL,
  ratifier_role TEXT NOT NULL,
  candidate_decision TEXT NOT NULL,
  scope TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  ship_authority_allowed BOOLEAN NOT NULL DEFAULT false,
  deployment_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_human_ratification_preflights ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_human_ratification_preflights_tenant_access ON forge_human_ratification_preflights;
CREATE POLICY forge_human_ratification_preflights_tenant_access ON forge_human_ratification_preflights
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS forge_human_ratification_preflights_ci_idx ON forge_human_ratification_preflights(tenant_id, ci_evidence_preflight_id, created_at);

CREATE TABLE IF NOT EXISTS forge_deployment_preflights (
  deployment_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  human_ratification_preflight_id TEXT NOT NULL REFERENCES forge_human_ratification_preflights(human_ratification_preflight_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  environment TEXT NOT NULL,
  target TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  deployer_id TEXT NOT NULL,
  deployer_role TEXT NOT NULL,
  scope TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  deployment_authority_granted BOOLEAN NOT NULL DEFAULT false,
  forge_production_deployed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_deployment_preflights ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_deployment_preflights_tenant_access ON forge_deployment_preflights;
CREATE POLICY forge_deployment_preflights_tenant_access ON forge_deployment_preflights
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS forge_deployment_preflights_human_idx ON forge_deployment_preflights(tenant_id, human_ratification_preflight_id, created_at);
