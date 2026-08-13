-- RLS v2: resource-aware access for user-private surfaces.
--
-- Tenant membership alone no longer grants access to private resources. These
-- policies add actor and resource context on top of the tenant predicate, so a
-- user in the same tenant cannot read another user's Twin, a non-participant
-- cannot read a Message thread, and a non-owner cannot read a Forge build.
--
-- Required session context (set per request from the trusted boundary, see
-- docs/AUTH-PRODUCTION-BOUNDARY.md and docs/SECURITY-RLS-V2.md):
--   mdx.tenant_id, mdx.actor_id
--
-- These replace the tenant-only policies from earlier migrations. They are the
-- authoritative policy for these tables; the structural migration contract
-- counts both the original and the override and expects this exact set.

-- Twin is user-private: only the owning actor, never the tenant at large.
DROP POLICY IF EXISTS twin_sessions_tenant_access ON twin_sessions;
CREATE POLICY twin_sessions_tenant_access ON twin_sessions
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND actor_id = current_setting('mdx.actor_id', true)
  );

DROP POLICY IF EXISTS twin_session_messages_tenant_access ON twin_session_messages;
CREATE POLICY twin_session_messages_tenant_access ON twin_session_messages
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND actor_id = current_setting('mdx.actor_id', true)
  );

-- Message threads and their messages are participant-scoped: you must be a
-- participant of the thread, not merely in the tenant.
DROP POLICY IF EXISTS message_threads_tenant_access ON message_threads;
CREATE POLICY message_threads_tenant_access ON message_threads
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND EXISTS (
      SELECT 1 FROM message_thread_participants p
      WHERE p.thread_id = message_threads.thread_id
        AND p.tenant_id = current_setting('mdx.tenant_id', true)
        AND p.actor_id = current_setting('mdx.actor_id', true)
    )
  );

DROP POLICY IF EXISTS message_thread_messages_tenant_access ON message_thread_messages;
CREATE POLICY message_thread_messages_tenant_access ON message_thread_messages
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND EXISTS (
      SELECT 1 FROM message_thread_participants p
      WHERE p.thread_id = message_thread_messages.thread_id
        AND p.tenant_id = current_setting('mdx.tenant_id', true)
        AND p.actor_id = current_setting('mdx.actor_id', true)
    )
  );

-- A participant row is visible to that participant only.
DROP POLICY IF EXISTS message_thread_participants_tenant_access ON message_thread_participants;
CREATE POLICY message_thread_participants_tenant_access ON message_thread_participants
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND actor_id = current_setting('mdx.actor_id', true)
  );

-- Forge build requests are owner-scoped. Collaborator sharing is a later
-- contract; owner-only is the safe default until it exists.
DROP POLICY IF EXISTS forge_build_requests_tenant_access ON forge_build_requests;
CREATE POLICY forge_build_requests_tenant_access ON forge_build_requests
  USING (
    tenant_id = current_setting('mdx.tenant_id', true)
    AND actor_id = current_setting('mdx.actor_id', true)
  );
