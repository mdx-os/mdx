-- Marketplace pack actions were added after the original marketplace table.
-- Keep the durable constraint aligned with the kernel's declared action kinds
-- so a valid governed receipt can always be exported to Postgres.
ALTER TABLE marketplace_acts
  DROP CONSTRAINT IF EXISTS marketplace_acts_act_check;

ALTER TABLE marketplace_acts
  ADD CONSTRAINT marketplace_acts_act_check CHECK (
    act IN (
      'install',
      'approval',
      'review',
      'revocation',
      'try',
      'import_candidate',
      'import_scan',
      'pack_install',
      'pack_configure',
      'pack_connect',
      'pack_authorize',
      'pack_enable',
      'pack_disable',
      'pack_update',
      'pack_rollback',
      'pack_rescope',
      'pack_remove',
      'pack_try',
      'pack_use',
      'pack_request',
      'pack_propose'
    )
  );
