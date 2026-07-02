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
        let conn = Connection::open(Self::db_path(root))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", 1)?;
        }

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

        for f in files {
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
}
