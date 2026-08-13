CREATE TABLE IF NOT EXISTS product_signals (
  product_signal_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  signal_source TEXT NOT NULL,
  signal_kind TEXT NOT NULL,
  source_surface TEXT NOT NULL,
  production_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE product_signals ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS product_signals_tenant_access ON product_signals;
CREATE POLICY product_signals_tenant_access ON product_signals USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS product_bet_drafts (
  product_bet_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_signal_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  shape_status TEXT NOT NULL,
  forbidden_action TEXT NOT NULL,
  production_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE product_bet_drafts ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS product_bet_drafts_tenant_access ON product_bet_drafts;
CREATE POLICY product_bet_drafts_tenant_access ON product_bet_drafts USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS product_handoff_requests (
  product_handoff_request_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  shaped_bet_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  human_edge_surface TEXT NOT NULL,
  ratification_required BOOLEAN NOT NULL DEFAULT true,
  runtime_status TEXT NOT NULL,
  production_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE product_handoff_requests ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS product_handoff_requests_tenant_access ON product_handoff_requests;
CREATE POLICY product_handoff_requests_tenant_access ON product_handoff_requests USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS eval_guardrail_verdicts (
  eval_guardrail_verdict_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  trace_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  suite TEXT NOT NULL,
  score INTEGER NOT NULL,
  passed BOOLEAN NOT NULL,
  worker_credential_issuance_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE eval_guardrail_verdicts ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS eval_guardrail_verdicts_tenant_access ON eval_guardrail_verdicts;
CREATE POLICY eval_guardrail_verdicts_tenant_access ON eval_guardrail_verdicts USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS security_findings (
  security_finding_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  scan_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  severity TEXT NOT NULL,
  classification TEXT NOT NULL,
  remediation_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE security_findings ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS security_findings_tenant_access ON security_findings;
CREATE POLICY security_findings_tenant_access ON security_findings USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS charter_attestations (
  charter_attestation_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  obligation_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  verdict TEXT NOT NULL,
  exception_status TEXT NOT NULL DEFAULT 'NONE_OPEN',
  production_cutover_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE charter_attestations ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS charter_attestations_tenant_access ON charter_attestations;
CREATE POLICY charter_attestations_tenant_access ON charter_attestations USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS product_signals_source_receipt_idx ON product_signals(source_receipt_id);
CREATE INDEX IF NOT EXISTS product_bet_drafts_signal_idx ON product_bet_drafts(source_signal_receipt_id);
CREATE INDEX IF NOT EXISTS product_handoff_requests_bet_idx ON product_handoff_requests(shaped_bet_receipt_id);
CREATE INDEX IF NOT EXISTS eval_guardrail_verdicts_trace_idx ON eval_guardrail_verdicts(trace_receipt_id);
CREATE INDEX IF NOT EXISTS security_findings_scan_idx ON security_findings(scan_receipt_id);
CREATE INDEX IF NOT EXISTS charter_attestations_obligation_idx ON charter_attestations(obligation_receipt_id);
