ALTER TABLE ledger_entries
  ADD COLUMN IF NOT EXISTS receipt_timestamp TEXT;

ALTER TABLE ledger_entries
  ADD COLUMN IF NOT EXISTS hash_version INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS ledger_entries_tenant_receipt_timestamp_idx
  ON ledger_entries (tenant_id, receipt_timestamp DESC)
  WHERE receipt_timestamp IS NOT NULL AND receipt_timestamp <> '';
