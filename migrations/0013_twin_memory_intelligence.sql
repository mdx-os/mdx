ALTER TABLE twin_memory_retrievals
  ADD COLUMN IF NOT EXISTS memory_relevance_score INTEGER NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS memory_decay_state TEXT NOT NULL DEFAULT 'fresh_session_memory',
  ADD COLUMN IF NOT EXISTS memory_decay_policy TEXT NOT NULL DEFAULT 'local_recent_session_decay_v1',
  ADD COLUMN IF NOT EXISTS world_model_source TEXT NOT NULL DEFAULT 'generated/world-model/pages-projection-fixtures.json';

ALTER TABLE twin_grounded_answers
  ADD COLUMN IF NOT EXISTS persona_contract_status TEXT NOT NULL DEFAULT 'MATCHED_DECLARED_STANCE',
  ADD COLUMN IF NOT EXISTS voice_drift_status TEXT NOT NULL DEFAULT 'IN_BOUNDS',
  ADD COLUMN IF NOT EXISTS voice_drift_score INTEGER NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS world_model_source TEXT NOT NULL DEFAULT 'generated/world-model/pages-projection-fixtures.json';

ALTER TABLE twin_conversation_summaries
  ADD COLUMN IF NOT EXISTS compaction_policy TEXT NOT NULL DEFAULT 'local_session_summary_v1',
  ADD COLUMN IF NOT EXISTS compaction_state TEXT NOT NULL DEFAULT 'COMPACTED_LOCAL',
  ADD COLUMN IF NOT EXISTS world_model_source_count INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS twin_memory_retrievals_relevance_idx ON twin_memory_retrievals(tenant_id, session_id, memory_relevance_score DESC, created_at);
CREATE INDEX IF NOT EXISTS twin_grounded_answers_drift_idx ON twin_grounded_answers(tenant_id, session_id, voice_drift_status, created_at);
