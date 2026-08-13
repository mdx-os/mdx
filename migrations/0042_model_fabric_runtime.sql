-- Durable tenant-scoped Model Fabric control plane. Credential values never
-- enter these tables: provider connections retain only a secrets-backend ref.

CREATE TABLE IF NOT EXISTS model_provider_connections (
  connection_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  provider_id TEXT NOT NULL,
  credential_ref TEXT NOT NULL,
  endpoint_base_url TEXT NOT NULL,
  region TEXT NOT NULL,
  residency TEXT NOT NULL,
  data_retention TEXT NOT NULL CHECK (data_retention IN ('none', 'provider', 'unknown')),
  training_policy TEXT NOT NULL CHECK (training_policy IN ('none', 'allowed', 'unknown')),
  data_policy_provenance TEXT NOT NULL,
  data_policy_observed_at TEXT NOT NULL,
  health TEXT NOT NULL CHECK (health IN ('disconnected', 'healthy', 'observed', 'unhealthy')),
  live_call_allowed BOOLEAN NOT NULL DEFAULT false,
  secret_value_recorded BOOLEAN NOT NULL DEFAULT false,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_catalog_models (
  model_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  provider_model_id TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  lifecycle TEXT NOT NULL,
  modality TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, model_id)
);

CREATE TABLE IF NOT EXISTS model_deployments (
  deployment_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  connection_id TEXT NOT NULL REFERENCES model_provider_connections(connection_id),
  provider_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  privacy_class TEXT NOT NULL,
  region TEXT NOT NULL,
  residency TEXT NOT NULL,
  enabled BOOLEAN NOT NULL DEFAULT true,
  tools BOOLEAN NOT NULL DEFAULT false,
  structured_output BOOLEAN NOT NULL DEFAULT false,
  vision BOOLEAN NOT NULL DEFAULT false,
  context_tokens BIGINT NOT NULL DEFAULT 0,
  capability_provenance TEXT NOT NULL,
  capability_observed_at TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  FOREIGN KEY (tenant_id, model_id) REFERENCES model_catalog_models(tenant_id, model_id)
);

