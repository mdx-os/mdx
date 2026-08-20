-- Canonical restore follows the declared head backward by hash. These indexes
-- keep reconciliation bounded without altering or deleting historical rows.
CREATE INDEX IF NOT EXISTS ledger_entries_hash_idx
  ON ledger_entries (hash);

CREATE INDEX IF NOT EXISTS ledger_entries_previous_hash_idx
  ON ledger_entries (previous_hash)
  WHERE previous_hash IS NOT NULL;
