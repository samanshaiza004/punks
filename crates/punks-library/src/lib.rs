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

    pub errors: usize,
}

fn set_wal_mode_with_retry(conn: &Connection) -> Result<(), LibraryError> {
    for attempt in 0..3 {
        match conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0)) {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(mode) => {
                return Err(LibraryError::Db(format!(
                    "could not enable WAL: SQLite selected {mode}"
                )))
            }
            Err(rusqlite::Error::SqliteFailure(ref e, _))
                if matches!(
                    e.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) && attempt < 2 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            Err(e) => return Err(LibraryError::Db(format!("enable WAL: {e}"))),
        }
    }
    unreachable!()
}

mod schema;

#[derive(Debug, Clone, PartialEq)]
pub enum Fact {
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}
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
    pub fn exists(root: &Path) -> bool {
        Self::db_path(root).is_file()
    }

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

    pub fn create(root: &Path) -> Result<Library, LibraryError> {
        if !root.is_dir() {
            return Err(LibraryError::Io(format!(
                "not a directory: {}",
                root.display()
            )));
        }
        std::fs::create_dir_all(root.join(".punks"))?;
        Self::open_at(root, true)
    }

    pub fn open(root: &Path) -> Result<Library, LibraryError> {
        if !Self::exists(root) {
            return Err(LibraryError::NotALibrary(root.to_path_buf()));
        }
        Self::open_at(root, false)
    }

    fn open_at(root: &Path, create: bool) -> Result<Library, LibraryError> {
        let root = root.to_path_buf();
        let path = Self::db_path(&root);
        let mut flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE;
        if create {
            flags |= rusqlite::OpenFlags::SQLITE_OPEN_CREATE;
        }
        let mut conn = Connection::open_with_flags(&path, flags)
            .map_err(|e| LibraryError::Db(format!("open {}: {e}", path.display())))?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::migrate(&mut conn)
            .map_err(|e| LibraryError::Db(format!("{}: {e}", path.display())))?;
        set_wal_mode_with_retry(&conn)?;
        Ok(Library { conn, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

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
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(relative_path) DO NOTHING",
            rusqlite::params![rel, meta.len(), mtime_ms(&meta), hash],
        )?;
        self.asset_id_for(abs_path)?
            .ok_or_else(|| LibraryError::Db("asset missing after ingest".into()))
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

    pub fn asset_uuid_for(&self, abs_path: &Path) -> Result<Option<String>, LibraryError> {
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

    pub fn load_waveform(&self, abs_path: &Path) -> Result<Option<Vec<(f32, f32)>>, LibraryError> {
        let Some(uuid) = self.asset_uuid_for(abs_path)? else {
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

    pub fn store_waveform(
        &self,
        abs_path: &Path,
        peaks: &[(f32, f32)],
    ) -> Result<(), LibraryError> {
        self.ensure_asset(abs_path)?;
        let uuid = self
            .asset_uuid_for(abs_path)?
            .ok_or_else(|| LibraryError::Db("asset uuid missing after ingest".into()))?;
        let bin = self.waveform_bin_path(&uuid);
        if let Some(parent) = bin.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let meta = std::fs::metadata(abs_path)?;
        let bytes = encode_waveform_bin(peaks, meta.len(), mtime_ms(&meta));
        static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = bin.with_extension(format!("bin.tmp.{}.{seq}", std::process::id()));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &bin)?;
        Ok(())
    }

    pub fn enqueue_all(&self, pipeline_version: u32) -> Result<(), LibraryError> {
        self.conn.execute(
            "INSERT INTO analysis_jobs(asset_id, status, pipeline_version)
             SELECT id, 'pending', ?1 FROM assets WHERE missing = 0
             ON CONFLICT(asset_id) DO UPDATE SET
               status = 'pending', pipeline_version = excluded.pipeline_version,
               error = NULL, updated_at = unixepoch()
             WHERE analysis_jobs.pipeline_version != excluded.pipeline_version
                OR analysis_jobs.status = 'pending'",
            rusqlite::params![pipeline_version],
        )?;
        Ok(())
    }

    pub fn reset_running_jobs(&self) -> Result<(), LibraryError> {
        self.conn.execute(
            "UPDATE analysis_jobs SET status = 'pending', started_at = NULL,
               updated_at = unixepoch()
             WHERE status = 'running'",
            [],
        )?;
        Ok(())
    }

    pub fn claim_next_pending(&mut self) -> Result<Option<PathBuf>, LibraryError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let row=tx.query_row("SELECT j.asset_id,a.relative_path FROM analysis_jobs j JOIN assets a ON a.id=j.asset_id WHERE j.status='pending' AND a.missing=0 ORDER BY j.asset_id LIMIT 1",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)));
        let result = match row {
            Ok((id, rel)) => {
                tx.execute("UPDATE analysis_jobs SET status='running',started_at=unixepoch(),finished_at=NULL,duration_ms=NULL,error=NULL,updated_at=unixepoch() WHERE asset_id=?1 AND status='pending'",[id])?;
                Some(self.root.join(str_to_rel(&rel)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };
        tx.commit()?;
        Ok(result)
    }

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

    pub fn set_metadata_text(
        &self,
        abs_path: &Path,
        metric: &str,
        value: &str,
    ) -> Result<(), LibraryError> {
        let asset_id = self.ensure_asset(abs_path)?;
        self.conn.execute(
            "INSERT INTO audio_analysis(asset_id, metric, text_value)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(asset_id, metric) DO UPDATE SET
               text_value = excluded.text_value, real_value = NULL, blob_value = NULL,
               computed_at = unixepoch()",
            rusqlite::params![asset_id, metric, value],
        )?;
        Ok(())
    }

    pub fn set_description(&self, abs_path: &Path, description: &str) -> Result<(), LibraryError> {
        self.set_metadata_text(abs_path, "description", description)
    }

    pub fn store_analysis(
        &mut self,
        abs_path: &Path,
        facts: &[(&str, Fact)],
        duration_ms: u32,
    ) -> Result<(), LibraryError> {
        let asset_id = self.ensure_asset(abs_path)?;
        let tx = self.conn.transaction()?;
        for (metric, fact) in facts {
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

    pub fn reconcile(&mut self, files: &[ScannedFile]) -> Result<ScanSummary, LibraryError> {
        self.reconcile_with_progress(files, |_| {})
    }

    pub fn reconcile_with_progress(
        &mut self,
        files: &[ScannedFile],
        on_progress: impl FnMut(usize),
    ) -> Result<ScanSummary, LibraryError> {
        self.reconcile_cancellable(files, on_progress, || false)
    }
    pub fn reconcile_cancellable(
        &mut self,
        files: &[ScannedFile],
        mut on_progress: impl FnMut(usize),
        cancelled: impl Fn() -> bool,
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

        'files: for (fi, f) in files.iter().enumerate() {
            if cancelled() {
                return Err(LibraryError::Io("scan cancelled".into()));
            }
            if f.relative_path.is_absolute()
                || f.relative_path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(LibraryError::Io("scan path escapes library".into()));
            }
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
                    let hash = match hash_file(&abs, f.size) {
                        Ok(h) => h,
                        Err(e) => {
                            log::warn!("scan: hashing {} failed, skipping: {e}", abs.display());
                            summary.errors += 1;
                            continue 'files;
                        }
                    };
                    tx.execute(
                        "UPDATE assets SET size=?1, mtime_ms=?2, content_hash=?3, missing=0
                          WHERE id=?4",
                        rusqlite::params![f.size, f.mtime_ms, hash, row.id],
                    )?;
                    tx.execute("UPDATE analysis_jobs SET status='pending',error=NULL,updated_at=unixepoch() WHERE asset_id=?1",[row.id])?;
                    summary.modified += 1;
                }
                continue;
            }

            let mut file_hash: Option<String> = None;
            let mut matched: Option<i64> = None;

            for r in &rows {
                if seen.contains(&r.id)
                    || r.size != f.size
                    || r.mtime_ms != f.mtime_ms
                    || disk_paths.contains(&r.path)
                {
                    continue;
                }
                if file_hash.is_none() {
                    file_hash = match hash_file(&abs, f.size) {
                        Ok(h) => Some(h),
                        Err(e) => {
                            log::warn!("scan: hashing {} failed, skipping: {e}", abs.display());
                            summary.errors += 1;
                            continue 'files;
                        }
                    };
                }
                if r.hash.is_some() && r.hash == file_hash {
                    matched = Some(r.id);
                    break;
                }
            }

            if matched.is_none() {
                if file_hash.is_none() {
                    file_hash = match hash_file(&abs, f.size) {
                        Ok(h) => Some(h),
                        Err(e) => {
                            log::warn!("scan: hashing {} failed, skipping: {e}", abs.display());
                            summary.errors += 1;
                            continue 'files;
                        }
                    };
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

fn row_to_fact_at(r: &rusqlite::Row, base: usize) -> Result<Fact, rusqlite::Error> {
    if let Some(v) = r.get::<_, Option<f64>>(base)? {
        Ok(Fact::Real(v))
    } else if let Some(s) = r.get::<_, Option<String>>(base + 1)? {
        Ok(Fact::Text(s))
    } else {
        Ok(Fact::Blob(r.get::<_, Vec<u8>>(base + 2)?))
    }
}
fn row_to_override_at(r: &rusqlite::Row, base: usize) -> Result<Option<Fact>, rusqlite::Error> {
    if r.get::<_, bool>(base + 2)? {
        return Ok(None);
    }
    match r.get::<_, Option<f64>>(base)? {
        Some(v) => Ok(Some(Fact::Real(v))),
        None => Ok(Some(Fact::Text(r.get::<_, String>(base + 1)?))),
    }
}

pub fn scan_files(root: &Path) -> Result<Vec<ScannedFile>, LibraryError> {
    scan_files_cancellable(root, || false)
}
pub fn scan_files_cancellable(
    root: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<ScannedFile>, LibraryError> {
    if !root.is_dir() {
        return Err(LibraryError::Io(format!(
            "not a directory: {}",
            root.display()
        )));
    }
    let mut out = Vec::new();
    collect_audio_files(root, root, &mut out, &cancelled)?;
    Ok(out)
}

const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "ogg"];

fn collect_audio_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<ScannedFile>,
    cancelled: &impl Fn() -> bool,
) -> Result<(), LibraryError> {
    let entries = std::fs::read_dir(current)?;
    for entry in entries {
        if cancelled() {
            return Err(LibraryError::Io("scan cancelled".into()));
        }
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_audio_files(root, &path, out, cancelled)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
            continue;
        }

        let metadata = entry.metadata()?;
        let Ok(relative_path) = path.strip_prefix(root) else {
            continue;
        };
        out.push(ScannedFile {
            relative_path: relative_path.to_path_buf(),
            size: metadata.len(),
            mtime_ms: mtime_ms(&metadata),
        });
    }
    Ok(())
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
// Version 2 uses an envelope across channels instead of an averaged mono mix.
// Recompute version-1 caches so phase-inverted stereo cannot appear silent.
const WAVEFORM_VERSION: u8 = 2;
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

fn rel_to_str(rel: &Path) -> String {
    let s = rel.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '/' {
        s.into_owned()
    } else {
        s.replace(std::path::MAIN_SEPARATOR, "/")
    }
}

fn str_to_rel(s: &str) -> PathBuf {
    PathBuf::from(s)
}

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
mod waveform_cache_tests {
    use super::*;
    #[test]
    fn version_two_rejects_old_channel_averaged_and_changed_source_caches() {
        let peaks = vec![(-1.0, 1.0), (0.25, 0.75)];
        let mut bytes = encode_waveform_bin(&peaks, 128, 42);
        assert_eq!(decode_waveform_bin(&bytes, 128, 42), Some(peaks));
        assert!(decode_waveform_bin(&bytes, 129, 42).is_none());
        assert!(decode_waveform_bin(&bytes, 128, 43).is_none());
        bytes[4] = 1;
        assert!(decode_waveform_bin(&bytes, 128, 42).is_none());
    }
}
