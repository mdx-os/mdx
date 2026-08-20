-- Forge records a verified no-change run as a real learning outcome. Keep the
-- durable constraint aligned with the kernel so those receipts cannot stall
-- the entire app-state export transaction.
ALTER TABLE forge_outcome_signals
  DROP CONSTRAINT IF EXISTS forge_outcome_signals_disposition_check;

ALTER TABLE forge_outcome_signals
  ADD CONSTRAINT forge_outcome_signals_disposition_check CHECK (
    disposition IN (
      'completed',
      'no_change',
      'blocked',
      'failed',
      'stopped',
      'budget_exhausted',
      'escalated',
      'approved',
      'declined',
      'revised'
    )
  );
