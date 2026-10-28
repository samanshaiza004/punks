use punks_library::{scan_files, Fact, Library};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Barrier,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "punks-library-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn historical(version: usize) -> Self {
        let fixture = Self::new();
        std::fs::create_dir(fixture.0.join(".punks")).unwrap();
        let conn = fixture.db();
        let schemas = [
            include_str!("fixtures/schema-v1.sql"),
            include_str!("fixtures/schema-v2.sql"),
            include_str!("fixtures/schema-v3.sql"),
            include_str!("fixtures/schema-v4.sql"),
            include_str!("fixtures/schema-v5.sql"),
            include_str!("fixtures/schema-v6.sql"),
        ];
        for sql in &schemas[..version] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", version).unwrap();
        conn.execute_batch("INSERT INTO assets(id,uuid,relative_path,size,mtime_ms) VALUES(41,'fixture-uuid','kick.wav',4,100); INSERT INTO tags(id,name) VALUES(7,'User tag'); INSERT INTO asset_tags VALUES(41,7); INSERT INTO fields VALUES(9,'Notes'); INSERT INTO field_values VALUES(41,9,'keep this');").unwrap();
        if version == 2 {
            conn.execute_batch("INSERT INTO analysis_jobs(asset_id,analyzer,version,status) VALUES(41,'one',2,'done'),(41,'two',3,'done'); INSERT INTO audio_analysis(asset_id,analyzer,version,metric,value,computed_at) VALUES(41,'one',2,'rms',0.25,11),(41,'two',3,'rms',0.5,12);").unwrap();
        } else if version == 3 {
            conn.execute_batch("INSERT INTO analysis_jobs(asset_id,pipeline_version,status) VALUES(41,3,'done'); INSERT INTO audio_analysis(asset_id,metric,value,computed_at) VALUES(41,'rms',0.25,11);").unwrap();
        } else if version >= 4 {
            conn.execute_batch("INSERT INTO analysis_jobs(asset_id,pipeline_version,status) VALUES(41,3,'done'); INSERT INTO audio_analysis(asset_id,metric,real_value,computed_at) VALUES(41,'rms',0.25,11); INSERT INTO audio_analysis(asset_id,metric,text_value) VALUES(41,'description','Original metadata'); INSERT INTO audio_analysis(asset_id,metric,blob_value) VALUES(41,'opaque',X'010203');").unwrap();
        }
        if version >= 5 {
            conn.execute_batch("INSERT INTO fact_overrides(asset_id,metric,text_value,source,updated_at) VALUES(41,'key','F#min','user',123);").unwrap();
        }
        if version >= 6 {
            conn.execute_batch(
                "INSERT INTO fact_overrides(asset_id,metric,is_absent) VALUES(41,'bpm',1);",
            )
            .unwrap();
        }
        fixture
    }
    fn db(&self) -> Connection {
        Connection::open(Library::db_path(&self.0)).unwrap()
    }
    fn write(&self, path: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn scalar<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}
