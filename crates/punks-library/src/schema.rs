use crate::LibraryError;
use rusqlite::{Connection, TransactionBehavior};

pub const VERSION: i64 = 6;
const MIGRATIONS: [&str; 6] = [
    include_str!("migrations/1.sql"),
    include_str!("migrations/2.sql"),
    include_str!("migrations/3.sql"),
    include_str!("migrations/4.sql"),
    include_str!("migrations/5.sql"),
    include_str!("migrations/6.sql"),
];

fn version(conn: &Connection) -> Result<i64, LibraryError> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}
fn columns(
    conn: &Connection,
    table: &str,
    expected: &[&str],
    exact: bool,
) -> Result<(), LibraryError> {
    let actual: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get(1))?
        .collect::<Result<_, _>>()?;
    if expected
        .iter()
        .any(|name| !actual.iter().any(|s| s == name))
        || (exact && actual.len() != expected.len())
    {
        return Err(LibraryError::Db(format!("schema mismatch in {table}: expected columns {expected:?}, found {actual:?}; database left unchanged")));
    }
    Ok(())
}
fn validate(conn: &Connection, v: i64) -> Result<(), LibraryError> {
    if !(0..=VERSION).contains(&v) {
        return Err(LibraryError::Db(format!(
            "unsupported database version {v}; newest supported is {VERSION}"
        )));
    }
    if v == 0 {
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if count != 0 {
            return Err(LibraryError::Db(
                "unversioned nonempty database; refusing to infer or overwrite its schema".into(),
            ));
        }
        return Ok(());
    }
    columns(
        conn,
        "assets",
        &[
            "id",
            "uuid",
            "relative_path",
            "size",
            "mtime_ms",
            "content_hash",
            "missing",
            "created_at",
        ],
        false,
    )?;
    columns(conn, "tags", &["id", "name", "created_at"], false)?;
    columns(conn, "asset_tags", &["asset_id", "tag_id"], false)?;
    columns(conn, "fields", &["id", "name"], false)?;
    columns(
        conn,
        "field_values",
        &["asset_id", "field_id", "value"],
        false,
    )?;
    if v == 2 {
        columns(
            conn,
            "analysis_jobs",
            &[
                "asset_id",
                "analyzer",
                "version",
                "status",
                "error",
                "updated_at",
            ],
            true,
        )?;
        columns(
            conn,
            "audio_analysis",
            &[
                "asset_id",
                "analyzer",
                "version",
                "metric",
                "value",
                "computed_at",
            ],
            true,
        )?;
    } else if v >= 3 {
        columns(
            conn,
            "analysis_jobs",
            &[
                "asset_id",
                "status",
                "pipeline_version",
                "error",
                "started_at",
                "finished_at",
                "duration_ms",
                "updated_at",
            ],
            false,
        )?;
        if v == 3 {
            columns(
                conn,
                "audio_analysis",
                &["asset_id", "metric", "value", "computed_at"],
                true,
            )?;
        } else {
            columns(
                conn,
                "audio_analysis",
                &[
                    "asset_id",
                    "metric",
                    "real_value",
                    "text_value",
                    "blob_value",
                    "computed_at",
                ],
                false,
            )?;
        }
    }
    if v == 5 {
        columns(
            conn,
            "fact_overrides",
            &[
                "asset_id",
                "metric",
                "real_value",
                "text_value",
                "source",
                "updated_at",
            ],
            true,
        )?;
    }
    if v >= 6 {
        columns(
            conn,
            "fact_overrides",
            &[
                "asset_id",
                "metric",
                "real_value",
                "text_value",
                "is_absent",
                "source",
                "updated_at",
            ],
            false,
        )?;
    }
    Ok(())
}

pub fn migrate(conn: &mut Connection) -> Result<(), LibraryError> {
    let initial = version(conn)?;
    if initial > VERSION {
        return Err(LibraryError::Db(format!(
            "unsupported database version {initial}; newest supported is {VERSION}"
        )));
    }
    // Current databases need only a consistent read snapshot. Upgraders take
    // the write lock before rereading the version, so concurrent opens serialize.
    let behavior = if initial == VERSION {
        TransactionBehavior::Deferred
    } else {
        TransactionBehavior::Immediate
    };
    let tx = conn.transaction_with_behavior(behavior)?;
    let current = version(&tx)?;
    validate(&tx, current)?;
    for next in current + 1..=VERSION {
        tx.execute_batch(MIGRATIONS[(next - 1) as usize])
            .map_err(|e| {
                LibraryError::Db(format!(
                    "migration {next} failed: {e}; rolled back to version {current}"
                ))
            })?;
        tx.pragma_update(None, "user_version", next)?;
    }
    validate(&tx, VERSION)?;
    if current != VERSION {
        let mut check = tx.prepare("PRAGMA foreign_key_check")?;
        if check.query([])?.next()?.is_some() {
            return Err(LibraryError::Db(
                "migration foreign-key validation failed; all changes rolled back".into(),
            ));
        }
    }
    tx.commit()?;
    Ok(())
}
