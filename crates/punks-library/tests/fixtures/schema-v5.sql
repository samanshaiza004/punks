CREATE TABLE fact_overrides (
  asset_id   INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  metric     TEXT NOT NULL,                  -- 'instrument' | 'key' | 'bpm'
  real_value REAL,
  text_value TEXT,
  source     TEXT NOT NULL DEFAULT 'user',   -- always 'user' today; forward-compat provenance
  updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (asset_id, metric),
  CHECK ((real_value IS NOT NULL) + (text_value IS NOT NULL) = 1)
) WITHOUT ROWID;
