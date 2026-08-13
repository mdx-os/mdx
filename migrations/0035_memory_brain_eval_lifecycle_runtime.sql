CREATE TABLE IF NOT EXISTS memory_lifecycle_evaluations (
    evaluation_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    policy TEXT NOT NULL,
    evaluated_memory_count INTEGER NOT NULL,
    stale_count INTEGER NOT NULL,
    contradiction_count INTEGER NOT NULL,
    supersession_count INTEGER NOT NULL,
    trigger_receipt_id TEXT NOT NULL,
    trusted_time_floor TEXT NOT NULL,
    receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_eval_fixture_results (
    fixture_result_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    fixture_family TEXT NOT NULL,
    query TEXT NOT NULL,
    expected_memory_id TEXT NOT NULL,
    matched_memory_id TEXT NOT NULL,
    final_score INTEGER NOT NULL,
    passed BOOLEAN NOT NULL,
    receipt_id TEXT NOT NULL
);

ALTER TABLE memory_lifecycle_evaluations ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_eval_fixture_results ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_lifecycle_evaluations_tenant_access ON memory_lifecycle_evaluations;
CREATE POLICY memory_lifecycle_evaluations_tenant_access ON memory_lifecycle_evaluations USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_eval_fixture_results_tenant_access ON memory_eval_fixture_results;
CREATE POLICY memory_eval_fixture_results_tenant_access ON memory_eval_fixture_results USING (tenant_id = current_setting('mdx.tenant_id', true));

ALTER TABLE memory_lifecycle_evaluations FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_eval_fixture_results FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_lifecycle_evaluations_persist_plane ON memory_lifecycle_evaluations;
CREATE POLICY memory_lifecycle_evaluations_persist_plane ON memory_lifecycle_evaluations FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_eval_fixture_results_persist_plane ON memory_eval_fixture_results;
CREATE POLICY memory_eval_fixture_results_persist_plane ON memory_eval_fixture_results FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
