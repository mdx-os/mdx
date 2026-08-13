-- RLS v3 (Pages): same-tenant resource isolation enforced by Postgres.
--
-- Tenant membership alone no longer grants read access to a private page or any
-- of its child records. A page is owner-private by default. Team, org, and
-- tenant visibility require explicit audience rows or an approved publication.
-- The ten Pages child tables inherit their parent page's visibility, so a leak
-- cannot slip through a revision, publication, citation, attachment, or search
-- record.
--
-- Required session context (set per request from the trusted boundary, see
-- docs/AUTH-PRODUCTION-BOUNDARY.md and docs/SECURITY-RLS-V3.md):
--   mdx.tenant_id, mdx.actor_id, mdx.actor_kind
--
-- These replace the tenant-only Pages policies from migrations 0009 and 0010.

-- A page has one owner. Existing rows are backfilled from the page's revision
-- author; new rows set the owner explicitly (app_state_export and the writers).
ALTER TABLE pages_documents ADD COLUMN IF NOT EXISTS owner_actor_id TEXT REFERENCES actors(actor_id);

UPDATE pages_documents d
SET owner_actor_id = r.author_id
FROM pages_revisions r
WHERE d.owner_actor_id IS NULL
  AND r.page_id = d.page_id
  AND r.tenant_id = d.tenant_id
  AND r.revision_id = COALESCE(d.current_revision_id, r.revision_id);

CREATE INDEX IF NOT EXISTS pages_documents_owner_actor_id_idx
  ON pages_documents (tenant_id, owner_actor_id);

-- Explicit sharing. A page is shared by inserting an audience row; sharing is
-- revoked by setting revoked_at. audience_kind USER names a single actor (also
-- the only way an agent ever reads a page: explicit delegation). TEAM and ORG
-- match a tenant membership scope. TENANT grants company-wide read.
CREATE TABLE IF NOT EXISTS pages_document_audiences (
  audience_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  page_id TEXT NOT NULL REFERENCES pages_documents(page_id),
  audience_kind TEXT NOT NULL CHECK (audience_kind IN ('USER', 'TEAM', 'ORG', 'TENANT')),
  audience_ref TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'reader',
  granted_by TEXT NOT NULL REFERENCES actors(actor_id),
  revoked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS pages_document_audiences_page_idx
  ON pages_document_audiences (tenant_id, page_id) WHERE revoked_at IS NULL;

-- The visibility decision, computed once. SECURITY DEFINER so it can read the
-- ownership, audience, membership, and publication tables without re-triggering
-- the page policy (which calls this function) and recursing. It reads the actor
-- context from the request-local session settings.
CREATE OR REPLACE FUNCTION mdx_pages_page_visible(p_page_id TEXT)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public
AS $$
  SELECT EXISTS (
    SELECT 1
    FROM pages_documents d
    WHERE d.page_id = p_page_id
      AND d.tenant_id = current_setting('mdx.tenant_id', true)
      AND (
        -- The owner always reaches their own page.
        d.owner_actor_id = current_setting('mdx.actor_id', true)
        -- An explicit per-actor grant (the only path for an agent).
        OR EXISTS (
          SELECT 1 FROM pages_document_audiences a
          WHERE a.page_id = d.page_id
            AND a.tenant_id = d.tenant_id
            AND a.revoked_at IS NULL
            AND a.audience_kind = 'USER'
            AND a.audience_ref = current_setting('mdx.actor_id', true)
        )
        -- Humans additionally reach team/org/tenant shares and published pages.
        -- Agents never do: tenant membership is not delegation.
        OR (
          current_setting('mdx.actor_kind', true) IS DISTINCT FROM 'agent'
          AND (
            EXISTS (
              SELECT 1 FROM pages_document_audiences a
              WHERE a.page_id = d.page_id
                AND a.tenant_id = d.tenant_id
                AND a.revoked_at IS NULL
                AND a.audience_kind = 'TENANT'
            )
            OR EXISTS (
              SELECT 1
              FROM pages_document_audiences a
              JOIN auth_tenant_memberships m
                ON m.tenant_id = d.tenant_id
               AND m.actor_id = current_setting('mdx.actor_id', true)
               AND m.tenant_membership_scope = a.audience_ref
               AND m.membership_state = 'ACTIVE'
              WHERE a.page_id = d.page_id
                AND a.tenant_id = d.tenant_id
                AND a.revoked_at IS NULL
                AND a.audience_kind IN ('TEAM', 'ORG')
            )
            OR EXISTS (
              SELECT 1 FROM pages_publications p
              WHERE p.page_id = d.page_id
                AND p.tenant_id = d.tenant_id
                AND p.publication_state = 'PUBLISHED'
            )
          )
        )
      )
  );
$$;

-- The audience table itself is visible only to those who can see the page.
ALTER TABLE pages_document_audiences ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS pages_document_audiences_tenant_access ON pages_document_audiences;
CREATE POLICY pages_document_audiences_tenant_access ON pages_document_audiences
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

-- The page itself: tenant plus the resource-aware visibility decision.
DROP POLICY IF EXISTS pages_documents_tenant_access ON pages_documents;
CREATE POLICY pages_documents_tenant_access ON pages_documents
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

-- Child tables keyed directly by page_id inherit the page decision.
DROP POLICY IF EXISTS pages_revisions_tenant_access ON pages_revisions;
CREATE POLICY pages_revisions_tenant_access ON pages_revisions
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

DROP POLICY IF EXISTS pages_publications_tenant_access ON pages_publications;
CREATE POLICY pages_publications_tenant_access ON pages_publications
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

DROP POLICY IF EXISTS pages_approval_requests_tenant_access ON pages_approval_requests;
CREATE POLICY pages_approval_requests_tenant_access ON pages_approval_requests
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

DROP POLICY IF EXISTS pages_search_preflights_tenant_access ON pages_search_preflights;
CREATE POLICY pages_search_preflights_tenant_access ON pages_search_preflights
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

DROP POLICY IF EXISTS pages_citations_tenant_access ON pages_citations;
CREATE POLICY pages_citations_tenant_access ON pages_citations
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

DROP POLICY IF EXISTS pages_attachments_tenant_access ON pages_attachments;
CREATE POLICY pages_attachments_tenant_access ON pages_attachments
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

DROP POLICY IF EXISTS pages_search_index_records_tenant_access ON pages_search_index_records;
CREATE POLICY pages_search_index_records_tenant_access ON pages_search_index_records
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND mdx_pages_page_visible(page_id)
  );

-- Child tables keyed by revision or publication inherit through their parent,
-- whose own policy enforces page visibility.
DROP POLICY IF EXISTS pages_revision_citations_tenant_access ON pages_revision_citations;
CREATE POLICY pages_revision_citations_tenant_access ON pages_revision_citations
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND EXISTS (
      SELECT 1 FROM pages_revisions r
      WHERE r.revision_id = pages_revision_citations.revision_id
        AND r.tenant_id = current_setting('mdx.tenant_id', true)
    )
  );

DROP POLICY IF EXISTS pages_publication_targets_tenant_access ON pages_publication_targets;
CREATE POLICY pages_publication_targets_tenant_access ON pages_publication_targets
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND EXISTS (
      SELECT 1 FROM pages_publications p
      WHERE p.publication_id = pages_publication_targets.publication_id
        AND p.tenant_id = current_setting('mdx.tenant_id', true)
    )
  );
