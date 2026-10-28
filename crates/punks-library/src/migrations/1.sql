CREATE TABLE assets (
  id            INTEGER PRIMARY KEY,
  uuid          TEXT NOT NULL UNIQUE DEFAULT (lower(hex(randomblob(16)))),
  relative_path TEXT NOT NULL UNIQUE,
  size          INTEGER NOT NULL,
  mtime_ms      INTEGER NOT NULL,
  content_hash  TEXT,
  missing       INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_assets_size_mtime ON assets(size, mtime_ms);
CREATE INDEX idx_assets_hash ON assets(content_hash);

CREATE TABLE tags (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE asset_tags (
  asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (asset_id, tag_id)
) WITHOUT ROWID;

CREATE TABLE fields (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE field_values (
  asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  field_id INTEGER NOT NULL REFERENCES fields(id) ON DELETE CASCADE,
  value    TEXT NOT NULL,
  PRIMARY KEY (asset_id, field_id)
) WITHOUT ROWID;
