ALTER TABLE analysis_jobs RENAME TO analysis_jobs_v2_archive;
ALTER TABLE audio_analysis RENAME TO audio_analysis_v2_archive;
CREATE TABLE analysis_jobs (
 asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
 status TEXT NOT NULL DEFAULT 'pending', pipeline_version INTEGER NOT NULL,
 error TEXT, started_at INTEGER, finished_at INTEGER, duration_ms INTEGER,
 updated_at INTEGER NOT NULL DEFAULT (unixepoch())
) WITHOUT ROWID;
CREATE TABLE audio_analysis (
 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
 metric TEXT NOT NULL, value REAL NOT NULL,
 computed_at INTEGER NOT NULL DEFAULT (unixepoch()),
 PRIMARY KEY(asset_id, metric)
) WITHOUT ROWID;
INSERT INTO analysis_jobs(asset_id, status, pipeline_version, updated_at)
 SELECT asset_id, 'pending', 0, max(updated_at) FROM analysis_jobs_v2_archive GROUP BY asset_id;
INSERT INTO audio_analysis(asset_id, metric, value, computed_at)
 SELECT a.asset_id, a.metric, a.value, a.computed_at FROM audio_analysis_v2_archive a
 WHERE NOT EXISTS (
  SELECT 1 FROM audio_analysis_v2_archive b WHERE b.asset_id=a.asset_id AND b.metric=a.metric
   AND (b.computed_at>a.computed_at OR (b.computed_at=a.computed_at AND b.analyzer<a.analyzer))
 );
