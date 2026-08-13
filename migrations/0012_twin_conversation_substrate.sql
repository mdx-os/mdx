CREATE TABLE IF NOT EXISTS twin_conversation_sessions (
  twin_conversation_session_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  companion_id TEXT NOT NULL,
  companion_stance TEXT NOT NULL,
  persona_profile_id TEXT NOT NULL,
  prompt_shape TEXT NOT NULL,
  conversation_state TEXT NOT NULL CHECK (conversation_state IN ('LOCAL_GROUNDED', 'PROVIDER_BLOCKED')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_conversation_sessions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_conversation_sessions_tenant_access ON twin_conversation_sessions;
CREATE POLICY twin_conversation_sessions_tenant_access ON twin_conversation_sessions
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_companion_stance_records (
  twin_companion_stance_record_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  companion_id TEXT NOT NULL,
  companion_stance TEXT NOT NULL,
  persona_profile_id TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_companion_stance_records ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_companion_stance_records_tenant_access ON twin_companion_stance_records;
CREATE POLICY twin_companion_stance_records_tenant_access ON twin_companion_stance_records
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_memory_retrievals (
  twin_memory_retrieval_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  memory_id TEXT REFERENCES memory_records(memory_id),
  retrieval_driver TEXT NOT NULL,
  retrieval_scope TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  provider_call_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_memory_retrievals ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_memory_retrievals_tenant_access ON twin_memory_retrievals;
CREATE POLICY twin_memory_retrievals_tenant_access ON twin_memory_retrievals
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_grounded_answers (
  twin_grounded_answer_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  answer_text TEXT NOT NULL,
  model_gateway_driver TEXT NOT NULL,
  model_gateway_provider TEXT NOT NULL,
  model_gateway_model_id TEXT NOT NULL,
  model_gateway_routing TEXT NOT NULL,
  model_gateway_inference_id TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  provider_call_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_grounded_answers ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_grounded_answers_tenant_access ON twin_grounded_answers;
CREATE POLICY twin_grounded_answers_tenant_access ON twin_grounded_answers
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_conversation_summaries (
  twin_conversation_summary_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  summary_text TEXT NOT NULL,
  summary_state TEXT NOT NULL CHECK (summary_state IN ('LOCAL_ONLY', 'PROVIDER_BLOCKED')),
  message_count INTEGER NOT NULL,
  memory_reference_count INTEGER NOT NULL,
  model_trace_count INTEGER NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  production_write_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_conversation_summaries ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_conversation_summaries_tenant_access ON twin_conversation_summaries;
CREATE POLICY twin_conversation_summaries_tenant_access ON twin_conversation_summaries
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS twin_conversation_sessions_session_idx ON twin_conversation_sessions(tenant_id, session_id, created_at);
CREATE INDEX IF NOT EXISTS twin_companion_stance_records_session_idx ON twin_companion_stance_records(tenant_id, session_id, created_at);
CREATE INDEX IF NOT EXISTS twin_memory_retrievals_session_idx ON twin_memory_retrievals(tenant_id, session_id, created_at);
CREATE INDEX IF NOT EXISTS twin_grounded_answers_session_idx ON twin_grounded_answers(tenant_id, session_id, created_at);
CREATE INDEX IF NOT EXISTS twin_conversation_summaries_session_idx ON twin_conversation_summaries(tenant_id, session_id, created_at);