fn schema(conn: &Connection) -> Vec<(String, String)> {
    conn.prepare("SELECT name,sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn fresh_create_and_reopen_do_not_rewrite_schema() {
    let f = Fixture::new();
    drop(Library::create(&f.0).unwrap());
    let conn = f.db();
    assert_eq!(scalar::<i64>(&conn, "PRAGMA user_version"), 6);
    let before = schema(&conn);
    let schema_version: i64 = scalar(&conn, "PRAGMA schema_version");
    drop(conn);
    for _ in 0..5 {
        drop(Library::open(&f.0).unwrap());
    }
    let conn = f.db();
    assert_eq!(schema(&conn), before);
    assert_eq!(
        scalar::<i64>(&conn, "PRAGMA schema_version"),
        schema_version
    );
}

#[test]
fn all_supported_historical_schemas_preserve_user_and_analysis_data() {
    for version in 1..=6 {
        let f = Fixture::historical(version);
        let lib = Library::open(&f.0).unwrap_or_else(|e| panic!("v{version}: {e}"));
        let conn = f.db();
        assert_eq!(scalar::<i64>(&conn, "PRAGMA user_version"), 6);
        assert_eq!(
            scalar::<String>(&conn, "SELECT uuid FROM assets WHERE id=41"),
            "fixture-uuid"
        );
        assert_eq!(
            scalar::<String>(&conn, "SELECT name FROM tags WHERE id=7"),
            "User tag"
        );
        assert_eq!(
            scalar::<i64>(
                &conn,
                "SELECT count(*) FROM asset_tags WHERE asset_id=41 AND tag_id=7"
            ),
            1
        );
        assert_eq!(
            scalar::<String>(&conn, "SELECT value FROM field_values WHERE asset_id=41"),
            "keep this"
        );
        if version >= 2 {
            assert!(lib
                .facts(&f.0.join("kick.wav"))
                .unwrap()
                .iter()
                .any(|(m, _)| m == "rms"));
        }
        if version == 2 {
            assert_eq!(
                scalar::<i64>(&conn, "SELECT count(*) FROM audio_analysis_v2_archive"),
                2
            );
            assert_eq!(
                scalar::<i64>(&conn, "SELECT count(*) FROM analysis_jobs_v2_archive"),
                2
            );
            assert_eq!(
                scalar::<f64>(
                    &conn,
                    "SELECT real_value FROM audio_analysis WHERE metric='rms'"
                ),
                0.5
            );
        }
        if version >= 4 {
            assert_eq!(
                scalar::<String>(
                    &conn,
                    "SELECT text_value FROM audio_analysis WHERE metric='description'"
                ),
                "Original metadata"
            );
            assert_eq!(
                scalar::<Vec<u8>>(
                    &conn,
                    "SELECT blob_value FROM audio_analysis WHERE metric='opaque'"
                ),
                [1, 2, 3]
            );
        }
        if version >= 5 {
            assert_eq!(
                scalar::<String>(
                    &conn,
                    "SELECT text_value FROM fact_overrides WHERE metric='key'"
                ),
                "F#min"
            );
            assert_eq!(
                scalar::<i64>(
                    &conn,
                    "SELECT updated_at FROM fact_overrides WHERE metric='key'"
                ),
                123
            );
        }
        if version == 6 {
            assert!(lib
                .overrides(&f.0.join("kick.wav"))
                .unwrap()
                .contains(&("bpm".into(), None)));
        }
    }
}

#[test]
fn a_late_migration_failure_rolls_back_earlier_steps_and_version() {
    let f = Fixture::historical(3);
    let conn = f.db();
    conn.execute_batch("CREATE TABLE fact_overrides(sentinel TEXT); INSERT INTO fact_overrides VALUES('preserve');").unwrap();
    let before = schema(&conn);
    drop(conn);
    let error = Library::open(&f.0).err().unwrap().to_string();
    assert!(error.contains("migration"), "{error}");
    let conn = f.db();
    assert_eq!(scalar::<i64>(&conn, "PRAGMA user_version"), 3);
    assert_eq!(schema(&conn), before);
    assert_eq!(
        scalar::<f64>(&conn, "SELECT value FROM audio_analysis WHERE metric='rms'"),
        0.25
    );
    assert_eq!(
        scalar::<String>(&conn, "SELECT sentinel FROM fact_overrides"),
        "preserve"
    );
}

#[test]
fn unknown_and_mislabelled_databases_are_rejected_without_mutation() {
    for version in [0, 6, 99] {
        let f = Fixture::new();
        std::fs::create_dir(f.0.join(".punks")).unwrap();
        let conn = f.db();
        conn.execute_batch(
            "CREATE TABLE precious(value TEXT); INSERT INTO precious VALUES('user data');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        let before = schema(&conn);
        drop(conn);
        assert!(Library::open(&f.0).is_err());
        let conn = f.db();
        assert_eq!(schema(&conn), before);
        assert_eq!(scalar::<i64>(&conn, "PRAGMA user_version"), version);
        assert_eq!(
            scalar::<String>(&conn, "SELECT value FROM precious"),
            "user data"
        );
    }
}

#[test]
fn legacy_rebuild_remnants_are_preserved_and_reported() {
    let f = Fixture::historical(5);
    let conn = f.db();
    conn.execute_batch("CREATE TABLE fact_overrides_v5(precious TEXT); INSERT INTO fact_overrides_v5 VALUES('ambiguous');").unwrap();
    drop(conn);
    assert!(Library::open(&f.0).is_err());
    let conn = f.db();
    assert_eq!(scalar::<i64>(&conn, "PRAGMA user_version"), 5);
    assert_eq!(
        scalar::<String>(&conn, "SELECT precious FROM fact_overrides_v5"),
        "ambiguous"
    );
    assert_eq!(
        scalar::<String>(&conn, "SELECT text_value FROM fact_overrides"),
        "F#min"
    );
}

#[test]
fn open_missing_library_does_not_create_one() {
    let f = Fixture::new();
    assert!(Library::open(&f.0).is_err());
    assert!(!Library::db_path(&f.0).exists());
    let file = f.write("file", b"x");
    assert!(Library::create(&file).is_err());
}

#[test]
fn concurrent_migrators_observe_one_complete_schema() {
    let f = Fixture::historical(5);
    let barrier = Arc::new(Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let root = f.0.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                Library::open(&root).unwrap();
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(scalar::<i64>(&f.db(), "PRAGMA user_version"), 6);
    assert_eq!(
        scalar::<String>(&f.db(), "SELECT text_value FROM fact_overrides"),
        "F#min"
    );
}

#[test]
fn concurrent_claims_never_claim_the_same_job_twice() {
    let f = Fixture::new();
    drop(Library::create(&f.0).unwrap());
    let conn = f.db();
    for id in 1..=40 {
        conn.execute(
            "INSERT INTO assets(id,relative_path,size,mtime_ms) VALUES(?1,?2,1,0)",
            params![id, format!("{id}.wav")],
        )
        .unwrap();
    }
    drop(conn);
    Library::open(&f.0).unwrap().enqueue_all(1).unwrap();
    let barrier = Arc::new(Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let root = f.0.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                let mut lib = Library::open(&root).unwrap();
                b.wait();
                let mut got = vec![];
                while let Some(path) = lib.claim_next_pending().unwrap() {
                    got.push(path);
                }
                got
            })
        })
        .collect();
    let paths: Vec<_> = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect();
    let unique: std::collections::HashSet<_> = paths.iter().collect();
    assert_eq!(paths.len(), 40);
    assert_eq!(unique.len(), 40);
}

