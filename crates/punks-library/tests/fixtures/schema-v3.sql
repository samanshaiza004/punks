DROP TABLE IF EXISTS analysis_jobs;
DROP TABLE IF EXISTS audio_analysis;

CREATE TABLE analysis_jobs (
  asset_id         INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  status           TEXT NOT NULL DEFAULT 'pending',  -- pending | running | done | error
  pipeline_version INTEGER NOT NULL,
  error            TEXT,
  started_at       INTEGER,
  finished_at      INTEGER,
  duration_ms      INTEGER,
  updated_at       INTEGER NOT NULL DEFAULT (unixepoch())
) WITHOUT ROWID;

CREATE TABLE audio_analysis (
  asset_id    INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  metric      TEXT NOT NULL,                   -- opaque key, e.g. 'rms' | 'peak' | 'zcr'
  value       REAL NOT NULL,
  computed_at INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (asset_id, metric)
) WITHOUT ROWID;
