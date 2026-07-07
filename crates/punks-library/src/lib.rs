//! Per-root sample library: SQLite-backed storage for user knowledge about
//! audio files (tags, custom fields) plus, later, regeneratable analysis data.
//!
//! Governing principles for every schema/API decision here:
//! - the filesystem stores bytes; the library database stores knowledge;
//! - generated data is always disposable, user data never is;
//! - asset identity survives renames/moves (two-stage reconciliation), so a
//!   user reorganising folders never orphans their tags.
//!
//! One database per library root, at `<root>/.punks/library.db`. Paths are
//! stored relative to the root with `/` separators, so a library root can be
//! relocated (or read on another OS) without a migration.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub enum LibraryError {
    Io(String),
    Db(String),
    NotALibrary(PathBuf),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Io(e) => write!(f, "library io error: {e}"),
            LibraryError::Db(e) => write!(f, "library db error: {e}"),
            LibraryError::NotALibrary(p) => write!(f, "not a library: {}", p.display()),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        LibraryError::Db(e.to_string())
    }
}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

/// A tag plus how many (non-missing) assets carry it. What sidebars want.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub id: i64,
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub id: i64,
    pub relative_path: PathBuf,
    pub size: u64,
    pub mtime_ms: i64,
}

/// A file found on disk during a scan, described cheaply (no hash yet).
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub relative_path: PathBuf,
    pub size: u64,
    pub mtime_ms: i64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub added: usize,
    pub moved: usize,
    pub modified: usize,
    pub missing: usize,
    pub unchanged: usize,
}

