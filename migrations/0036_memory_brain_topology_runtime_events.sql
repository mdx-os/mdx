CREATE TABLE IF NOT EXISTS memory_topology_runtime_events (
    topology_event_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    service TEXT NOT NULL,
    event_kind TEXT NOT NULL,
    queue_or_cache TEXT NOT NULL,
    cache_key TEXT NOT NULL,
    observed_latency_ms INTEGER NOT NULL,
    receipt_id TEXT NOT NULL
);

ALTER TABLE memory_topology_runtime_events ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_topology_runtime_events_tenant_access ON memory_topology_runtime_events;
CREATE POLICY memory_topology_runtime_events_tenant_access ON memory_topology_runtime_events USING (tenant_id = current_setting('mdx.tenant_id', true));

ALTER TABLE memory_topology_runtime_events FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS memory_topology_runtime_events_persist_plane ON memory_topology_runtime_events;
CREATE POLICY memory_topology_runtime_events_persist_plane ON memory_topology_runtime_events FOR ALL TO mdx_persist USING (true) WITH CHECK (true);