CREATE TABLE IF NOT EXISTS model_price_observations (
  price_observation_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  deployment_id TEXT NOT NULL REFERENCES model_deployments(deployment_id),
  input_microusd_per_million BIGINT NOT NULL,
  output_microusd_per_million BIGINT NOT NULL,
  currency TEXT NOT NULL,
  source TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_route_policies (
  policy_key TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  policy_id TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  lifecycle TEXT NOT NULL CHECK (lifecycle IN ('static', 'adaptive')),
  workload_ids TEXT NOT NULL,
  canary_percent INTEGER NOT NULL DEFAULT 0 CHECK (canary_percent BETWEEN 0 AND 100),
  app_id TEXT NOT NULL,
  environment TEXT NOT NULL,
  policy_json TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('draft', 'shadow', 'canary', 'promoted', 'rollback_required')),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_route_decisions (
  decision_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  workload_id TEXT NOT NULL,
  app_id TEXT NOT NULL,
  environment TEXT NOT NULL,
  policy_id TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  preset TEXT NOT NULL,
  selected_deployment_id TEXT REFERENCES model_deployments(deployment_id),
  selection_reason TEXT NOT NULL,
  session_id TEXT NOT NULL,
  session_sticky_applied BOOLEAN NOT NULL DEFAULT false,
  provider_failover_deployment_ids TEXT NOT NULL,
  model_fallback_deployment_ids TEXT NOT NULL,
  exclusions_json TEXT NOT NULL,
  grants_execution_authority BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_outcomes (
  outcome_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  decision_id TEXT NOT NULL REFERENCES model_route_decisions(decision_id),
  workload_id TEXT NOT NULL,
  deployment_id TEXT NOT NULL REFERENCES model_deployments(deployment_id),
  latency_ms BIGINT NOT NULL,
  cost_microusd BIGINT,
  quality_score INTEGER CHECK (quality_score BETWEEN 0 AND 100),
  safety_status TEXT NOT NULL CHECK (safety_status IN ('not_evaluated', 'passed', 'failed')),
  task_status TEXT NOT NULL CHECK (task_status IN ('not_evaluated', 'succeeded', 'failed')),
  correction_status TEXT NOT NULL CHECK (correction_status IN ('not_observed', 'accepted', 'corrected')),
  provenance TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_adaptive_policy_versions (
  adaptive_policy_key TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  policy_id TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('draft', 'shadow', 'canary', 'promoted', 'rollback_required')),
  replay_cases BIGINT NOT NULL DEFAULT 0,
  shadow_decisions BIGINT NOT NULL DEFAULT 0,
  canary_decisions BIGINT NOT NULL DEFAULT 0,
  guardrail_evidence_sufficient BOOLEAN NOT NULL DEFAULT false,
  guardrails_passed BOOLEAN NOT NULL DEFAULT false,
  grants_execution_authority BOOLEAN NOT NULL DEFAULT false,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_adaptive_comparisons (
  comparison_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  comparison_kind TEXT NOT NULL CHECK (comparison_kind IN ('replay', 'shadow')),
  policy_id TEXT NOT NULL,
  policy_version TEXT NOT NULL,
  baseline_decision_id TEXT NOT NULL REFERENCES model_route_decisions(decision_id),
  baseline_deployment_id TEXT NOT NULL REFERENCES model_deployments(deployment_id),
  candidate_status TEXT NOT NULL,
  candidate_deployment_id TEXT REFERENCES model_deployments(deployment_id),
  candidate_changed_route BOOLEAN NOT NULL,
  grants_execution_authority BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE model_provider_connections ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_provider_connections FORCE ROW LEVEL SECURITY;
ALTER TABLE model_catalog_models ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_catalog_models FORCE ROW LEVEL SECURITY;
ALTER TABLE model_deployments ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_deployments FORCE ROW LEVEL SECURITY;
ALTER TABLE model_price_observations ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_price_observations FORCE ROW LEVEL SECURITY;
ALTER TABLE model_route_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_route_policies FORCE ROW LEVEL SECURITY;
ALTER TABLE model_route_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_route_decisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_outcomes ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_outcomes FORCE ROW LEVEL SECURITY;
ALTER TABLE model_adaptive_policy_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_adaptive_policy_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_adaptive_comparisons ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_adaptive_comparisons FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS model_provider_connections_tenant_access ON model_provider_connections;
CREATE POLICY model_provider_connections_tenant_access ON model_provider_connections USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_provider_connections_persist_plane ON model_provider_connections;
CREATE POLICY model_provider_connections_persist_plane ON model_provider_connections FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_catalog_models_tenant_access ON model_catalog_models;
CREATE POLICY model_catalog_models_tenant_access ON model_catalog_models USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_catalog_models_persist_plane ON model_catalog_models;
CREATE POLICY model_catalog_models_persist_plane ON model_catalog_models FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_deployments_tenant_access ON model_deployments;
CREATE POLICY model_deployments_tenant_access ON model_deployments USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_deployments_persist_plane ON model_deployments;
CREATE POLICY model_deployments_persist_plane ON model_deployments FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_price_observations_tenant_access ON model_price_observations;
CREATE POLICY model_price_observations_tenant_access ON model_price_observations USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_price_observations_persist_plane ON model_price_observations;
CREATE POLICY model_price_observations_persist_plane ON model_price_observations FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_route_policies_tenant_access ON model_route_policies;
CREATE POLICY model_route_policies_tenant_access ON model_route_policies USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_route_policies_persist_plane ON model_route_policies;
CREATE POLICY model_route_policies_persist_plane ON model_route_policies FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_route_decisions_tenant_access ON model_route_decisions;
CREATE POLICY model_route_decisions_tenant_access ON model_route_decisions USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_route_decisions_persist_plane ON model_route_decisions;
CREATE POLICY model_route_decisions_persist_plane ON model_route_decisions FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_outcomes_tenant_access ON model_outcomes;
CREATE POLICY model_outcomes_tenant_access ON model_outcomes USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_outcomes_persist_plane ON model_outcomes;
CREATE POLICY model_outcomes_persist_plane ON model_outcomes FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_adaptive_policy_versions_tenant_access ON model_adaptive_policy_versions;
CREATE POLICY model_adaptive_policy_versions_tenant_access ON model_adaptive_policy_versions USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_adaptive_policy_versions_persist_plane ON model_adaptive_policy_versions;
CREATE POLICY model_adaptive_policy_versions_persist_plane ON model_adaptive_policy_versions FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS model_adaptive_comparisons_tenant_access ON model_adaptive_comparisons;
CREATE POLICY model_adaptive_comparisons_tenant_access ON model_adaptive_comparisons USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS model_adaptive_comparisons_persist_plane ON model_adaptive_comparisons;
CREATE POLICY model_adaptive_comparisons_persist_plane ON model_adaptive_comparisons FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

CREATE INDEX IF NOT EXISTS model_provider_connections_tenant_provider_idx ON model_provider_connections (tenant_id, provider_id);
CREATE INDEX IF NOT EXISTS model_deployments_tenant_model_idx ON model_deployments (tenant_id, model_id);
CREATE INDEX IF NOT EXISTS model_route_decisions_tenant_workload_idx ON model_route_decisions (tenant_id, workload_id, created_at DESC);
CREATE INDEX IF NOT EXISTS model_outcomes_tenant_decision_idx ON model_outcomes (tenant_id, decision_id, created_at DESC);
CREATE INDEX IF NOT EXISTS model_adaptive_comparisons_policy_idx ON model_adaptive_comparisons (tenant_id, policy_id, policy_version, comparison_kind, created_at DESC);
