CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS ctx_memory_vectors (
  vector_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  memory_id TEXT NOT NULL REFERENCES memory_records(memory_id) ON DELETE CASCADE,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  tier TEXT NOT NULL CHECK (tier IN ('working', 'episodic', 'semantic', 'procedural')),
  content TEXT NOT NULL,
  embedding VECTOR(1536) NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE ctx_memory_vectors ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS ctx_memory_vectors_tenant_access ON ctx_memory_vectors;
CREATE POLICY ctx_memory_vectors_tenant_access ON ctx_memory_vectors
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS ctx_memory_vectors_tenant_tier_idx ON ctx_memory_vectors(tenant_id, tier, created_at DESC);
CREATE INDEX IF NOT EXISTS ctx_memory_vectors_embedding_hnsw_idx ON ctx_memory_vectors USING hnsw (embedding vector_cosine_ops);
