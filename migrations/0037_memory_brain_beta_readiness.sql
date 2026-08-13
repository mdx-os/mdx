CREATE TABLE IF NOT EXISTS memory_benchmark_imports (
    import_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    fixture_family TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    task_shape TEXT NOT NULL,
    fixture_count INTEGER NOT NULL,
    synthetic BOOLEAN NOT NULL,
    receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_scale_load_runs (
    scale_run_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    synthetic_session_count INTEGER NOT NULL,
    memory_record_count INTEGER NOT NULL,
    ranking_count INTEGER NOT NULL,
    latency_budget_ms INTEGER NOT NULL,
    observed_p95_latency_ms INTEGER NOT NULL,
    brain_score INTEGER NOT NULL,
    receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_cloud_turn_on_checks (
    check_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    check_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    evidence TEXT NOT NULL,
    receipt_id TEXT NOT NULL
);

ALTER TABLE memory_benchmark_imports ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_scale_load_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_cloud_turn_on_checks ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_benchmark_imports_tenant_access ON memory_benchmark_imports;
CREATE POLICY memory_benchmark_imports_tenant_access ON memory_benchmark_imports USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_scale_load_runs_tenant_access ON memory_scale_load_runs;
CREATE POLICY memory_scale_load_runs_tenant_access ON memory_scale_load_runs USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_cloud_turn_on_checks_tenant_access ON memory_cloud_turn_on_checks;
CREATE POLICY memory_cloud_turn_on_checks_tenant_access ON memory_cloud_turn_on_checks USING (tenant_id = current_setting('mdx.tenant_id', true));

ALTER TABLE memory_benchmark_imports FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_scale_load_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_cloud_turn_on_checks FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_benchmark_imports_persist_plane ON memory_benchmark_imports;
CREATE POLICY memory_benchmark_imports_persist_plane ON memory_benchmark_imports FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_scale_load_runs_persist_plane ON memory_scale_load_runs;
CREATE POLICY memory_scale_load_runs_persist_plane ON memory_scale_load_runs FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_cloud_turn_on_checks_persist_plane ON memory_cloud_turn_on_checks;
CREATE POLICY memory_cloud_turn_on_checks_persist_plane ON memory_cloud_turn_on_checks FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
