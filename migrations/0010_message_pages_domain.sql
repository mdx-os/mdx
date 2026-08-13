CREATE TABLE IF NOT EXISTS message_channels (
  channel_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  name TEXT NOT NULL,
  channel_kind TEXT NOT NULL CHECK (channel_kind IN ('LOCAL_OPS', 'TEAM', 'DM', 'SYSTEM')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_channels ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_channels_tenant_access ON message_channels;
CREATE POLICY message_channels_tenant_access ON message_channels
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS message_channels_tenant_idx ON message_channels (tenant_id, channel_kind);

CREATE TABLE IF NOT EXISTS message_channel_members (
  membership_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  channel_id TEXT NOT NULL REFERENCES message_channels(channel_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  member_role TEXT NOT NULL CHECK (member_role IN ('OWNER', 'MEMBER', 'OBSERVER')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_channel_members ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_channel_members_tenant_access ON message_channel_members;
CREATE POLICY message_channel_members_tenant_access ON message_channel_members
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS message_channel_members_channel_idx ON message_channel_members (tenant_id, channel_id, actor_id);

CREATE TABLE IF NOT EXISTS message_thread_participants (
  participant_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  thread_id TEXT NOT NULL REFERENCES message_threads(thread_id),
  actor_id TEXT NOT NULL REFERENCES actors(actor_id),
  participant_role TEXT NOT NULL CHECK (participant_role IN ('AUTHOR', 'MENTIONED', 'WATCHING')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_thread_participants ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_thread_participants_tenant_access ON message_thread_participants;
CREATE POLICY message_thread_participants_tenant_access ON message_thread_participants
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS message_thread_participants_thread_idx ON message_thread_participants (tenant_id, thread_id, actor_id);

CREATE TABLE IF NOT EXISTS message_envelopes (
  envelope_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  message_id TEXT NOT NULL REFERENCES message_thread_messages(message_id),
  thread_id TEXT NOT NULL REFERENCES message_threads(thread_id),
  channel_id TEXT NOT NULL REFERENCES message_channels(channel_id),
  envelope_state TEXT NOT NULL CHECK (envelope_state IN ('RECORDED', 'FANOUT_BLOCKED', 'DELIVERED')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE message_envelopes ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS message_envelopes_tenant_access ON message_envelopes;
CREATE POLICY message_envelopes_tenant_access ON message_envelopes
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS message_envelopes_thread_idx ON message_envelopes (tenant_id, thread_id, channel_id);

CREATE TABLE IF NOT EXISTS pages_publication_targets (
  publication_target_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  publication_id TEXT NOT NULL REFERENCES pages_publications(publication_id),
  target_scope TEXT NOT NULL CHECK (target_scope IN ('TENANT_ONLY', 'TEAM', 'PUBLIC_BLOCKED')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_publication_targets ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_publication_targets_tenant_access ON pages_publication_targets;
CREATE POLICY pages_publication_targets_tenant_access ON pages_publication_targets
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS pages_publication_targets_publication_idx ON pages_publication_targets (tenant_id, publication_id, target_scope);

CREATE TABLE IF NOT EXISTS pages_revision_citations (
  revision_citation_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  revision_id TEXT NOT NULL REFERENCES pages_revisions(revision_id),
  citation_handle TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_ref TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_revision_citations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_revision_citations_tenant_access ON pages_revision_citations;
CREATE POLICY pages_revision_citations_tenant_access ON pages_revision_citations
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS pages_revision_citations_revision_idx ON pages_revision_citations (tenant_id, revision_id, citation_handle);

CREATE TABLE IF NOT EXISTS pages_attachments (
  attachment_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  revision_id TEXT NOT NULL REFERENCES pages_revisions(revision_id),
  attachment_policy_id TEXT NOT NULL,
  attachment_state TEXT NOT NULL CHECK (attachment_state IN ('POLICY_RECORDED', 'ACCESS_BLOCKED', 'AVAILABLE')),
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_attachments ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_attachments_tenant_access ON pages_attachments;
CREATE POLICY pages_attachments_tenant_access ON pages_attachments
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS pages_attachments_revision_idx ON pages_attachments (tenant_id, revision_id, attachment_policy_id);

CREATE TABLE IF NOT EXISTS pages_search_index_records (
  search_index_record_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  revision_id TEXT NOT NULL REFERENCES pages_revisions(revision_id),
  search_preflight_id TEXT NOT NULL REFERENCES pages_search_preflights(search_preflight_id),
  indexing_state TEXT NOT NULL CHECK (indexing_state IN ('PREFLIGHT_RECORDED', 'INDEXING_BLOCKED', 'INDEXED')),
  embedding_provider TEXT NOT NULL,
  source_receipt_id TEXT NOT NULL REFERENCES ledger_entries(receipt_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pages_search_index_records ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS pages_search_index_records_tenant_access ON pages_search_index_records;
CREATE POLICY pages_search_index_records_tenant_access ON pages_search_index_records
  USING (tenant_id = current_setting('mdx.tenant_id', true));

CREATE INDEX IF NOT EXISTS pages_search_index_records_page_idx ON pages_search_index_records (tenant_id, page_id, indexing_state);
