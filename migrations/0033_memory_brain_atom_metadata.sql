ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS atom_origin TEXT NOT NULL DEFAULT 'asserted'
  CHECK (atom_origin IN ('derived', 'asserted', 'external'));

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS valid_from_receipt_timestamp TEXT;

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS memory_scope TEXT NOT NULL DEFAULT 'private_user_memory';

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS memory_tier TEXT NOT NULL DEFAULT 'episodic';

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS decay_policy TEXT NOT NULL DEFAULT 'local_recent_session_decay_v1';

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS importance_score INTEGER NOT NULL DEFAULT 70
  CHECK (importance_score >= 0 AND importance_score <= 100);

CREATE INDEX IF NOT EXISTS memory_records_tenant_scope_tier_idx
  ON memory_records (tenant_id, memory_scope, memory_tier);

CREATE INDEX IF NOT EXISTS memory_records_tenant_origin_idx
  ON memory_records (tenant_id, atom_origin);
