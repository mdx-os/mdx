CREATE TABLE IF NOT EXISTS role_view_declarations (
  role_view_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL REFERENCES tenants(tenant_id),
  role_name TEXT NOT NULL,
  declared_sources TEXT[] NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE role_view_declarations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS role_view_declarations_tenant_access ON role_view_declarations;
CREATE POLICY role_view_declarations_tenant_access ON role_view_declarations
  USING (tenant_id = current_setting('mdx.tenant_id', true));
