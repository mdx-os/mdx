CREATE TABLE IF NOT EXISTS message_threads (
  thread_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  channel_id TEXT NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('OPEN', 'ARCHIVED')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_threads ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_threads_tenant_access ON message_threads;
CREATE POLICY message_threads_tenant_access ON message_threads
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS message_thread_messages (
  message_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  thread_id TEXT NOT NULL REFERENCES message_threads(thread_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  body TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  fanout_status TEXT NOT NULL CHECK (fanout_status IN ('BLOCKED', 'REQUESTED', 'DELIVERED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_thread_messages ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_thread_messages_tenant_access ON message_thread_messages;
CREATE POLICY message_thread_messages_tenant_access ON message_thread_messages
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS message_fanout_requests (
  fanout_request_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  message_id TEXT NOT NULL REFERENCES message_thread_messages(message_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  delivery_authority TEXT NOT NULL CHECK (delivery_authority IN ('BLOCKED', 'APPROVED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_fanout_requests ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_fanout_requests_tenant_access ON message_fanout_requests;
CREATE POLICY message_fanout_requests_tenant_access ON message_fanout_requests
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS message_presence_records (
  presence_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  thread_id TEXT REFERENCES message_threads(thread_id),
  status TEXT NOT NULL CHECK (status IN ('REQUESTED', 'BLOCKED', 'ACTIVE', 'AWAY')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_presence_records ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_presence_records_tenant_access ON message_presence_records;
CREATE POLICY message_presence_records_tenant_access ON message_presence_records
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS pages_documents (
  page_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  slug TEXT NOT NULL,
  title TEXT NOT NULL,
  visibility TEXT NOT NULL CHECK (visibility IN ('PRIVATE', 'TEAM', 'PUBLIC_BLOCKED')),
  current_revision_id TEXT,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_documents ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_documents_tenant_access ON pages_documents;
CREATE POLICY pages_documents_tenant_access ON pages_documents
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS pages_revisions (
  revision_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  author_id TEXT NOT NULL REFERENCES actors(actor_id),
  body TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_revisions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_revisions_tenant_access ON pages_revisions;
CREATE POLICY pages_revisions_tenant_access ON pages_revisions
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS pages_publications (
  publication_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  revision_id TEXT NOT NULL REFERENCES pages_revisions(revision_id),
  publication_state TEXT NOT NULL CHECK (publication_state IN ('RECORDED', 'PUBLICATION_BLOCKED', 'PUBLISHED')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_publications ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_publications_tenant_access ON pages_publications;
CREATE POLICY pages_publications_tenant_access ON pages_publications
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS pages_approval_requests (
  approval_request_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  revision_id TEXT NOT NULL REFERENCES pages_revisions(revision_id),
  requested_by TEXT NOT NULL REFERENCES actors(actor_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  approval_state TEXT NOT NULL CHECK (approval_state IN ('REQUESTED', 'APPROVED', 'DENIED', 'BLOCKED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_approval_requests ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_approval_requests_tenant_access ON pages_approval_requests;
CREATE POLICY pages_approval_requests_tenant_access ON pages_approval_requests
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS pages_search_preflights (
  search_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  indexing_authority TEXT NOT NULL CHECK (indexing_authority IN ('BLOCKED', 'APPROVED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_search_preflights ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_search_preflights_tenant_access ON pages_search_preflights;
CREATE POLICY pages_search_preflights_tenant_access ON pages_search_preflights
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS pages_citations (
  citation_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  target_kind TEXT NOT NULL,
  target_ref TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_citations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_citations_tenant_access ON pages_citations;
CREATE POLICY pages_citations_tenant_access ON pages_citations
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_sessions (
  session_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  stance TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_sessions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_sessions_tenant_access ON twin_sessions;
CREATE POLICY twin_sessions_tenant_access ON twin_sessions
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_session_messages (
  twin_message_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  role TEXT NOT NULL CHECK (role IN ('HUMAN', 'COMPANION', 'SYSTEM')),
  body TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_session_messages ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_session_messages_tenant_access ON twin_session_messages;
CREATE POLICY twin_session_messages_tenant_access ON twin_session_messages
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_memory_snapshots (
  twin_memory_snapshot_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  memory_id TEXT REFERENCES memory_records(memory_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  consolidation_decision TEXT NOT NULL CHECK (consolidation_decision IN ('RETAIN', 'SKIP')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_memory_snapshots ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_memory_snapshots_tenant_access ON twin_memory_snapshots;
CREATE POLICY twin_memory_snapshots_tenant_access ON twin_memory_snapshots
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS twin_model_traces (
  model_trace_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  session_id TEXT NOT NULL REFERENCES twin_sessions(session_id),
  driver_id TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  provider_call_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE twin_model_traces ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS twin_model_traces_tenant_access ON twin_model_traces;
CREATE POLICY twin_model_traces_tenant_access ON twin_model_traces
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS forge_build_requests (
  forge_build_request_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  request_summary TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  execution_authority TEXT NOT NULL CHECK (execution_authority IN ('BLOCKED', 'APPROVED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_build_requests ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_build_requests_tenant_access ON forge_build_requests;
CREATE POLICY forge_build_requests_tenant_access ON forge_build_requests
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS forge_build_approvals (
  forge_build_approval_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  forge_build_request_id TEXT NOT NULL REFERENCES forge_build_requests(forge_build_request_id),
  approved_by TEXT NOT NULL REFERENCES actors(actor_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  approval_state TEXT NOT NULL CHECK (approval_state IN ('RECORDED', 'APPROVED', 'DENIED', 'BLOCKED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_build_approvals ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_build_approvals_tenant_access ON forge_build_approvals;
CREATE POLICY forge_build_approvals_tenant_access ON forge_build_approvals
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS forge_workflow_plan_proofs (
  workflow_plan_proof_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  forge_build_approval_id TEXT NOT NULL REFERENCES forge_build_approvals(forge_build_approval_id),
  workflow_id TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  execution_authority TEXT NOT NULL CHECK (execution_authority IN ('BLOCKED', 'APPROVED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_workflow_plan_proofs ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_workflow_plan_proofs_tenant_access ON forge_workflow_plan_proofs;
CREATE POLICY forge_workflow_plan_proofs_tenant_access ON forge_workflow_plan_proofs
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS forge_worker_authority_requests (
  worker_authority_request_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  workflow_plan_proof_id TEXT NOT NULL REFERENCES forge_workflow_plan_proofs(workflow_plan_proof_id),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  worker_spawn_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_worker_authority_requests ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_worker_authority_requests_tenant_access ON forge_worker_authority_requests;
CREATE POLICY forge_worker_authority_requests_tenant_access ON forge_worker_authority_requests
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE TABLE IF NOT EXISTS forge_preflight_records (
  forge_preflight_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  worker_authority_request_id TEXT REFERENCES forge_worker_authority_requests(worker_authority_request_id),
  preflight_kind TEXT NOT NULL CHECK (preflight_kind IN ('TALENT_AUTHORIZATION', 'WORKER_CREDENTIAL', 'WORKER_SPAWN', 'CI_EVIDENCE', 'HUMAN_RATIFICATION', 'DEPLOYMENT')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  authority_allowed BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE forge_preflight_records ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS forge_preflight_records_tenant_access ON forge_preflight_records;
CREATE POLICY forge_preflight_records_tenant_access ON forge_preflight_records
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS message_thread_messages_thread_idx ON message_thread_messages(tenant_id, thread_id, created_at);
CREATE INDEX IF NOT EXISTS message_presence_records_actor_idx ON message_presence_records(tenant_id, actor_id, created_at);
CREATE INDEX IF NOT EXISTS pages_documents_slug_idx ON pages_documents(tenant_id, slug);
CREATE INDEX IF NOT EXISTS pages_revisions_page_idx ON pages_revisions(tenant_id, page_id, created_at);
CREATE INDEX IF NOT EXISTS pages_citations_page_idx ON pages_citations(tenant_id, page_id);
CREATE INDEX IF NOT EXISTS twin_session_messages_session_idx ON twin_session_messages(tenant_id, session_id, created_at);
CREATE INDEX IF NOT EXISTS twin_model_traces_session_idx ON twin_model_traces(tenant_id, session_id, created_at);
CREATE INDEX IF NOT EXISTS forge_preflight_records_kind_idx ON forge_preflight_records(tenant_id, preflight_kind, created_at);
