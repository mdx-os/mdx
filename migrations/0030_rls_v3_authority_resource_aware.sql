-- RLS v3 (Authority surfaces): same-tenant resource isolation for the high
-- authority surfaces. Tenant membership alone no longer reads another sponsor's
-- Talent lease, another owner's Product or Strategy draft, the Observatory
-- operational snapshots, or another user's tenant membership row.
--
-- These tables carry no owner column; ownership derives from the source
-- receipt's creator. mdx_receipt_creator() reads that creator with RLS bypassed
-- (SECURITY DEFINER) so a policy can compare it to the request actor. Operator
-- and admin access is by role from the request context.
--
-- Charter is deliberately NOT included: charter_records is tenant-visible
-- governance content by design, and per-section restriction is a future product
-- feature, not a same-tenant leak. See docs/SECURITY-RLS-V3.md.
--
-- Required session context: mdx.tenant_id, mdx.actor_id, mdx.actor_role.

-- The creator (actor_id) of a receipt, read with RLS bypassed so a policy can
-- derive ownership for a table that only references the source receipt.
CREATE OR REPLACE FUNCTION mdx_receipt_creator(p_receipt_id TEXT)
RETURNS TEXT
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public
AS $$
  SELECT actor_id FROM ledger_entries WHERE receipt_id = p_receipt_id;
$$;

-- Talent: the sponsor (receipt creator), or a governed operator/admin. Never
-- tenant-wide by default.
DROP POLICY IF EXISTS talent_worker_lease_authorities_tenant_access ON talent_worker_lease_authorities;
CREATE POLICY talent_worker_lease_authorities_tenant_access ON talent_worker_lease_authorities
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND (
      mdx_receipt_creator(source_receipt_id) = current_setting('mdx.actor_id', true)
      OR current_setting('mdx.actor_role', true) IN ('tenant_admin', 'tenant_owner', 'security_operator')
    )
  );

-- Product: the draft owner (receipt creator) or a tenant admin. Tenant-wide
-- expansion after ratification is a later refinement; owner/admin is the safe
-- default so a tenant member cannot read another team's draft.
DROP POLICY IF EXISTS product_bet_drafts_tenant_access ON product_bet_drafts;
CREATE POLICY product_bet_drafts_tenant_access ON product_bet_drafts
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND (
      mdx_receipt_creator(source_receipt_id) = current_setting('mdx.actor_id', true)
      OR current_setting('mdx.actor_role', true) IN ('tenant_admin', 'tenant_owner')
    )
  );

-- Strategy: the draft owner (receipt creator) or a tenant admin, same posture as
-- Product. Tenant-wide visibility after ratification is a later refinement.
DROP POLICY IF EXISTS strategy_ratification_snapshots_tenant_access ON strategy_ratification_snapshots;
CREATE POLICY strategy_ratification_snapshots_tenant_access ON strategy_ratification_snapshots
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND (
      mdx_receipt_creator(source_receipt_id) = current_setting('mdx.actor_id', true)
      OR current_setting('mdx.actor_role', true) IN ('tenant_admin', 'tenant_owner')
    )
  );

-- Observatory: operator, security, and admin roles only. It carries operational
-- posture, never private content, and is not creator-scoped.
DROP POLICY IF EXISTS observatory_role_view_snapshots_tenant_access ON observatory_role_view_snapshots;
CREATE POLICY observatory_role_view_snapshots_tenant_access ON observatory_role_view_snapshots
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND current_setting('mdx.actor_role', true) IN ('tenant_admin', 'tenant_owner', 'security_operator', 'workspace_admin')
  );

-- Admin memberships: a user sees their own membership row; a tenant admin manages
-- all. Managing membership is not a window into private user content.
DROP POLICY IF EXISTS auth_tenant_memberships_tenant_access ON auth_tenant_memberships;
CREATE POLICY auth_tenant_memberships_tenant_access ON auth_tenant_memberships
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND (
      actor_id = current_setting('mdx.actor_id', true)
      OR current_setting('mdx.actor_role', true) IN ('tenant_admin', 'tenant_owner')
    )
  );
