CREATE TABLE IF NOT EXISTS memory_graph_nodes (
  node_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  node_kind TEXT NOT NULL,
  label TEXT NOT NULL,
  memory_id TEXT,
  source_receipt_id TEXT NOT NULL,
  atom_origin TEXT NOT NULL,
  valid_from_receipt_timestamp TEXT NOT NULL,
  lifecycle_state TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_graph_edges (
  edge_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  from_node_id TEXT NOT NULL,
  to_node_id TEXT NOT NULL,
  edge_kind TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL,
  weight INTEGER NOT NULL,
  valid_from_receipt_timestamp TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_lifecycle_events (
  event_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  memory_id TEXT NOT NULL,
  action TEXT NOT NULL,
  lifecycle_state TEXT NOT NULL,
  reason TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL,
  valid_from_receipt_timestamp TEXT NOT NULL,
  receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_recall_rankings (
  ranking_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  surface TEXT NOT NULL,
  query TEXT NOT NULL,
  memory_id TEXT NOT NULL,
  lexical_score INTEGER NOT NULL,
  vector_score INTEGER NOT NULL,
  graph_score INTEGER NOT NULL,
  recency_score INTEGER NOT NULL,
  importance_score INTEGER NOT NULL,
  scope_score INTEGER NOT NULL,
  source_authority_score INTEGER NOT NULL,
  final_score INTEGER NOT NULL,
  rank INTEGER NOT NULL,
  source_receipt_id TEXT NOT NULL,
  receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_brain_eval_runs (
  eval_run_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  fixture_family TEXT NOT NULL,
  fixture_count INTEGER NOT NULL,
  correct_count INTEGER NOT NULL,
  latency_budget_ms INTEGER NOT NULL,
  observed_latency_ms INTEGER NOT NULL,
  brain_score INTEGER NOT NULL,
  receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_vendor_comparator_runs (
  comparator_run_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  vendor_id TEXT NOT NULL,
  status TEXT NOT NULL,
  accuracy_score INTEGER NOT NULL,
  latency_ms INTEGER NOT NULL,
  cost_micros INTEGER NOT NULL,
  receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_surface_access (
  access_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  surface TEXT NOT NULL,
  scope TEXT NOT NULL,
  can_read BOOLEAN NOT NULL,
  can_write BOOLEAN NOT NULL,
  review_required BOOLEAN NOT NULL,
  receipt_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_production_topology_checks (
  topology_check_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  service TEXT NOT NULL,
  deployment_shape TEXT NOT NULL,
  latency_role TEXT NOT NULL,
  queue_or_cache TEXT NOT NULL,
  observed_latency_ms INTEGER NOT NULL,
  receipt_id TEXT NOT NULL
);

ALTER TABLE memory_graph_nodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_graph_edges ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_lifecycle_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_recall_rankings ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_brain_eval_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_vendor_comparator_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_surface_access ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_production_topology_checks ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_graph_nodes_tenant_access ON memory_graph_nodes;
CREATE POLICY memory_graph_nodes_tenant_access ON memory_graph_nodes USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_graph_edges_tenant_access ON memory_graph_edges;
CREATE POLICY memory_graph_edges_tenant_access ON memory_graph_edges USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_lifecycle_events_tenant_access ON memory_lifecycle_events;
CREATE POLICY memory_lifecycle_events_tenant_access ON memory_lifecycle_events USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_recall_rankings_tenant_access ON memory_recall_rankings;
CREATE POLICY memory_recall_rankings_tenant_access ON memory_recall_rankings USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_brain_eval_runs_tenant_access ON memory_brain_eval_runs;
CREATE POLICY memory_brain_eval_runs_tenant_access ON memory_brain_eval_runs USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_vendor_comparator_runs_tenant_access ON memory_vendor_comparator_runs;
CREATE POLICY memory_vendor_comparator_runs_tenant_access ON memory_vendor_comparator_runs USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_surface_access_tenant_access ON memory_surface_access;
CREATE POLICY memory_surface_access_tenant_access ON memory_surface_access USING (tenant_id = current_setting('mdx.tenant_id', true));
DROP POLICY IF EXISTS memory_production_topology_checks_tenant_access ON memory_production_topology_checks;
CREATE POLICY memory_production_topology_checks_tenant_access ON memory_production_topology_checks USING (tenant_id = current_setting('mdx.tenant_id', true));

ALTER TABLE memory_graph_nodes FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_graph_edges FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_lifecycle_events FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_recall_rankings FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_brain_eval_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_vendor_comparator_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_surface_access FORCE ROW LEVEL SECURITY;
ALTER TABLE memory_production_topology_checks FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_graph_nodes_persist_plane ON memory_graph_nodes;
CREATE POLICY memory_graph_nodes_persist_plane ON memory_graph_nodes FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_graph_edges_persist_plane ON memory_graph_edges;
CREATE POLICY memory_graph_edges_persist_plane ON memory_graph_edges FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_lifecycle_events_persist_plane ON memory_lifecycle_events;
CREATE POLICY memory_lifecycle_events_persist_plane ON memory_lifecycle_events FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_recall_rankings_persist_plane ON memory_recall_rankings;
CREATE POLICY memory_recall_rankings_persist_plane ON memory_recall_rankings FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_brain_eval_runs_persist_plane ON memory_brain_eval_runs;
CREATE POLICY memory_brain_eval_runs_persist_plane ON memory_brain_eval_runs FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_vendor_comparator_runs_persist_plane ON memory_vendor_comparator_runs;
CREATE POLICY memory_vendor_comparator_runs_persist_plane ON memory_vendor_comparator_runs FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_surface_access_persist_plane ON memory_surface_access;
CREATE POLICY memory_surface_access_persist_plane ON memory_surface_access FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
DROP POLICY IF EXISTS memory_production_topology_checks_persist_plane ON memory_production_topology_checks;
CREATE POLICY memory_production_topology_checks_persist_plane ON memory_production_topology_checks FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
