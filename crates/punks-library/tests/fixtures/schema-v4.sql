DROP TABLE IF EXISTS audio_analysis;

CREATE TABLE audio_analysis (
  asset_id    INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  metric      TEXT NOT NULL,
  real_value  REAL,
  text_value  TEXT,
  blob_value  BLOB,
  computed_at INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (asset_id, metric),
  CHECK ((real_value IS NOT NULL) + (text_value IS NOT NULL) + (blob_value IS NOT NULL) = 1)
) WITHOUT ROWID;