#[test]
fn enqueue_does_not_reclaim_running_jobs() {
    let f = Fixture::historical(6);
    let mut lib = Library::open(&f.0).unwrap();
    lib.enqueue_all(8).unwrap();
    assert!(lib.claim_next_pending().unwrap().is_some());
    lib.enqueue_all(8).unwrap();
    assert_eq!(
        lib.job_status(&f.0.join("kick.wav")).unwrap().as_deref(),
        Some("running")
    );
}

#[test]
fn reconciliation_preserves_identity_tags_overrides_and_results_across_moves_and_missing_files() {
    let f = Fixture::new();
    let original = f.write("kick.wav", b"same audio");
    let mut lib = Library::create(&f.0).unwrap();
    lib.reconcile(&scan_files(&f.0).unwrap()).unwrap();
    let uuid = lib.asset_uuid_for(&original).unwrap().unwrap();
    let tag = lib.create_tag("favorite").unwrap();
    lib.assign_tag(&original, tag.id).unwrap();
    lib.set_override(&original, "key", &Fact::Text("Dmin".into()))
        .unwrap();
    lib.mark_absent(&original, "bpm").unwrap();
    lib.enqueue_all(1).unwrap();
    lib.store_analysis(&original, &[("rms", Fact::Real(0.25))], 1)
        .unwrap();
    let moved = f.0.join("sub/renamed.wav");
    std::fs::create_dir(moved.parent().unwrap()).unwrap();
    std::fs::rename(&original, &moved).unwrap();
    assert_eq!(lib.reconcile(&scan_files(&f.0).unwrap()).unwrap().moved, 1);
    assert_eq!(
        lib.asset_uuid_for(&moved).unwrap().as_deref(),
        Some(uuid.as_str())
    );
    assert_eq!(lib.tags_for_asset(&moved).unwrap().len(), 1);
    assert_eq!(lib.overrides(&moved).unwrap().len(), 2);
    assert_eq!(lib.facts(&moved).unwrap().len(), 1);
    std::fs::remove_file(&moved).unwrap();
    assert_eq!(
        lib.reconcile(&scan_files(&f.0).unwrap()).unwrap().missing,
        1
    );
    assert_eq!(
        scalar::<i64>(&f.db(), "SELECT count(*) FROM fact_overrides"),
        2
    );
    f.write("sub/renamed.wav", b"same audio");
    lib.reconcile(&scan_files(&f.0).unwrap()).unwrap();
    assert_eq!(
        lib.asset_uuid_for(&moved).unwrap().as_deref(),
        Some(uuid.as_str())
    );
    assert_eq!(lib.tags_for_asset(&moved).unwrap().len(), 1);
}

