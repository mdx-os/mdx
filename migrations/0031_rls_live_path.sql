-- RLS live path: named roles for the two access planes, and FORCE so the
-- table owner is bound by row security like everyone else.
--
-- Before this migration the deployment role choice was undefined in-repo: a
-- server connecting as the table owner silently bypassed every policy. This
-- migration encodes the contract:
--
--   mdx_app     - the interactive plane. Every per-request read or write on
--                 behalf of a user runs as a login role granted mdx_app and
--                 is bound by every tenant/actor/resource policy. NOLOGIN
--                 here; a deployment grants LOGIN and a password out of band
--                 (never in a migration).
--
--   mdx_persist - the kernel persistence plane. The serving kernel durably
--                 exports receipts and app state it already authorized
--                 through the HTTP boundary, across all actors in a tenant.
--                 Its write authority is an explicit named policy per table,
--                 visible in pg_policies and revocable in one statement -
--                 not a hidden superuser and not the table owner.
--
-- Deployment contract: the server's DATABASE_URL uses a login role granted
-- mdx_persist. Humans, tools, and future per-request paths use roles granted
-- mdx_app. Nothing connects as the owner or a superuser.
--
-- Future migrations that create row-secured tables must re-apply FORCE and
-- the persist-plane policy for those tables (the dynamic blocks below cover
-- only tables that exist when this migration runs; re-running it is safe).

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'mdx_app') THEN
    CREATE ROLE mdx_app NOLOGIN;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'mdx_persist') THEN
    CREATE ROLE mdx_persist NOLOGIN;
  END IF;
END $$;

GRANT USAGE ON SCHEMA public TO mdx_app, mdx_persist;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO mdx_app, mdx_persist;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO mdx_app, mdx_persist;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO mdx_app, mdx_persist;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO mdx_app, mdx_persist;

-- FORCE: the owner is bound too. Only a superuser bypasses row security
-- after this, and production must never connect as one.
DO $$
DECLARE r record;
BEGIN
  FOR r IN SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND rowsecurity LOOP
    EXECUTE format('ALTER TABLE public.%I FORCE ROW LEVEL SECURITY', r.tablename);
  END LOOP;
END $$;

-- The persistence plane's explicit, named write authority: one policy per
-- row-secured table. USING (true) is deliberate and auditable - this role
-- exists to write the kernel's already-authorized state for every actor.
DO $$
DECLARE r record;
BEGIN
  FOR r IN SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND rowsecurity LOOP
    EXECUTE format('DROP POLICY IF EXISTS %I ON public.%I', r.tablename || '_persist_plane', r.tablename);
    EXECUTE format('CREATE POLICY %I ON public.%I FOR ALL TO mdx_persist USING (true) WITH CHECK (true)', r.tablename || '_persist_plane', r.tablename);
  END LOOP;
END $$;
