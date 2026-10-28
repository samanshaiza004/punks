DROP TABLE IF EXISTS fact_overrides_v5;

ALTER TABLE fact_overrides RENAME TO fact_overrides_v5;

CREATE TABLE fact_overrides (
  asset_id   INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  metric     TEXT NOT NULL,
  real_value REAL,
  text_value TEXT,
  is_absent  INTEGER NOT NULL DEFAULT 0,     -- 1 = explicitly "no value", not just uncorrected
  source     TEXT NOT NULL DEFAULT 'user',
  updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (asset_id, metric),
  CHECK (
    (is_absent = 1 AND real_value IS NULL AND text_value IS NULL)
    OR (is_absent = 0 AND (real_value IS NOT NULL) + (text_value IS NOT NULL) = 1)
  )
) WITHOUT ROWID;

INSERT INTO fact_overrides(asset_id, metric, real_value, text_value, is_absent, source, updated_at)
  SELECT asset_id, metric, real_value, text_value, 0, source, updated_at FROM fact_overrides_v5;

DROP TABLE fact_overrides_v5;