/// Set WAL mode, retrying on contention. `busy_timeout` does not cover this
/// specific pragma (see the call site in `open_at`), so this is a manual
/// backoff loop instead: up to 1s total, which comfortably covers the
/// millisecond-scale window where a fresh database's first WAL conversion can
/// briefly contend with another connection doing the same thing.
fn set_wal_mode_with_retry(conn: &Connection) -> Result<(), LibraryError> {
    let mut last_err = None;
    for _ in 0..50 {
        match conn.pragma_update(None, "journal_mode", "WAL") {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
    Err(last_err.unwrap().into())
}

// Migration v1: the Core (user-owned) tables only. Generated tables
// (analysis_jobs, waveforms, audio_analysis, thumbnails) and Cache tables
// (preview_cache) arrive as v2+ migrations when those features land — the
// user_version machinery below is the extension point.
const SCHEMA_V1: &str = "
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
";

// Migration v2: the analysis jobs queue + results (superseded by v3 below).
const SCHEMA_V2: &str = "
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
";

// Migration v3: recast the queue as one row *per asset* (status lifecycle
// pending → running → done/error, plus profiling) and store results as opaque
// (metric, value) pairs. The library is analyzer-agnostic: a single
// `pipeline_version` (owned by the analysis crate) decides staleness; it never
// names an analyzer. Both tables are Generated — a worker can re-run them any
// time — so they cascade away with their asset. Safe DROP+CREATE: nothing wrote
// to the v2 tables (no worker existed).
const SCHEMA_V3: &str = "
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
";

// Migration v4: results become typed *facts* — each row holds exactly one of a
// real, text, or blob value, so analyzers can emit categorical observations
// (filename → instrument/key) beside scalars. `blob_value` is unused today but
// present so fingerprints/embeddings never need another migration. Generated /
// disposable → safe DROP+CREATE (the worker recomputes everything next scan).
const SCHEMA_V4: &str = "
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
";

// Migration v5: user corrections that *patch* a detected fact, resolved
// `user ?? analysis` at read. This is USER data (never regenerated), so it's an
// additive migration that leaves `audio_analysis` untouched — re-running an
// analyzer can't erase an override. Cascades on asset delete like `asset_tags`,
// and survives rename/move via asset identity. (Independently-authored metadata
// — notes/pack/author/license — is a separate `asset_metadata` layer, later.)
const SCHEMA_V5: &str = "
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
";

// Migration v6: `is_absent` lets a user say "this metric genuinely doesn't
// apply" (e.g. an atonal sound has no key, a one-shot has no BPM) — distinct
// from having no override row at all (which falls back to the detected
// guess). `fact_overrides` is USER data, so existing rows must survive: SQLite
// can't ALTER a CHECK constraint in place, so rename/recreate/copy/drop
// instead of the disposable tables' DROP+CREATE. The leading DROP makes this
// self-healing if a `fact_overrides_v5` temp table was ever left behind by an
// earlier, non-transactional version of this same migration racing across
// connections (fixed by wrapping the whole migration in one transaction in
// `open_at`, but this guard costs nothing and closes the gap for good).
const SCHEMA_V6: &str = "
DROP TABLE IF EXISTS fact_overrides_v5;

ALTER TABLE fact_overrides RENAME TO fact_overrides_v5;

CREATE TABLE fact_overrides (
  asset_id   INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  metric     TEXT NOT NULL,
  real_value REAL,
  text_value TEXT,
  is_absent  INTEGER NOT NULL DEFAULT 0,     -- 1 = explicitly \"no value\", not just uncorrected
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
";

/// A typed analysis fact — the value an analyzer observed for a metric. The
/// library's storage vocabulary; it names no analyzer, so the library stays
/// analyzer-agnostic. `Blob` is for future compact descriptors (fingerprints,
/// embeddings).
#[derive(Debug, Clone, PartialEq)]
pub enum Fact {
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

/// One asset's stored facts, keyed by absolute path. The caller (which owns the
/// analyzers) reconstructs a typed report from these.
pub type AssetFacts = (PathBuf, Vec<(String, Fact)>);

/// One asset's stored overrides, keyed by absolute path. `None` per metric means
/// explicitly marked absent (see [`Library::overrides`]).
pub type AssetOverrides = (PathBuf, Vec<(String, Option<Fact>)>);

pub struct Library {
    conn: Connection,
    root: PathBuf,
}

impl Library {
    pub fn db_path(root: &Path) -> PathBuf {
        root.join(".punks").join("library.db")
    }

    /// Does `root` itself have a library?
    pub fn exists(root: &Path) -> bool {
        Self::db_path(root).is_file()
    }

    /// Walk up from `start` looking for the nearest ancestor (inclusive) that
    /// is a library root.
    pub fn find_root(start: &Path) -> Option<PathBuf> {
        let mut cur = Some(start);
        while let Some(dir) = cur {
            if Self::exists(dir) {
                return Some(dir.to_path_buf());
            }
            cur = dir.parent();
        }
        None
    }

    /// Create a library at `root` (making `<root>/.punks/`) and open it.
    /// This is the ONLY place a `.punks` folder is created — callers must make
    /// it an explicit user action.
    pub fn create(root: &Path) -> Result<Library, LibraryError> {
        std::fs::create_dir_all(root.join(".punks"))?;
        Self::open_at(root)
    }

    /// Open an existing library at `root`. Errors if none exists — use
    /// [`create`](Self::create) for deliberate initialization.
    pub fn open(root: &Path) -> Result<Library, LibraryError> {
        if !Self::exists(root) {
            return Err(LibraryError::NotALibrary(root.to_path_buf()));
        }
        Self::open_at(root)
    }

    fn open_at(root: &Path) -> Result<Library, LibraryError> {
        let mut conn = Connection::open(Self::db_path(root))?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        // `PRAGMA journal_mode=WAL` is a documented SQLite exception: unlike
        // ordinary statements, it does NOT honor busy_timeout's retry handler.
        // Converting a brand-new file to WAL for the first time needs an
        // exclusive lock, so two connections racing to open a fresh database
        // can each get an immediate, un-retried "database is locked" — verified
        // empirically (a bare busy_timeout does nothing for this specific
        // pragma). Retry it ourselves with a short backoff instead.
        set_wal_mode_with_retry(&conn)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Multiple threads (scan/peaks/analysis workers) each open their own
        // connection to the same database, sometimes moments apart — e.g. right
        // after `create()`, the background scan opens a second connection before
        // this one's migrations would otherwise be visible. An IMMEDIATE
        // transaction takes SQLite's write lock up front, so a second connection
        // racing in here blocks (via `busy_timeout` above) until the first
        // commits, then re-reads `user_version` and finds nothing left to do —
        // instead of both racing the same DDL (e.g. two `ALTER TABLE ... RENAME`
        // to the same temp name).
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            tx.execute_batch(SCHEMA_V1)?;
            tx.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            tx.execute_batch(SCHEMA_V2)?;
            tx.pragma_update(None, "user_version", 2)?;
        }
        if version < 3 {
            tx.execute_batch(SCHEMA_V3)?;
            tx.pragma_update(None, "user_version", 3)?;
        }
        if version < 4 {
            tx.execute_batch(SCHEMA_V4)?;
            tx.pragma_update(None, "user_version", 4)?;
        }
        if version < 5 {
            tx.execute_batch(SCHEMA_V5)?;
            tx.pragma_update(None, "user_version", 5)?;
        }
        if version < 6 {
            tx.execute_batch(SCHEMA_V6)?;
            tx.pragma_update(None, "user_version", 6)?;
        }
        tx.commit()?;

        Ok(Library {
            conn,
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // --- Tags --------------------------------------------------------------

    /// Get-or-create by name (case-insensitive): tagging "Kick" when "kick"
    /// exists reuses the existing tag rather than splitting the vocabulary.
    pub fn create_tag(&self, name: &str) -> Result<Tag, LibraryError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(LibraryError::Db("empty tag name".into()));
        }
        self.conn.execute(
            "INSERT INTO tags(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            [name],
        )?;
        let (id, name) = self.conn.query_row(
            "SELECT id, name FROM tags WHERE name = ?1 COLLATE NOCASE",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(Tag { id, name })
    }

    /// Exact-match (case-insensitive) tag lookup.
    pub fn find_tag(&self, name: &str) -> Result<Option<Tag>, LibraryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM tags WHERE name = ?1 COLLATE NOCASE")?;
        let mut rows = stmt.query_map([name.trim()], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    /// Delete a tag everywhere (assignments cascade). User-initiated only.
    pub fn delete_tag(&self, tag_id: i64) -> Result<(), LibraryError> {
        self.conn
            .execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
        Ok(())
    }

    pub fn list_tags(&self) -> Result<Vec<TagCount>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name,
                    (SELECT COUNT(*) FROM asset_tags at
                       JOIN assets a ON a.id = at.asset_id
                      WHERE at.tag_id = t.id AND a.missing = 0)
               FROM tags t ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TagCount {
                id: r.get(0)?,
                name: r.get(1)?,
                count: r.get::<_, i64>(2)? as usize,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Assign a tag to the asset at `abs_path` (must live under the root).
    /// If the file hasn't been scanned yet it is ingested on the spot, so
    /// tagging never requires a full rescan first.
    pub fn assign_tag(&self, abs_path: &Path, tag_id: i64) -> Result<(), LibraryError> {
        let asset_id = self.ensure_asset(abs_path)?;
        self.conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) VALUES (?1, ?2)
             ON CONFLICT DO NOTHING",
            [asset_id, tag_id],
        )?;
        Ok(())
    }

    pub fn remove_tag(&self, abs_path: &Path, tag_id: i64) -> Result<(), LibraryError> {
        let Some(asset_id) = self.asset_id_for(abs_path)? else {
            return Ok(());
        };
        self.conn.execute(
            "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
            [asset_id, tag_id],
        )?;
        Ok(())
    }

    pub fn tags_for_asset(&self, abs_path: &Path) -> Result<Vec<Tag>, LibraryError> {
        let Some(asset_id) = self.asset_id_for(abs_path)? else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name FROM tags t
               JOIN asset_tags at ON at.tag_id = t.id
              WHERE at.asset_id = ?1 ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([asset_id], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Non-missing assets carrying ALL of `tag_ids` (AND semantics).
    pub fn assets_with_all_tags(&self, tag_ids: &[i64]) -> Result<Vec<Asset>, LibraryError> {
        if tag_ids.is_empty() {
            return self.list_assets();
        }
        let placeholders = vec!["?"; tag_ids.len()].join(",");
        let sql = format!(
            "SELECT a.id, a.relative_path, a.size, a.mtime_ms FROM assets a
              WHERE a.missing = 0 AND (
                SELECT COUNT(*) FROM asset_tags at
                 WHERE at.asset_id = a.id AND at.tag_id IN ({placeholders})
              ) = {}
              ORDER BY a.relative_path",
            tag_ids.len()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(tag_ids), row_to_asset)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn list_assets(&self) -> Result<Vec<Asset>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, relative_path, size, mtime_ms FROM assets
              WHERE missing = 0 ORDER BY relative_path",
        )?;
        let rows = stmt.query_map([], row_to_asset)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every (relative_path, tag_id) assignment on non-missing assets, for
    /// callers building an in-memory display cache in one query.
    pub fn all_asset_tags(&self) -> Result<Vec<(PathBuf, i64)>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.relative_path, at.tag_id FROM asset_tags at
               JOIN assets a ON a.id = at.asset_id WHERE a.missing = 0",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((str_to_rel(&r.get::<_, String>(0)?), r.get::<_, i64>(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn asset_id_for(&self, abs_path: &Path) -> Result<Option<i64>, LibraryError> {
        let rel = self.rel_str(abs_path)?;
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM assets WHERE relative_path = ?1")?;
        let mut rows = stmt.query_map([rel], |r| r.get::<_, i64>(0))?;
        Ok(rows.next().transpose()?)
    }

    fn ensure_asset(&self, abs_path: &Path) -> Result<i64, LibraryError> {
        if let Some(id) = self.asset_id_for(abs_path)? {
            return Ok(id);
        }
        let rel = self.rel_str(abs_path)?;
        let meta = std::fs::metadata(abs_path)?;
        let hash = hash_file(abs_path, meta.len())?;
        self.conn.execute(
            "INSERT INTO assets(relative_path, size, mtime_ms, content_hash)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![rel, meta.len(), mtime_ms(&meta), hash],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn rel_str(&self, abs_path: &Path) -> Result<String, LibraryError> {
        let rel = abs_path.strip_prefix(&self.root).map_err(|_| {
            LibraryError::Io(format!(
                "{} is outside library root {}",
                abs_path.display(),
                self.root.display()
            ))
        })?;
        Ok(rel_to_str(rel))
    }

    // --- Waveform cache (Generated / disposable) ---------------------------
    //
    // Full-source waveform peaks are expensive to compute (one whole-file
    // stream-decode) but cheap to store, so they're cached on disk, named by
    // the asset's stable UUID, under `.punks/waveforms/`. This is Generated
    // data: deleting the folder just forces a recompute, never loses anything.

    fn asset_uuid(&self, abs_path: &Path) -> Result<Option<String>, LibraryError> {
        let rel = self.rel_str(abs_path)?;
        let mut stmt = self
            .conn
            .prepare("SELECT uuid FROM assets WHERE relative_path = ?1")?;
        let mut rows = stmt.query_map([rel], |r| r.get::<_, String>(0))?;
        Ok(rows.next().transpose()?)
    }

    fn waveform_bin_path(&self, uuid: &str) -> PathBuf {
        self.root
            .join(".punks")
            .join("waveforms")
            .join(format!("{uuid}.bin"))
    }

    /// Load cached waveform peaks for `abs_path`, or `None` if there's no
    /// cache entry or it's stale. Staleness is a cheap size+mtime check (the
    /// same fast-path signal used for asset identity): a file touched but
    /// unchanged triggers a harmless recompute, never a wrong waveform.
    pub fn load_waveform(&self, abs_path: &Path) -> Result<Option<Vec<(f32, f32)>>, LibraryError> {
        let Some(uuid) = self.asset_uuid(abs_path)? else {
            return Ok(None);
        };
        let bin = self.waveform_bin_path(&uuid);
        let bytes = match std::fs::read(&bin) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(LibraryError::Io(e.to_string())),
        };
        let meta = std::fs::metadata(abs_path)?;
        Ok(decode_waveform_bin(&bytes, meta.len(), mtime_ms(&meta)))
    }

    /// Cache waveform peaks for `abs_path`. Ingests the asset if it hasn't been
    /// scanned yet, so a first-audition waveform is cached without a full scan.
    pub fn store_waveform(
        &self,
        abs_path: &Path,
        peaks: &[(f32, f32)],
    ) -> Result<(), LibraryError> {
        self.ensure_asset(abs_path)?;
        let uuid = self
            .asset_uuid(abs_path)?
            .ok_or_else(|| LibraryError::Db("asset uuid missing after ingest".into()))?;
        let bin = self.waveform_bin_path(&uuid);
        if let Some(parent) = bin.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let meta = std::fs::metadata(abs_path)?;
        let bytes = encode_waveform_bin(peaks, meta.len(), mtime_ms(&meta));
        std::fs::write(&bin, bytes)?;
        Ok(())
    }

    // --- Analysis (per-asset jobs queue + opaque results) ------------------
    //
    // Generated data, analyzer-agnostic. The library owns only the queue's
    // lifecycle (pending → running → done/error, with profiling) and stores
    // results as opaque (metric, value) pairs. Which analyzers exist, what they
    // compute, and the single `pipeline_version` that decides staleness all live
    // in punks-analysis; nothing here names an analyzer.

    /// Enqueue a `pending` job for every present asset at `pipeline_version`,
    /// in one set-based statement. Idempotent: re-queues an asset only if its
    /// stored pipeline version differs or it isn't already `done`, so it cheaply
    /// backfills legacy assets and picks up newly-scanned ones on every scan.
    pub fn enqueue_all(&self, pipeline_version: u32) -> Result<(), LibraryError> {
        self.conn.execute(
            "INSERT INTO analysis_jobs(asset_id, status, pipeline_version)
             SELECT id, 'pending', ?1 FROM assets WHERE missing = 0
             ON CONFLICT(asset_id) DO UPDATE SET
               status = 'pending', pipeline_version = excluded.pipeline_version,
               error = NULL, updated_at = unixepoch()
             WHERE analysis_jobs.pipeline_version != excluded.pipeline_version
                OR analysis_jobs.status != 'done'",
            rusqlite::params![pipeline_version],
        )?;
        Ok(())
    }

    /// Requeue any `running` job back to `pending`. Run once at the start of a
    /// drain so jobs left mid-flight by a crash or quit are retried, not stuck.
    pub fn reset_running_jobs(&self) -> Result<(), LibraryError> {
        self.conn.execute(
            "UPDATE analysis_jobs SET status = 'pending', started_at = NULL,
               updated_at = unixepoch()
             WHERE status = 'running'",
            [],
        )?;
        Ok(())
    }

    /// Claim the next `pending` asset: mark it `running` (stamping `started_at`)
    /// and return its absolute path for the caller to decode. `None` when the
    /// queue is drained. A single worker drains the queue, so a plain
    /// select-then-update needs no extra locking.
    pub fn claim_next_pending(&mut self) -> Result<Option<PathBuf>, LibraryError> {
        let row = self.conn.query_row(
            "SELECT j.asset_id, a.relative_path
               FROM analysis_jobs j JOIN assets a ON a.id = j.asset_id
              WHERE j.status = 'pending' AND a.missing = 0
              ORDER BY j.asset_id LIMIT 1",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        );
        let (asset_id, rel) = match row {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        self.conn.execute(
            "UPDATE analysis_jobs SET status = 'running', started_at = unixepoch(),
               finished_at = NULL, duration_ms = NULL, error = NULL,
               updated_at = unixepoch()
             WHERE asset_id = ?1",
            rusqlite::params![asset_id],
        )?;
        Ok(Some(self.root.join(str_to_rel(&rel))))
    }

    /// Claim one specific asset out of order (e.g. the file the user just
    /// selected, jumping a FIFO backlog) — same `running` transition as
    /// [`claim_next_pending`](Self::claim_next_pending), but only if it's
    /// currently `pending`. Returns `None` (a no-op) if it's already
    /// running/done/error, or has no job at all — never re-claims finished work.
    pub fn claim_path(&mut self, abs_path: &Path) -> Result<Option<PathBuf>, LibraryError> {
        let Some(asset_id) = self.asset_id_for(abs_path)? else {
            return Ok(None);
        };
        let updated = self.conn.execute(
            "UPDATE analysis_jobs SET status = 'running', started_at = unixepoch(),
               finished_at = NULL, duration_ms = NULL, error = NULL,
               updated_at = unixepoch()
             WHERE asset_id = ?1 AND status = 'pending'",
            rusqlite::params![asset_id],
        )?;
        Ok((updated > 0).then(|| abs_path.to_path_buf()))
    }

    /// Store an asset's analysis facts (opaque `(metric, Fact)` pairs, e.g. from
    /// the caller's report) and mark its job `done` with the elapsed
    /// `duration_ms` — one transaction. Upserts, so re-running is idempotent.
    pub fn store_analysis(
        &mut self,
        abs_path: &Path,
        facts: &[(&str, Fact)],
        duration_ms: u32,
    ) -> Result<(), LibraryError> {
        let asset_id = self.ensure_asset(abs_path)?;
        let tx = self.conn.transaction()?;
        for (metric, fact) in facts {
            // Exactly one column is non-null per the table CHECK.
            let (real, text, blob): (Option<f64>, Option<&str>, Option<&[u8]>) = match fact {
                Fact::Real(v) => (Some(*v), None, None),
                Fact::Text(s) => (None, Some(s.as_str()), None),
                Fact::Blob(b) => (None, None, Some(b.as_slice())),
            };
            tx.execute(
                "INSERT INTO audio_analysis(asset_id, metric, real_value, text_value, blob_value)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(asset_id, metric) DO UPDATE SET
                   real_value = excluded.real_value, text_value = excluded.text_value,
                   blob_value = excluded.blob_value, computed_at = unixepoch()",
                rusqlite::params![asset_id, metric, real, text, blob],
            )?;
        }
        // Normal flow: claim already created this row (with the right
        // pipeline_version); the UPDATE branch keeps it. The INSERT branch (no
        // prior job) only fires for a direct store that skipped the queue.
        tx.execute(
            "INSERT INTO analysis_jobs(asset_id, status, pipeline_version, finished_at, duration_ms)
             VALUES (?1, 'done', 0, unixepoch(), ?2)
             ON CONFLICT(asset_id) DO UPDATE SET
               status = 'done', error = NULL, finished_at = unixepoch(),
               duration_ms = ?2, updated_at = unixepoch()",
            rusqlite::params![asset_id, duration_ms],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Mark an asset's job `error` (e.g. its file wouldn't decode) so the worker
    /// doesn't re-pull it every drain.
    pub fn fail_analysis(&mut self, abs_path: &Path, err: &str) -> Result<(), LibraryError> {
        let asset_id = self.ensure_asset(abs_path)?;
        self.conn.execute(
            "INSERT INTO analysis_jobs(asset_id, status, pipeline_version, error, finished_at)
             VALUES (?1, 'error', 0, ?2, unixepoch())
             ON CONFLICT(asset_id) DO UPDATE SET
               status = 'error', error = ?2, finished_at = unixepoch(),
               updated_at = unixepoch()",
            rusqlite::params![asset_id, err],
        )?;
        Ok(())
    }

    /// This asset's stored `(metric, Fact)` pairs (empty if none computed).
    pub fn facts(&self, abs_path: &Path) -> Result<Vec<(String, Fact)>, LibraryError> {
        let Some(asset_id) = self.asset_id_for(abs_path)? else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT metric, real_value, text_value, blob_value
               FROM audio_analysis WHERE asset_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![asset_id], |r| {
            Ok((r.get::<_, String>(0)?, row_to_fact_at(r, 1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every present asset's facts, grouped by absolute path, for a display cache
    /// reload. Mirrors [`all_asset_tags`](Self::all_asset_tags).
    pub fn all_facts(&self) -> Result<Vec<AssetFacts>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.relative_path, m.metric, m.real_value, m.text_value, m.blob_value
               FROM audio_analysis m JOIN assets a ON a.id = m.asset_id
              WHERE a.missing = 0
              ORDER BY a.relative_path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                row_to_fact_at(r, 2)?,
            ))
        })?;
        // Rows are ordered by path, so consecutive same-path rows group.
        let mut out: Vec<AssetFacts> = Vec::new();
        for row in rows {
            let (rel, metric, fact) = row?;
            let path = self.root.join(str_to_rel(&rel));
            match out.last_mut() {
                Some((p, v)) if *p == path => v.push((metric, fact)),
                _ => out.push((path, vec![(metric, fact)])),
            }
        }
        Ok(out)
    }

    /// This asset's job status (`pending` | `running` | `done` | `error`), or
    /// `None` if it has no job. Drives the UI's "analyzing…" state.
    pub fn job_status(&self, abs_path: &Path) -> Result<Option<String>, LibraryError> {
        let Some(asset_id) = self.asset_id_for(abs_path)? else {
            return Ok(None);
        };
        let mut stmt = self
            .conn
            .prepare("SELECT status FROM analysis_jobs WHERE asset_id = ?1")?;
        let mut rows = stmt.query_map(rusqlite::params![asset_id], |r| r.get(0))?;
        Ok(rows.next().transpose()?)
    }

    // --- Fact overrides (user corrections; user data, never regenerated) ---
    //
    // An override row's value is `Option<Fact>`: `Some(fact)` corrects the
    // detected value; `None` means the user marked the metric explicitly absent
    // (e.g. "this sound has no key") — different from *no row*, which falls back
    // to the detected guess. See [`overrides`](Self::overrides).

    /// Set a user override for one metric on `abs_path` (ingesting the asset if
    /// needed): upsert into `fact_overrides`, leaving `audio_analysis` untouched
    /// so re-running analyzers can never erase it. `Blob` overrides are
    /// unsupported (the override UI only produces text/number) → warn + no-op.
    pub fn set_override(
        &self,
        abs_path: &Path,
        metric: &str,
        value: &Fact,
    ) -> Result<(), LibraryError> {
        let (real, text): (Option<f64>, Option<&str>) = match value {
            Fact::Real(v) => (Some(*v), None),
            Fact::Text(s) => (None, Some(s.as_str())),
            Fact::Blob(_) => {
                log::warn!("ignoring blob override for {metric} on {abs_path:?}");
                return Ok(());
            }
        };
        let asset_id = self.ensure_asset(abs_path)?;
        self.conn.execute(
            "INSERT INTO fact_overrides(asset_id, metric, real_value, text_value, is_absent)
             VALUES (?1, ?2, ?3, ?4, 0)
             ON CONFLICT(asset_id, metric) DO UPDATE SET
               real_value = excluded.real_value, text_value = excluded.text_value,
               is_absent = 0, updated_at = unixepoch()",
            rusqlite::params![asset_id, metric, real, text],
        )?;
        Ok(())
    }

    /// Mark a metric explicitly absent on `abs_path` — "this sound has no key",
    /// "no BPM applies" — hiding the detected guess even though it exists.
    /// Distinct from [`clear_override`](Self::clear_override), which instead
    /// *reveals* the detected guess again.
    pub fn mark_absent(&self, abs_path: &Path, metric: &str) -> Result<(), LibraryError> {
        let asset_id = self.ensure_asset(abs_path)?;
        self.conn.execute(
            "INSERT INTO fact_overrides(asset_id, metric, real_value, text_value, is_absent)
             VALUES (?1, ?2, NULL, NULL, 1)
             ON CONFLICT(asset_id, metric) DO UPDATE SET
               real_value = NULL, text_value = NULL, is_absent = 1, updated_at = unixepoch()",
            rusqlite::params![asset_id, metric],
        )?;
        Ok(())
    }

    /// Remove a user override (a value override or an absent mark); the
    /// resolved value falls back to the detected one.
    pub fn clear_override(&self, abs_path: &Path, metric: &str) -> Result<(), LibraryError> {
        let Some(asset_id) = self.asset_id_for(abs_path)? else {
            return Ok(());
        };
        self.conn.execute(
            "DELETE FROM fact_overrides WHERE asset_id = ?1 AND metric = ?2",
            rusqlite::params![asset_id, metric],
        )?;
        Ok(())
    }

    /// This asset's user overrides as `(metric, Option<Fact>)` pairs (empty if
    /// none). `None` means the metric is marked explicitly absent.
    pub fn overrides(&self, abs_path: &Path) -> Result<Vec<(String, Option<Fact>)>, LibraryError> {
        let Some(asset_id) = self.asset_id_for(abs_path)? else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT metric, real_value, text_value, is_absent
               FROM fact_overrides WHERE asset_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![asset_id], |r| {
            Ok((r.get::<_, String>(0)?, row_to_override_at(r, 1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every present asset's overrides, grouped by absolute path, for a display
    /// cache reload. Mirrors [`all_facts`](Self::all_facts).
    pub fn all_overrides(&self) -> Result<Vec<AssetOverrides>, LibraryError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.relative_path, o.metric, o.real_value, o.text_value, o.is_absent
               FROM fact_overrides o JOIN assets a ON a.id = o.asset_id
              WHERE a.missing = 0
              ORDER BY a.relative_path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                row_to_override_at(r, 2)?,
            ))
        })?;
        let mut out: Vec<AssetOverrides> = Vec::new();
        for row in rows {
            let (rel, metric, fact) = row?;
            let path = self.root.join(str_to_rel(&rel));
            match out.last_mut() {
                Some((p, v)) if *p == path => v.push((metric, fact)),
                _ => out.push((path, vec![(metric, fact)])),
            }
        }
        Ok(out)
    }

    // --- Reconciliation ----------------------------------------------------

    /// Reconcile the database against `files` (the current on-disk state).
    /// Identity resolution, in order per file:
    ///   1. (relative_path, size, mtime) match — unchanged, no hashing.
    ///   2. path match, size/mtime differ — same asset modified in place.
    ///   3. (size, mtime) candidates whose old path vanished — verify by hash.
    ///   4. content-hash match among vanished assets — file was moved/renamed.
    ///   5. otherwise a genuinely new asset (hashed at ingest so future moves
    ///      can be recognised).
    ///
    /// Assets no longer on disk are marked missing, never deleted: tags are
    /// user data and must survive a file that comes back later.
    pub fn reconcile(&mut self, files: &[ScannedFile]) -> Result<ScanSummary, LibraryError> {
        self.reconcile_with_progress(files, |_| {})
    }

    /// As [`reconcile`](Self::reconcile), calling `on_progress(done)` after each
    /// file is considered (1..=files.len()) so a UI can show scan progress. The
    /// per-file hashing is the slow part, so this is the useful granularity.
    pub fn reconcile_with_progress(
        &mut self,
        files: &[ScannedFile],
        mut on_progress: impl FnMut(usize),
    ) -> Result<ScanSummary, LibraryError> {
        struct Row {
            id: i64,
            path: String,
            size: u64,
            mtime_ms: i64,
            hash: Option<String>,
            missing: bool,
        }

        let tx = self.conn.transaction()?;
        let mut summary = ScanSummary::default();

        let rows: Vec<Row> = {
            let mut stmt = tx.prepare(
                "SELECT id, relative_path, size, mtime_ms, content_hash, missing FROM assets",
            )?;
            let it = stmt.query_map([], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    size: r.get::<_, i64>(2)? as u64,
                    mtime_ms: r.get(3)?,
                    hash: r.get(4)?,
                    missing: r.get::<_, i64>(5)? != 0,
                })
            })?;
            it.collect::<Result<_, _>>()?
        };
        let by_path: HashMap<&str, usize> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.path.as_str(), i))
            .collect();
        let disk_paths: HashSet<String> =
            files.iter().map(|f| rel_to_str(&f.relative_path)).collect();
        let mut seen: HashSet<i64> = HashSet::new();

        for (fi, f) in files.iter().enumerate() {
            on_progress(fi + 1);
            let key = rel_to_str(&f.relative_path);
            let abs = self.root.join(&f.relative_path);

            if let Some(&i) = by_path.get(key.as_str()) {
                let row = &rows[i];
                seen.insert(row.id);
                if row.size == f.size && row.mtime_ms == f.mtime_ms {
                    if row.missing {
                        tx.execute("UPDATE assets SET missing = 0 WHERE id = ?1", [row.id])?;
                    }
                    summary.unchanged += 1;
                } else {
                    // Same path, new bytes: path identity dominates — the user
                    // edited the file in place; its tags stay.
                    let hash = hash_file(&abs, f.size)?;
                    tx.execute(
                        "UPDATE assets SET size=?1, mtime_ms=?2, content_hash=?3, missing=0
                          WHERE id=?4",
                        rusqlite::params![f.size, f.mtime_ms, hash, row.id],
                    )?;
                    summary.modified += 1;
                }
                continue;
            }

            // No path match. Hash lazily, at most once per file.
            let mut file_hash: Option<String> = None;
            let mut matched: Option<i64> = None;

            // Stage 2/3: (size, mtime) candidates whose recorded path is gone.
            // ponytail: O(files x rows) linear scans; fine for tens of
            // thousands of assets. Upgrade to prebuilt indices if libraries
            // grow past that.
            for r in &rows {
                if seen.contains(&r.id)
                    || r.size != f.size
                    || r.mtime_ms != f.mtime_ms
                    || disk_paths.contains(&r.path)
                {
                    continue;
                }
                if file_hash.is_none() {
                    file_hash = Some(hash_file(&abs, f.size)?);
                }
                // Only claim a candidate the hash actually confirms — a
                // heuristic-only match could pin a user's tags to the wrong
                // file, which is the one unforgivable failure here.
                if r.hash.is_some() && r.hash == file_hash {
                    matched = Some(r.id);
                    break;
                }
            }

            // Stage 4: pure content match (file moved AND touched).
            if matched.is_none() {
                if file_hash.is_none() {
                    file_hash = Some(hash_file(&abs, f.size)?);
                }
                for r in &rows {
                    if !seen.contains(&r.id)
                        && !disk_paths.contains(&r.path)
                        && r.hash.is_some()
                        && r.hash == file_hash
                    {
                        matched = Some(r.id);
                        break;
                    }
                }
            }

            match matched {
                Some(id) => {
                    tx.execute(
                        "UPDATE assets SET relative_path=?1, size=?2, mtime_ms=?3, missing=0
                          WHERE id=?4",
                        rusqlite::params![key, f.size, f.mtime_ms, id],
                    )?;
                    seen.insert(id);
                    summary.moved += 1;
                }
                None => {
                    tx.execute(
                        "INSERT INTO assets(relative_path, size, mtime_ms, content_hash)
                         VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![key, f.size, f.mtime_ms, file_hash],
                    )?;
                    summary.added += 1;
                }
            }
        }

        for r in &rows {
            if !seen.contains(&r.id) && !disk_paths.contains(&r.path) && !r.missing {
                tx.execute("UPDATE assets SET missing = 1 WHERE id = ?1", [r.id])?;
                summary.missing += 1;
            }
        }

        tx.commit()?;
        Ok(summary)
    }
}

fn row_to_asset(r: &rusqlite::Row) -> Result<Asset, rusqlite::Error> {
    Ok(Asset {
        id: r.get(0)?,
        relative_path: str_to_rel(&r.get::<_, String>(1)?),
        size: r.get::<_, i64>(2)? as u64,
        mtime_ms: r.get(3)?,
    })
}

/// Read a `Fact` from three consecutive columns (real, text, blob) starting at
/// `base`. Exactly one is non-null (enforced by the table CHECK).
fn row_to_fact_at(r: &rusqlite::Row, base: usize) -> Result<Fact, rusqlite::Error> {
    if let Some(v) = r.get::<_, Option<f64>>(base)? {
        Ok(Fact::Real(v))
    } else if let Some(s) = r.get::<_, Option<String>>(base + 1)? {
        Ok(Fact::Text(s))
    } else {
        Ok(Fact::Blob(r.get::<_, Vec<u8>>(base + 2)?))
    }
}

/// Read an override value from three consecutive columns (real, text,
/// is_absent) starting at `base`. `None` when `is_absent = 1` (explicitly no
/// value); otherwise exactly one of real/text is set (table CHECK).
fn row_to_override_at(r: &rusqlite::Row, base: usize) -> Result<Option<Fact>, rusqlite::Error> {
    if r.get::<_, bool>(base + 2)? {
        return Ok(None);
    }
    match r.get::<_, Option<f64>>(base)? {
        Some(v) => Ok(Some(Fact::Real(v))),
        None => Ok(Some(Fact::Text(r.get::<_, String>(base + 1)?))),
    }
}

/// Walk `root` for supported audio files, described cheaply for reconcile.
/// Reuses punks-core's recursive walker (which skips hidden entries, so the
/// `.punks` folder itself is never scanned).
pub fn scan_files(root: &Path) -> Result<Vec<ScannedFile>, LibraryError> {
    let entries = punks_core::search_directory(root, "", punks_core::SUPPORTED_EXTENSIONS)
        .map_err(|e| LibraryError::Io(e.to_string()))?;
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let Ok(rel) = e.path.strip_prefix(root) else {
            continue;
        };
        let Ok(meta) = std::fs::metadata(&e.path) else {
            continue;
        };
        out.push(ScannedFile {
            relative_path: rel.to_path_buf(),
            size: meta.len(),
            mtime_ms: mtime_ms(&meta),
        });
    }
    Ok(out)
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// Waveform cache binary format:
//   magic "PKWF" (4) | version u8 | num_buckets u32 LE |
//   source_size u64 LE | source_mtime_ms i64 LE |    <- validity stamp
//   num_buckets * (lo f32 LE, hi f32 LE)
const WAVEFORM_MAGIC: &[u8; 4] = b"PKWF";
const WAVEFORM_VERSION: u8 = 1;
const WAVEFORM_HEADER_LEN: usize = 4 + 1 + 4 + 8 + 8;

fn encode_waveform_bin(peaks: &[(f32, f32)], source_size: u64, source_mtime_ms: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(WAVEFORM_HEADER_LEN + peaks.len() * 8);
    out.extend_from_slice(WAVEFORM_MAGIC);
    out.push(WAVEFORM_VERSION);
    out.extend_from_slice(&(peaks.len() as u32).to_le_bytes());
    out.extend_from_slice(&source_size.to_le_bytes());
    out.extend_from_slice(&source_mtime_ms.to_le_bytes());
    for &(lo, hi) in peaks {
        out.extend_from_slice(&lo.to_le_bytes());
        out.extend_from_slice(&hi.to_le_bytes());
    }
    out
}

/// Returns the peaks only if the file is well-formed AND its validity stamp
/// matches the current source size/mtime; any mismatch (stale, corrupt, wrong
/// version) yields `None` so the caller recomputes.
fn decode_waveform_bin(
    bytes: &[u8],
    source_size: u64,
    source_mtime_ms: i64,
) -> Option<Vec<(f32, f32)>> {
    if bytes.len() < WAVEFORM_HEADER_LEN
        || &bytes[0..4] != WAVEFORM_MAGIC
        || bytes[4] != WAVEFORM_VERSION
    {
        return None;
    }
    let num_buckets = u32::from_le_bytes(bytes[5..9].try_into().ok()?) as usize;
    let size = u64::from_le_bytes(bytes[9..17].try_into().ok()?);
    let mtime = i64::from_le_bytes(bytes[17..25].try_into().ok()?);
    if size != source_size || mtime != source_mtime_ms {
        return None; // stale: file changed since the waveform was computed.
    }
    if bytes.len() != WAVEFORM_HEADER_LEN + num_buckets * 8 {
        return None;
    }
    let mut peaks = Vec::with_capacity(num_buckets);
    for i in 0..num_buckets {
        let base = WAVEFORM_HEADER_LEN + i * 8;
        let lo = f32::from_le_bytes(bytes[base..base + 4].try_into().ok()?);
        let hi = f32::from_le_bytes(bytes[base + 4..base + 8].try_into().ok()?);
        peaks.push((lo, hi));
    }
    Some(peaks)
}

/// Store relative paths with `/` separators so the DB is portable across
/// OSes and unaffected by where the root lives.
fn rel_to_str(rel: &Path) -> String {
    let s = rel.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '/' {
        s.into_owned()
    } else {
        s.replace(std::path::MAIN_SEPARATOR, "/")
    }
}

fn str_to_rel(s: &str) -> PathBuf {
    // `/` is accepted as a separator by Path APIs on all supported OSes.
    PathBuf::from(s)
}

/// Content identity hash. Full sha256 for small files; for large ones,
/// sha256 over (size ++ first MiB ++ last MiB).
/// ponytail: partial hashing means two >2 MiB files differing only in the
/// middle bytes collide. Failure mode: a move could be matched to the wrong
/// asset and inherit its tags — vanishingly unlikely for real audio, and this
/// is identity (not integrity). Upgrade path: full streaming hash behind the
/// same function signature.
fn hash_file(abs: &Path, size: u64) -> Result<String, LibraryError> {
    const CHUNK: u64 = 1024 * 1024;
    let mut hasher = Sha256::new();
    hasher.update(size.to_le_bytes());
    let mut file = File::open(abs)?;

    if size <= 2 * CHUNK {
        let mut buf = Vec::with_capacity(size as usize);
        file.read_to_end(&mut buf)?;
        hasher.update(&buf);
    } else {
        let mut buf = vec![0u8; CHUNK as usize];
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
        file.seek(SeekFrom::End(-(CHUNK as i64)))?;
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempRoot(PathBuf);
    impl TempRoot {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "punks2_lib_{}_{}_{tag}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            TempRoot(dir)
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_wav(root: &Path, rel: &str, contents: &[u8]) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, contents).unwrap();
        p
    }

    fn scan(lib: &mut Library) -> ScanSummary {
        let files = scan_files(lib.root().to_path_buf().as_path()).unwrap();
        lib.reconcile(&files).unwrap()
    }

    #[test]
    fn create_scan_and_tag() {
        let t = TempRoot::new("basic");
        let a = write_wav(&t.0, "kicks/one.wav", b"AAAA");
        write_wav(&t.0, "two.wav", b"BBBB");

        let mut lib = Library::create(&t.0).unwrap();
        let s = scan(&mut lib);
        assert_eq!(s.added, 2);

        let kick = lib.create_tag("kick").unwrap();
        lib.assign_tag(&a, kick.id).unwrap();

        let tags = lib.tags_for_asset(&a).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "kick");

        let assets = lib.assets_with_all_tags(&[kick.id]).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].relative_path, PathBuf::from("kicks/one.wav"));
    }

    #[test]
    fn tag_names_dedupe_case_insensitively() {
        let t = TempRoot::new("nocase");
        let lib = Library::create(&t.0).unwrap();
        let a = lib.create_tag("Kick").unwrap();
        let b = lib.create_tag("kick").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(lib.list_tags().unwrap().len(), 1);
        assert!(lib.find_tag("KICK").unwrap().is_some());
        assert!(lib.find_tag("snare").unwrap().is_none());
    }

    #[test]
    fn rename_keeps_tags() {
        let t = TempRoot::new("rename");
        let a = write_wav(&t.0, "old_name.wav", b"CONTENT-1");
        let mut lib = Library::create(&t.0).unwrap();
        scan(&mut lib);
        let tag = lib.create_tag("808").unwrap();
        lib.assign_tag(&a, tag.id).unwrap();

        // Rename on disk (rename preserves mtime -> stage-3 candidate path).
        let b = t.0.join("subdir").join("new_name.wav");
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        std::fs::rename(&a, &b).unwrap();

        let s = scan(&mut lib);
        assert_eq!(s.moved, 1);
        assert_eq!(s.added, 0);

        let tags = lib.tags_for_asset(&b).unwrap();
        assert_eq!(tags.len(), 1, "tags must survive a rename/move");
        assert_eq!(tags[0].name, "808");
        // Exactly one asset row — no duplicate identity.
        assert_eq!(lib.list_assets().unwrap().len(), 1);
    }

    #[test]
    fn modify_in_place_keeps_tags() {
        let t = TempRoot::new("modify");
        let a = write_wav(&t.0, "loop.wav", b"V1");
        let mut lib = Library::create(&t.0).unwrap();
        scan(&mut lib);
        let tag = lib.create_tag("loop").unwrap();
        lib.assign_tag(&a, tag.id).unwrap();

        std::fs::write(&a, b"V2 with more bytes").unwrap();
        let s = scan(&mut lib);
        assert_eq!(s.modified, 1);
        assert_eq!(lib.tags_for_asset(&a).unwrap().len(), 1);
        assert_eq!(lib.list_assets().unwrap().len(), 1);
    }

    #[test]
    fn delete_marks_missing_and_return_recovers() {
        let t = TempRoot::new("missing");
        let a = write_wav(&t.0, "gone.wav", b"UNIQUE-CONTENT");
        let mut lib = Library::create(&t.0).unwrap();
        scan(&mut lib);
        let tag = lib.create_tag("fx").unwrap();
        lib.assign_tag(&a, tag.id).unwrap();

        std::fs::remove_file(&a).unwrap();
        let s = scan(&mut lib);
        assert_eq!(s.missing, 1);
        // Hidden from listings, but the row (and its tag) still exists.
        assert!(lib.list_assets().unwrap().is_empty());
        assert_eq!(lib.list_tags().unwrap()[0].count, 0);

        // File comes back elsewhere with the same content -> recovered by hash.
        let b = write_wav(&t.0, "returned/gone.wav", b"UNIQUE-CONTENT");
        let s = scan(&mut lib);
        assert_eq!(s.moved, 1);
        assert_eq!(lib.tags_for_asset(&b).unwrap().len(), 1);
    }

    #[test]
    fn and_query_requires_all_tags() {
        let t = TempRoot::new("and");
        let a = write_wav(&t.0, "a.wav", b"A");
        let b = write_wav(&t.0, "b.wav", b"B");
        let mut lib = Library::create(&t.0).unwrap();
        scan(&mut lib);
        let kick = lib.create_tag("kick").unwrap();
        let punchy = lib.create_tag("punchy").unwrap();
        lib.assign_tag(&a, kick.id).unwrap();
        lib.assign_tag(&a, punchy.id).unwrap();
        lib.assign_tag(&b, kick.id).unwrap();

        assert_eq!(lib.assets_with_all_tags(&[kick.id]).unwrap().len(), 2);
        let both = lib.assets_with_all_tags(&[kick.id, punchy.id]).unwrap();
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].relative_path, PathBuf::from("a.wav"));
    }

    #[test]
    fn delete_tag_cascades_and_remove_unassigns() {
        let t = TempRoot::new("del");
        let a = write_wav(&t.0, "x.wav", b"X");
        let mut lib = Library::create(&t.0).unwrap();
        scan(&mut lib);
        let t1 = lib.create_tag("one").unwrap();
        let t2 = lib.create_tag("two").unwrap();
        lib.assign_tag(&a, t1.id).unwrap();
        lib.assign_tag(&a, t2.id).unwrap();

        lib.remove_tag(&a, t2.id).unwrap();
        assert_eq!(lib.tags_for_asset(&a).unwrap().len(), 1);

        lib.delete_tag(t1.id).unwrap();
        assert!(lib.tags_for_asset(&a).unwrap().is_empty());
        assert!(lib.list_tags().unwrap().iter().all(|t| t.name != "one"));
    }

    #[test]
    fn find_root_walks_ancestors() {
        let t = TempRoot::new("root");
        let sub = t.0.join("a/b/c");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(Library::find_root(&sub).is_none());
        Library::create(&t.0).unwrap();
        assert_eq!(Library::find_root(&sub), Some(t.0.clone()));
    }

    #[test]
    fn assign_before_scan_ingests_on_the_spot() {
        let t = TempRoot::new("ingest");
        let a = write_wav(&t.0, "fresh.wav", b"FRESH");
        let lib = Library::create(&t.0).unwrap();
        // No scan yet — tagging must still work.
        let tag = lib.create_tag("new").unwrap();
        lib.assign_tag(&a, tag.id).unwrap();
        assert_eq!(lib.tags_for_asset(&a).unwrap().len(), 1);
        assert_eq!(lib.list_assets().unwrap().len(), 1);
    }

    #[test]
    fn waveform_cache_round_trips() {
        let t = TempRoot::new("wave");
        let a = write_wav(&t.0, "loop.wav", b"AUDIO-BYTES");
        let lib = Library::create(&t.0).unwrap();
        assert!(lib.load_waveform(&a).unwrap().is_none()); // nothing cached yet

        let peaks = vec![(-0.5, 0.5), (-1.0, 1.0), (0.0, 0.25)];
        lib.store_waveform(&a, &peaks).unwrap();
        assert_eq!(lib.load_waveform(&a).unwrap(), Some(peaks));
    }

    #[test]
    fn waveform_cache_invalidates_on_change() {
        let t = TempRoot::new("wavestale");
        let a = write_wav(&t.0, "clip.wav", b"V1");
        let lib = Library::create(&t.0).unwrap();
        lib.store_waveform(&a, &[(-0.1, 0.1)]).unwrap();
        assert!(lib.load_waveform(&a).unwrap().is_some());

        // Rewrite with different length + a fresh mtime → stamp mismatch → miss.
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&a, b"V2 is longer than before").unwrap();
        assert!(lib.load_waveform(&a).unwrap().is_none());
    }

    #[test]
    fn decode_waveform_bin_rejects_junk() {
        assert!(super::decode_waveform_bin(b"nope", 0, 0).is_none());
    }

    #[test]
    fn reconcile_reports_monotonic_progress() {
        let t = TempRoot::new("progress");
        for i in 0..5 {
            write_wav(&t.0, &format!("s{i}.wav"), format!("DATA-{i}").as_bytes());
        }
        let mut lib = Library::create(&t.0).unwrap();
        let files = scan_files(lib.root().to_path_buf().as_path()).unwrap();
        assert_eq!(files.len(), 5);

        let mut seen = Vec::new();
        lib.reconcile_with_progress(&files, |done| seen.push(done))
            .unwrap();
        // One report per file, strictly increasing, ending at the count.
        assert_eq!(seen, vec![1, 2, 3, 4, 5]);
    }

    /// Count analysis_jobs rows in a given status.
    fn jobs_in(lib: &Library, status: &str) -> i64 {
        lib.conn
            .query_row(
                "SELECT COUNT(*) FROM analysis_jobs WHERE status = ?1",
                [status],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn enqueue_all_is_idempotent_and_backfills() {
        let t = TempRoot::new("enqueue_all");
        write_wav(&t.0, "a.wav", b"A");
        write_wav(&t.0, "b.wav", b"B");
        let mut lib = Library::create(&t.0).unwrap();
        // Ingest the two files as assets (reconcile is the real path).
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();

        lib.enqueue_all(7).unwrap();
        assert_eq!(jobs_in(&lib, "pending"), 2);
        // Re-enqueue at the same version: no change (both still pending, not done).
        lib.enqueue_all(7).unwrap();
        assert_eq!(jobs_in(&lib, "pending"), 2);
    }

    #[test]
    fn claim_run_store_read_lifecycle() {
        let t = TempRoot::new("lifecycle");
        let a = write_wav(&t.0, "clip.wav", b"DUMMY");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();

        lib.enqueue_all(1).unwrap();
        // Claim marks the asset running and hands back its absolute path.
        let claimed = lib.claim_next_pending().unwrap().unwrap();
        assert_eq!(claimed, a);
        assert_eq!(jobs_in(&lib, "running"), 1);
        assert_eq!(jobs_in(&lib, "pending"), 0);
        // Queue is now empty of pending work.
        assert!(lib.claim_next_pending().unwrap().is_none());

        // Store typed facts: numeric (rms/peak) + text (instrument) coexist.
        let facts: &[(&str, Fact)] = &[
            ("rms", Fact::Real(0.5)),
            ("peak", Fact::Real(0.5)),
            ("instrument", Fact::Text("kick".into())),
        ];
        lib.store_analysis(&a, facts, 12).unwrap();
        assert_eq!(jobs_in(&lib, "done"), 1);
        assert_eq!(jobs_in(&lib, "running"), 0);
        // duration_ms was recorded.
        let dur: i64 = lib
            .conn
            .query_row("SELECT duration_ms FROM analysis_jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(dur, 12);

        // Read back, order-independent — real and text facts both round-trip.
        let mut got = lib.facts(&a).unwrap();
        got.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(
            got,
            vec![
                ("instrument".to_string(), Fact::Text("kick".into())),
                ("peak".to_string(), Fact::Real(0.5)),
                ("rms".to_string(), Fact::Real(0.5)),
            ]
        );
        assert_eq!(lib.job_status(&a).unwrap().as_deref(), Some("done"));

        // Re-store is idempotent: same 3 rows, not 6.
        lib.store_analysis(&a, facts, 5).unwrap();
        let n: i64 = lib
            .conn
            .query_row("SELECT COUNT(*) FROM audio_analysis", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn blob_fact_round_trips() {
        let t = TempRoot::new("blob");
        let a = write_wav(&t.0, "b.wav", b"B");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();
        let bytes = vec![0u8, 1, 2, 250, 255];
        lib.store_analysis(&a, &[("fingerprint", Fact::Blob(bytes.clone()))], 0)
            .unwrap();
        assert_eq!(
            lib.facts(&a).unwrap(),
            vec![("fingerprint".to_string(), Fact::Blob(bytes))]
        );
    }

    #[test]
    fn check_rejects_zero_or_two_typed_columns() {
        let t = TempRoot::new("check");
        let a = write_wav(&t.0, "c.wav", b"C");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();
        let id = lib.asset_id_for(&a).unwrap().unwrap();
        // Two columns set → CHECK fails.
        let two = lib.conn.execute(
            "INSERT INTO audio_analysis(asset_id, metric, real_value, text_value)
             VALUES (?1, 'x', 1.0, 'y')",
            rusqlite::params![id],
        );
        assert!(two.is_err());
        // Zero columns set → CHECK fails.
        let none = lib.conn.execute(
            "INSERT INTO audio_analysis(asset_id, metric) VALUES (?1, 'x')",
            rusqlite::params![id],
        );
        assert!(none.is_err());
    }

    #[test]
    fn claim_path_jumps_a_specific_asset_and_is_idempotent() {
        let t = TempRoot::new("claim_path");
        let a = write_wav(&t.0, "a.wav", b"A");
        let b = write_wav(&t.0, "b.wav", b"B");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();
        lib.enqueue_all(1).unwrap();

        // Directory walk order (and so asset_id / FIFO order) isn't guaranteed;
        // pin down which asset FIFO would serve first, then jump the *other* one.
        let id_a = lib.asset_id_for(&a).unwrap().unwrap();
        let id_b = lib.asset_id_for(&b).unwrap().unwrap();
        let (fifo_first, jump) = if id_a < id_b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };

        let claimed = lib.claim_path(&jump).unwrap();
        assert_eq!(claimed, Some(jump.clone()));
        assert_eq!(jobs_in(&lib, "running"), 1);
        assert_eq!(jobs_in(&lib, "pending"), 1); // the FIFO-first asset untouched

        // FIFO backlog still serves the earlier asset normally afterwards.
        let next = lib.claim_next_pending().unwrap();
        assert_eq!(next, Some(fifo_first));

        // Re-claiming the jumped asset while it's already running is a no-op.
        assert_eq!(lib.claim_path(&jump).unwrap(), None);

        // A path outside the library (no asset/job) is a no-op, not an error.
        let outside = t.0.join("nonexistent.wav");
        assert_eq!(lib.claim_path(&outside).unwrap(), None);
    }

    #[test]
    fn fail_marks_error_and_is_not_reclaimed() {
        let t = TempRoot::new("fail");
        let a = write_wav(&t.0, "bad.wav", b"BAD");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();
        lib.enqueue_all(1).unwrap();

        lib.claim_next_pending().unwrap().unwrap();
        lib.fail_analysis(&a, "unsupported codec").unwrap();
        assert_eq!(jobs_in(&lib, "error"), 1);
        assert_eq!(lib.job_status(&a).unwrap().as_deref(), Some("error"));
        // Errored jobs aren't re-pulled.
        assert!(lib.claim_next_pending().unwrap().is_none());
    }

    #[test]
    fn reset_running_requeues_stale_jobs() {
        let t = TempRoot::new("reset");
        write_wav(&t.0, "c.wav", b"C");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();
        lib.enqueue_all(1).unwrap();

        lib.claim_next_pending().unwrap().unwrap(); // now 'running'
        assert_eq!(jobs_in(&lib, "running"), 1);
        // Simulate a crash mid-drain: reset requeues it.
        lib.reset_running_jobs().unwrap();
        assert_eq!(jobs_in(&lib, "running"), 0);
        assert_eq!(jobs_in(&lib, "pending"), 1);
    }

    #[test]
    fn analysis_rows_cascade_on_asset_delete() {
        let t = TempRoot::new("cascade");
        let a = write_wav(&t.0, "x.wav", b"X");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();
        lib.enqueue_all(1).unwrap();
        lib.store_analysis(&a, &[("rms", Fact::Real(0.5))], 1)
            .unwrap();

        let asset_id = lib.asset_id_for(&a).unwrap().unwrap();
        lib.conn
            .execute("DELETE FROM assets WHERE id = ?1", [asset_id])
            .unwrap();
        let results: i64 = lib
            .conn
            .query_row("SELECT COUNT(*) FROM audio_analysis", [], |r| r.get(0))
            .unwrap();
        let jobs: i64 = lib
            .conn
            .query_row("SELECT COUNT(*) FROM analysis_jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(results, 0);
        assert_eq!(jobs, 0);
    }

    #[test]
    fn override_set_read_clear() {
        let t = TempRoot::new("override");
        let a = write_wav(&t.0, "x.wav", b"X");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();

        assert!(lib.overrides(&a).unwrap().is_empty());
        lib.set_override(&a, "key", &Fact::Text("G#min".into()))
            .unwrap();
        lib.set_override(&a, "bpm", &Fact::Real(140.0)).unwrap();
        let mut got = lib.overrides(&a).unwrap();
        got.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(
            got,
            vec![
                ("bpm".to_string(), Some(Fact::Real(140.0))),
                ("key".to_string(), Some(Fact::Text("G#min".into()))),
            ]
        );
        // Upsert: setting the same metric replaces, doesn't duplicate.
        lib.set_override(&a, "key", &Fact::Text("Am".into()))
            .unwrap();
        assert_eq!(lib.overrides(&a).unwrap().len(), 2);

        lib.clear_override(&a, "key").unwrap();
        assert_eq!(
            lib.overrides(&a).unwrap(),
            vec![("bpm".to_string(), Some(Fact::Real(140.0)))]
        );
    }

    #[test]
    fn mark_absent_hides_detected_and_differs_from_clear() {
        let t = TempRoot::new("absent");
        let a = write_wav(&t.0, "x.wav", b"X");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();

        // Analyzer guessed a key; user says the sound is actually atonal.
        lib.store_analysis(&a, &[("key", Fact::Text("Am".into()))], 1)
            .unwrap();
        lib.mark_absent(&a, "key").unwrap();
        assert_eq!(lib.overrides(&a).unwrap(), vec![("key".to_string(), None)]);
        // The detected guess is untouched underneath.
        assert_eq!(
            lib.facts(&a).unwrap(),
            vec![("key".to_string(), Fact::Text("Am".into()))]
        );

        // Setting a real value afterwards clears the absent mark.
        lib.set_override(&a, "key", &Fact::Text("Cmaj".into()))
            .unwrap();
        assert_eq!(
            lib.overrides(&a).unwrap(),
            vec![("key".to_string(), Some(Fact::Text("Cmaj".into())))]
        );

        // Clearing the override (not marking absent) removes the row entirely —
        // a *different* state from "marked absent".
        lib.mark_absent(&a, "key").unwrap();
        lib.clear_override(&a, "key").unwrap();
        assert!(lib.overrides(&a).unwrap().is_empty());
    }

    #[test]
    fn v5_to_v6_migration_preserves_existing_override_rows() {
        // Simulate a database that already ran the v5 migration (no `is_absent`
        // column) and has a real user override in it — exactly the state of an
        // pre-existing library.db from before `is_absent` was added. Opening it
        // again must add the column via SCHEMA_V6 without losing that row.
        let t = TempRoot::new("v5_upgrade");
        let a = write_wav(&t.0, "x.wav", b"X");
        std::fs::create_dir_all(t.0.join(".punks")).unwrap();
        {
            let conn = Connection::open(Library::db_path(&t.0)).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute_batch(SCHEMA_V3).unwrap();
            conn.execute_batch(SCHEMA_V4).unwrap();
            conn.execute_batch(SCHEMA_V5).unwrap();
            conn.pragma_update(None, "user_version", 5).unwrap();
            conn.execute(
                "INSERT INTO assets(relative_path, size, mtime_ms) VALUES ('x.wav', 1, 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO fact_overrides(asset_id, metric, text_value) VALUES (1, 'key', 'G#min')",
                [],
            )
            .unwrap();
        }

        // Re-opening runs the v6 migration; the pre-existing override survives
        // and the new is_absent-based API works on the upgraded table.
        let lib = Library::open(&t.0).unwrap();
        assert_eq!(
            lib.overrides(&a).unwrap(),
            vec![("key".to_string(), Some(Fact::Text("G#min".into())))]
        );
        lib.mark_absent(&a, "bpm").unwrap();
        assert_eq!(lib.overrides(&a).unwrap().len(), 2);
    }

    #[test]
    fn v6_migration_self_heals_a_leftover_temp_table() {
        // Simulate a `fact_overrides_v5` stray left behind by some earlier,
        // non-transactional run of this same migration (e.g. a build predating
        // the `open_at` transaction fix, or a killed process mid-migration).
        // The v6 migration must clean it up rather than failing with "already
        // another table ... fact_overrides_v5".
        let t = TempRoot::new("leftover_temp");
        std::fs::create_dir_all(t.0.join(".punks")).unwrap();
        {
            let conn = Connection::open(Library::db_path(&t.0)).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute_batch(SCHEMA_V3).unwrap();
            conn.execute_batch(SCHEMA_V4).unwrap();
            conn.execute_batch(SCHEMA_V5).unwrap();
            conn.pragma_update(None, "user_version", 5).unwrap();
            // The stray: a leftover table with the exact name SCHEMA_V6 renames
            // `fact_overrides` to, from an interrupted prior attempt.
            conn.execute_batch("CREATE TABLE fact_overrides_v5(junk INTEGER);")
                .unwrap();
        }

        // Must not error, and must not leave the stray behind afterwards.
        let lib = Library::open(&t.0).unwrap();
        let user_version: i64 = lib
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(user_version, 6);
        let leftover: i64 = lib
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'fact_overrides_v5'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0);
    }

    #[test]
    fn concurrent_first_open_does_not_race_migrations() {
        // Regression: several threads (scan/peaks/analysis workers, in the real
        // app) can each open a brand-new library's connection within moments of
        // each other. Before migrations were serialized behind a single
        // transaction, two connections both mid-migration could both attempt
        // `ALTER TABLE fact_overrides RENAME TO fact_overrides_v5`, and the
        // second would fail with "already another table ... fact_overrides_v5".
        let t = TempRoot::new("concurrent_open");
        let a = write_wav(&t.0, "x.wav", b"X");
        std::fs::create_dir_all(t.0.join(".punks")).unwrap();
        // A zero-length file is a valid empty SQLite database (fresh, version 0)
        // — every thread below races the *entire* v1..v6 migration sequence.
        std::fs::write(Library::db_path(&t.0), []).unwrap();

        // 4 matches the real app's actual concurrent openers (create + scan +
        // peaks + analysis workers); racing far more than that under a loaded
        // CI machine risks exceeding busy_timeout on contention alone, which
        // would be a test artifact, not the bug under test.
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let root = t.0.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    Library::open(&root).map(|_| ())
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }

        // Exactly one, fully-migrated fact_overrides table — no leftover temp
        // table from a half-finished concurrent migration.
        let lib = Library::open(&t.0).unwrap();
        let user_version: i64 = lib
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(user_version, 6);
        lib.mark_absent(&a, "key").unwrap(); // exercises is_absent on the upgraded table
        let leftover: i64 = lib
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'fact_overrides_v5'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0);
    }

    #[test]
    fn override_survives_reanalysis() {
        // The provenance guarantee: re-running the analyzer (store_analysis) must
        // never touch the user's override — they live in separate tables.
        let t = TempRoot::new("override_regen");
        let a = write_wav(&t.0, "x.wav", b"X");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();

        // Analyzer detects key = Am; user overrides to G#min.
        lib.store_analysis(&a, &[("key", Fact::Text("Am".into()))], 1)
            .unwrap();
        lib.set_override(&a, "key", &Fact::Text("G#min".into()))
            .unwrap();

        // Re-analyze (still detects Am, maybe re-detects the same).
        lib.store_analysis(&a, &[("key", Fact::Text("Am".into()))], 1)
            .unwrap();

        // Detected value is intact AND the override is intact — resolution is the
        // caller's job, both layers survive independently.
        assert_eq!(
            lib.facts(&a).unwrap(),
            vec![("key".to_string(), Fact::Text("Am".into()))]
        );
        assert_eq!(
            lib.overrides(&a).unwrap(),
            vec![("key".to_string(), Some(Fact::Text("G#min".into())))]
        );
    }

    #[test]
    fn overrides_cascade_on_asset_delete() {
        let t = TempRoot::new("override_cascade");
        let a = write_wav(&t.0, "x.wav", b"X");
        let mut lib = Library::create(&t.0).unwrap();
        lib.reconcile(&scan_files(&t.0).unwrap()).unwrap();
        lib.set_override(&a, "instrument", &Fact::Text("snare".into()))
            .unwrap();

        let asset_id = lib.asset_id_for(&a).unwrap().unwrap();
        lib.conn
            .execute("DELETE FROM assets WHERE id = ?1", [asset_id])
            .unwrap();
        let n: i64 = lib
            .conn
            .query_row("SELECT COUNT(*) FROM fact_overrides", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
