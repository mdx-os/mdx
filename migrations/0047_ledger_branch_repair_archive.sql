-- Preserve noncanonical receipt branches before a guarded operator repair
-- restores colliding canonical receipt ids from a verified kernel snapshot.
-- These tables are append-only evidence. Tenant archives are visible only
-- inside their tenant context; the global repair run remains service-only.

CREATE TABLE IF NOT EXISTS ledger_repair_runs (
  repair_id TEXT PRIMARY KEY,
  approver_actor_id TEXT NOT NULL,
  approval_reference TEXT NOT NULL,
  declared_head_hash TEXT NOT NULL,
  snapshot_head_hash TEXT NOT NULL,
  archived_branch_receipts INTEGER NOT NULL,
  restored_canonical_receipts INTEGER NOT NULL,
  archived_reference_rows INTEGER NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('completed')),
  completed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE ledger_repair_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE ledger_repair_runs FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS ledger_repair_runs_persist_plane ON ledger_repair_runs;
CREATE POLICY ledger_repair_runs_persist_plane ON ledger_repair_runs
  FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

CREATE TABLE IF NOT EXISTS ledger_branch_entry_archives (
  hash TEXT PRIMARY KEY,
  repair_id TEXT NOT NULL REFERENCES ledger_repair_runs(repair_id),
  receipt_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  trace_id TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  loop_id TEXT,
  workflow_id TEXT,
  kind TEXT NOT NULL,
  policy_decision_id TEXT,
  payload JSONB NOT NULL,
  previous_hash TEXT,
  receipt_timestamp TEXT,
  hash_version INTEGER NOT NULL,
  original_created_at TIMESTAMPTZ NOT NULL,
  approver_actor_id TEXT NOT NULL,
  approval_reference TEXT NOT NULL,
  archived_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE ledger_branch_entry_archives ENABLE ROW LEVEL SECURITY;
ALTER TABLE ledger_branch_entry_archives FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS ledger_branch_entry_archives_tenant_access ON ledger_branch_entry_archives;
CREATE POLICY ledger_branch_entry_archives_tenant_access ON ledger_branch_entry_archives
  USING (tenant_id = current_setting('mdx.tenant_id', true))
  WITH CHECK (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS ledger_branch_entry_archives_persist_plane ON ledger_branch_entry_archives;
CREATE POLICY ledger_branch_entry_archives_persist_plane ON ledger_branch_entry_archives
  FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

CREATE TABLE IF NOT EXISTS ledger_branch_reference_archives (
  archive_id BIGSERIAL PRIMARY KEY,
  repair_id TEXT NOT NULL REFERENCES ledger_repair_runs(repair_id),
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  source_table TEXT NOT NULL,
  source_primary_key TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL,
  source_receipt_hash TEXT NOT NULL,
  row_data JSONB NOT NULL,
  approver_actor_id TEXT NOT NULL,
  approval_reference TEXT NOT NULL,
  archived_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repair_id, source_table, source_primary_key)
);
ALTER TABLE ledger_branch_reference_archives ENABLE ROW LEVEL SECURITY;
ALTER TABLE ledger_branch_reference_archives FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS ledger_branch_reference_archives_tenant_access ON ledger_branch_reference_archives;
CREATE POLICY ledger_branch_reference_archives_tenant_access ON ledger_branch_reference_archives
  USING (tenant_id = current_setting('mdx.tenant_id', true))
  WITH CHECK (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS ledger_branch_reference_archives_persist_plane ON ledger_branch_reference_archives;
CREATE POLICY ledger_branch_reference_archives_persist_plane ON ledger_branch_reference_archives
  FOR ALL TO mdx_persist USING (true) WITH CHECK (true);

CREATE OR REPLACE FUNCTION mdx_refuse_ledger_archive_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION 'ledger repair archives are immutable';
END;
$$;

DROP TRIGGER IF EXISTS ledger_repair_runs_immutable ON ledger_repair_runs;
CREATE TRIGGER ledger_repair_runs_immutable
  BEFORE UPDATE OR DELETE ON ledger_repair_runs
  FOR EACH ROW EXECUTE FUNCTION mdx_refuse_ledger_archive_mutation();

DROP TRIGGER IF EXISTS ledger_branch_entry_archives_immutable ON ledger_branch_entry_archives;
CREATE TRIGGER ledger_branch_entry_archives_immutable
  BEFORE UPDATE OR DELETE ON ledger_branch_entry_archives
  FOR EACH ROW EXECUTE FUNCTION mdx_refuse_ledger_archive_mutation();

DROP TRIGGER IF EXISTS ledger_branch_reference_archives_immutable ON ledger_branch_reference_archives;
CREATE TRIGGER ledger_branch_reference_archives_immutable
  BEFORE UPDATE OR DELETE ON ledger_branch_reference_archives
  FOR EACH ROW EXECUTE FUNCTION mdx_refuse_ledger_archive_mutation();
