-- Arc 5 semantic retrieval: memory records may carry a LOCAL embedding
-- vector so recall can score real cosine similarity instead of the byte-sum
-- checksum proxy. The vector is generated only by a boundary-locked local or
-- sovereign embedder (memory content is the most sensitive class and never
-- meets a frontier provider); an empty column is the honest fail-closed
-- default and recall falls back to the lexical and checksum components.
--
-- Stored as text (comma-separated f32, six decimals) so the durable shape
-- matches the kernel's lossless string serialization and the SQL export.
ALTER TABLE memory_records
  ADD COLUMN IF NOT EXISTS embedding text NOT NULL DEFAULT '';
