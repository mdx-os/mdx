CREATE TABLE IF NOT EXISTS ctx_sessions (
  session_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL,
  status TEXT NOT NULL,
  metadata JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  closed_at TIMESTAMPTZ
);
ALTER TABLE ctx_sessions ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS ctx_sessions_tenant_access ON ctx_sessions;
CREATE POLICY ctx_sessions_tenant_access ON ctx_sessions
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS ctx_sessions_tenant_status_idx
  ON ctx_sessions(tenant_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS ctx_session_state (
  state_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES ctx_sessions(session_id),
  state_key TEXT NOT NULL,
  state_value JSONB NOT NULL,
  agent_id TEXT NOT NULL,
  version INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, session_id, state_key)
);
ALTER TABLE ctx_session_state ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS ctx_session_state_tenant_access ON ctx_session_state;
CREATE POLICY ctx_session_state_tenant_access ON ctx_session_state
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS ctx_session_state_session_idx
  ON ctx_session_state(tenant_id, session_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS ctx_handoffs (
  handoff_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES ctx_sessions(session_id),
  from_agent TEXT NOT NULL,
  to_agent TEXT NOT NULL,
  reason TEXT NOT NULL,
  context_snapshot JSONB NOT NULL,
  conversation_summary TEXT NOT NULL,
  open_questions JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE ctx_handoffs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS ctx_handoffs_tenant_access ON ctx_handoffs;
CREATE POLICY ctx_handoffs_tenant_access ON ctx_handoffs
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS ctx_handoffs_session_chain_idx
  ON ctx_handoffs(tenant_id, session_id, created_at ASC);
CREATE INDEX IF NOT EXISTS ctx_handoffs_to_agent_idx
  ON ctx_handoffs(tenant_id, to_agent, created_at DESC);

CREATE TABLE IF NOT EXISTS ctx_background_jobs (
  job_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL,
  job_type TEXT NOT NULL,
  job_runner_driver TEXT NOT NULL,
  job_runner_provider TEXT NOT NULL,
  status TEXT NOT NULL,
  terminal_state TEXT NOT NULL,
  session_id TEXT REFERENCES ctx_sessions(session_id),
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ
);
ALTER TABLE ctx_background_jobs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS ctx_background_jobs_tenant_access ON ctx_background_jobs;
CREATE POLICY ctx_background_jobs_tenant_access ON ctx_background_jobs
  USING (tenant_id = current_setting('mdx.tenant_id', true));
CREATE INDEX IF NOT EXISTS ctx_background_jobs_tenant_status_idx
  ON ctx_background_jobs(tenant_id, status, created_at DESC);
