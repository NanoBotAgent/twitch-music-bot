-- Deduplicate songs and enforce uniqueness on (source, source_id) so that
-- get_or_create can rely on ON CONFLICT (source, source_id).
DELETE FROM songs a USING songs b
WHERE a.source = b.source
  AND a.source_id = b.source_id
  AND a.ctid > b.ctid;

CREATE UNIQUE INDEX IF NOT EXISTS uq_songs_source_source_id ON songs(source, source_id);
