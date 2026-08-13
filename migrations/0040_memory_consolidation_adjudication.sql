-- Real consolidation: adjudicated decisions, bi-temporal validity windows,
-- and distinct-actor ratification state for shared-scope memory.
--
-- valid_until_receipt_timestamp and invalidated_by_receipt_id stay empty
-- until a later consolidation or lifecycle sweep closes the record; both
-- values come from receipts whose trusted time is inside the signed hash.
-- consolidation_state is 'pending_ratification' for shared-scope writes
-- until a distinct actor clears them via memory.consolidation.ratified.

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS valid_until_receipt_timestamp TEXT NOT NULL DEFAULT '';

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS invalidated_by_receipt_id TEXT NOT NULL DEFAULT '';

ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS consolidation_state TEXT NOT NULL DEFAULT 'active'
  CHECK (consolidation_state IN ('active', 'pending_ratification', 'ratification_declined'));

CREATE INDEX IF NOT EXISTS memory_records_tenant_consolidation_state_idx
  ON memory_records (tenant_id, consolidation_state);
