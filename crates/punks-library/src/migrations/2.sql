CREATE TABLE analysis_jobs (
  asset_id   INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  analyzer   TEXT NOT NULL,
  version    INTEGER NOT NULL,
  status     TEXT NOT NULL DEFAULT 'pending',
  error      TEXT,
  updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (asset_id, analyzer)
) WITHOUT ROWID;

CREATE TABLE audio_analysis (
  asset_id    INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  analyzer    TEXT NOT NULL,
  version     INTEGER NOT NULL,
  metric      TEXT NOT NULL,
  value       REAL NOT NULL,
  computed_at INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (asset_id, analyzer, metric)
) WITHOUT ROWID;