#[test]
fn a_duplicate_file_does_not_steal_a_present_assets_identity() {
    let f = Fixture::new();
    let one = f.write("one.wav", b"same");
    let mut lib = Library::create(&f.0).unwrap();
    lib.reconcile(&scan_files(&f.0).unwrap()).unwrap();
    let id = lib.asset_uuid_for(&one).unwrap();
    f.write("two.wav", b"same");
    let summary = lib.reconcile(&scan_files(&f.0).unwrap()).unwrap();
    assert_eq!(summary.added, 1);
    assert_eq!(summary.moved, 0);
    assert_eq!(lib.asset_uuid_for(&one).unwrap(), id);
}

#[test]
fn hash_failure_does_not_mark_an_existing_asset_missing() {
    let f = Fixture::new();
    let path = f.write("kick.wav", b"data");
    let mut lib = Library::create(&f.0).unwrap();
    lib.reconcile(&scan_files(&f.0).unwrap()).unwrap();
    let mut files = scan_files(&f.0).unwrap();
    files[0].size += 1;
    std::fs::remove_file(path).unwrap();
    let result = lib.reconcile(&files).unwrap();
    assert_eq!(result.errors, 1);
    assert_eq!(result.missing, 0);
}

#[test]
fn cancelled_reconciliation_rolls_back_changes() {
    let f = Fixture::new();
    f.write("one.wav", b"one");
    f.write("two.wav", b"two");
    let mut lib = Library::create(&f.0).unwrap();
    let stop = std::cell::Cell::new(false);
    assert!(lib
        .reconcile_cancellable(
            &scan_files(&f.0).unwrap(),
            |_| stop.set(true),
            || stop.get()
        )
        .is_err());
    assert!(lib.list_assets().unwrap().is_empty());
    assert!(punks_library::scan_files_cancellable(&f.0, || true).is_err());
}

#[test]
fn modified_sources_keep_user_metadata_and_schedule_reanalysis() {
    let f = Fixture::new();
    let path = f.write("kick.wav", b"one");
    let mut lib = Library::create(&f.0).unwrap();
    lib.reconcile(&scan_files(&f.0).unwrap()).unwrap();
    lib.enqueue_all(1).unwrap();
    lib.store_analysis(&path, &[("rms", Fact::Real(0.25))], 1)
        .unwrap();
    lib.set_override(&path, "key", &Fact::Text("Cmin".into()))
        .unwrap();
    f.write("kick.wav", b"a changed source");
    assert_eq!(
        lib.reconcile(&scan_files(&f.0).unwrap()).unwrap().modified,
        1
    );
    assert_eq!(lib.job_status(&path).unwrap().as_deref(), Some("pending"));
    assert_eq!(lib.overrides(&path).unwrap().len(), 1);
}
