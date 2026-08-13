-- RLS v3 (Receipts): a receipt inherits its source resource's visibility, not
-- the tenant's. Same-tenant membership no longer reads the ledger entry behind
-- another user's private Page, Twin session, or Forge build.
--
-- A receipt row in ledger_entries is visible only to:
--   - its creator (actor_id), and
--   - anyone who can see a resource-aware resource that this receipt is the
--     source of (the resource's own RLS decides; tenant-only surfaces are
--     deliberately excluded so a receipt is never inherited into visibility
--     through a surface that is not yet resource-aware).
--
-- Operators do not read raw receipts: audit visibility is the Observatory
-- surface (receipt-grounded aggregates), and a column-restricted receipt-metadata
-- audit view is future work. Nothing here exposes a receipt's payload or its
-- private source content to a same-tenant user.
--
-- Required session context: mdx.tenant_id, mdx.actor_id.

-- Whether the receipt is the source of a resource-aware resource the caller can
-- see. SECURITY INVOKER (the default) so each EXISTS applies the target table's
-- own RLS and the receipt inherits that decision. Only resource-aware containers
-- are checked; a tenant-only surface is never a path to a receipt.
CREATE OR REPLACE FUNCTION mdx_receipt_resource_visible(p_receipt_id TEXT)
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
  SELECT
    EXISTS (SELECT 1 FROM pages_documents t WHERE t.source_receipt_id = p_receipt_id)
    OR EXISTS (SELECT 1 FROM pages_revisions t WHERE t.source_receipt_id = p_receipt_id)
    OR EXISTS (SELECT 1 FROM pages_publications t WHERE t.source_receipt_id = p_receipt_id)
    OR EXISTS (SELECT 1 FROM twin_sessions t WHERE t.source_receipt_id = p_receipt_id)
    OR EXISTS (SELECT 1 FROM twin_session_messages t WHERE t.source_receipt_id = p_receipt_id)
    OR EXISTS (SELECT 1 FROM message_threads t WHERE t.source_receipt_id = p_receipt_id)
    OR EXISTS (SELECT 1 FROM message_thread_messages t WHERE t.source_receipt_id = p_receipt_id)
    OR EXISTS (SELECT 1 FROM forge_build_requests t WHERE t.source_receipt_id = p_receipt_id);
$$;

DROP POLICY IF EXISTS ledger_entries_tenant_access ON ledger_entries;
CREATE POLICY ledger_entries_tenant_access ON ledger_entries
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND (
      actor_id = current_setting('mdx.actor_id', true)
      OR mdx_receipt_resource_visible(receipt_id)
    )
  );
