use punks_core::{PunksConfig, SampleBrowser};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "punks-core-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn write(&self, name: &str) {
        std::fs::write(self.0.join(name), b"not audio").unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn browser() -> SampleBrowser {
    SampleBrowser::without_audio(&PunksConfig::default(), Arc::new(|| {})).unwrap()
}
#[test]
fn browsing_selection_and_device_errors_are_independent() {
    let f = Fixture::new();
    f.write("b.wav");
    f.write("a.flac");
    f.write("ignore.txt");
    std::fs::create_dir(f.0.join("folder")).unwrap();
    let mut b = browser();
    b.open_directory(&f.0).unwrap();
    assert_eq!(b.entries().len(), 3);
    assert!(b.entries()[0].is_directory);
    assert!(!b.audio_available());
    b.select(1);
    let identity = b.asset_identity(&b.entries()[1].path);
    b.play_selected();
    assert!(b.last_error().unwrap().contains("disabled"));
    b.stop();
    b.open_directory(&f.0).unwrap();
    assert_eq!(b.asset_identity(&b.entries()[1].path), identity);
    b.navigate_up().unwrap();
    assert_eq!(b.current_directory(), f.0.parent());
}
#[test]
fn asynchronous_search_wakes_once_and_settles_to_idle() {
    let f = Fixture::new();
    for n in 0..200 {
        f.write(&format!("kick-{n}.wav"));
    }
    f.write("snare.ogg");
    let (tx, rx) = mpsc::channel();
    let count = Arc::new(AtomicU64::new(0));
    let c = count.clone();
    let mut b = SampleBrowser::without_audio(
        &PunksConfig::default(),
        Arc::new(move || {
            c.fetch_add(1, Ordering::Relaxed);
            let _ = tx.send(());
        }),
    )
    .unwrap();
    b.open_directory(&f.0).unwrap();
    b.search("snare");
    let deadline = Instant::now() + Duration::from_secs(3);
    while b.is_searching() {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        b.poll();
    }
    assert_eq!(b.search_results().unwrap().len(), 1);
    assert_eq!(b.search_results().unwrap()[0].name, "snare.ogg");
    std::thread::sleep(Duration::from_millis(20));
    let settled = count.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(40));
    assert_eq!(count.load(Ordering::Relaxed), settled);
    drop(b);
    let stopped = count.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(count.load(Ordering::Relaxed), stopped);
}
#[test]
fn obsolete_searches_cannot_replace_a_new_directory() {
    let a = Fixture::new();
    for n in 0..500 {
        a.write(&format!("kick-{n}.wav"));
    }
    let z = Fixture::new();
    z.write("other.mp3");
    let mut b = browser();
    b.open_directory(&a.0).unwrap();
    for n in 0..30 {
        b.search(&format!("kick-{n}"));
    }
    b.open_directory(&z.0).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    b.poll();
    assert!(!b.is_in_search_mode());
    assert_eq!(b.entries().len(), 1);
    assert_eq!(b.entries()[0].name, "other.mp3");
}
#[test]
fn shutdown_during_work_finishes_and_joins_workers() {
    let f = Fixture::new();
    for n in 0..1000 {
        f.write(&format!("sample-{n}.wav"));
    }
    let root = f.0.clone();
    let (tx, rx) = mpsc::channel();
    let t = std::thread::spawn(move || {
        for _ in 0..12 {
            let mut b = browser();
            b.open_directory(&root).unwrap();
            b.search("sample");
            drop(b);
        }
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(10))
        .expect("core shutdown deadlocked");
    t.join().unwrap();
}
#[test]
fn missing_directories_report_an_error_and_preserve_the_last_listing() {
    let f = Fixture::new();
    f.write("kick.wav");
    let mut b = browser();
    b.open_directory(&f.0).unwrap();
    assert!(b.open_directory(&f.0.join("missing")).is_err());
    assert_eq!(b.entries()[0].name, "kick.wav");
    assert!(b.last_error().is_some());
}

#[test]
fn persisted_directory_state_tracks_open_tabs_and_preserves_other_settings() {
    let first = Fixture::new();
    let second = Fixture::new();
    let mut b = browser();
    b.open_directory(&first.0).unwrap();
    b.new_tab(Some(&second.0));

    let mut config = PunksConfig {
        volume: 0.42,
        ..PunksConfig::default()
    };
    b.update_config(&mut config);

    assert_eq!(config.tabs, vec![first.0.clone(), second.0.clone()]);
    assert_eq!(config.active_tab, 1);
    assert_eq!(config.last_directory, Some(second.0.clone()));
    assert_eq!(config.volume, 0.42);
}

#[cfg(unix)]
#[test]
fn fallback_asset_ids_distinguish_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;

    let b = browser();
    let left = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()));
    let right = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xfe".to_vec()));

    let left_id = b.asset_identity(&left);
    assert_eq!(left_id, b.asset_identity(&left));
    assert_ne!(left_id, b.asset_identity(&right));
    assert_eq!(left_id.len(), 69);
}
