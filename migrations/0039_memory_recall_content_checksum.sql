-- The recall ranking component previously labeled vector_score is a
-- deterministic byte-sum checksum, not an embedding distance. Rename the
-- column so the stored label matches what the kernel actually computes.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'memory_recall_rankings'
      AND column_name = 'vector_score'
  ) THEN
    ALTER TABLE memory_recall_rankings
      RENAME COLUMN vector_score TO content_checksum_score;
  END IF;
END $$;
