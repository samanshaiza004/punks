use caliber_ffi::*;
use punks_caliber::*;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new(count: usize) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "punks-contract-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        for i in 0..count {
            std::fs::write(p.join(format!("kick-{i:04}.wav")), b"invalid audio").unwrap();
        }
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Client {
    session: *mut PunksSession,
    context: *mut CaliberContext,
    waiter: Option<std::thread::JoinHandle<()>>,
    rx: mpsc::Receiver<()>,
}
impl Client {
    fn new() -> Self {
        let mut session = std::ptr::null_mut();
        let mut context = std::ptr::null_mut();
        assert_eq!(
            unsafe { punks_create(1, 3, &mut session, &mut context) },
            CaliberStatus::Ok
        );
        let (tx, rx) = mpsc::sync_channel(1);
        let address = context as usize;
        let waiter = std::thread::spawn(move || {
            let context = address as *mut CaliberContext;
            let mut sequence = 0;
            loop {
                let status = unsafe { caliber_context_wait_wake(context, sequence, &mut sequence) };
                if status == CaliberStatus::Stopped {
                    break;
                }
                assert_eq!(status, CaliberStatus::Ok);
                let _ = tx.try_send(());
            }
        });
        Self {
            session,
            context,
            waiter: Some(waiter),
            rx,
        }
    }
    fn send(&self, value: Value) {
        let data = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            unsafe { punks_dispatch(self.session, data.as_ptr(), data.len()) },
            CaliberStatus::Ok
        );
    }
    fn state(&self) -> Option<(u64, State)> {
        let mut p = CaliberStatePublication::default();
        if unsafe { caliber_context_read_latest_state(self.context, &mut p) } != CaliberStatus::Ok {
            return None;
        }
        let result =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(p.data, p.len) }).unwrap();
        let rev = p.revision;
        unsafe { caliber_state_publication_release(&mut p) };
        Some((rev, result))
    }
    fn wait(&self, check: impl Fn(&State) -> bool) -> (u64, State) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(s) = self.state() {
                if check(&s.1) {
                    return s;
                }
            }
            self.rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("backend failed to publish expected state");
        }
    }
    fn open(&self, f: &Fixture) -> State {
        self.send(json!({"v":1,"op":"open","path":f.0}));
        self.wait(|s| s.total > 0 && !s.searching).1
    }
    fn window(&self, r: Resource) -> EntryWindow {
        let mut view = CaliberResourceView::default();
        assert_eq!(
            unsafe { caliber_context_map_resource(self.context, r.id, r.generation, &mut view) },
            CaliberStatus::Ok
        );
        let window =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(view.data, view.len) })
                .unwrap();
        unsafe { caliber_resource_release(&mut view) };
        window
    }
    fn stop(&mut self) {
        assert_eq!(unsafe { punks_stop(self.session) }, CaliberStatus::Ok);
        if let Some(w) = self.waiter.take() {
            w.join().unwrap();
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if !self.session.is_null() {
            self.stop();
            assert_eq!(unsafe { punks_destroy(self.session) }, CaliberStatus::Ok);
        }
    }
}
#[test]
fn bounded_windows_selection_search_and_errors_use_the_real_transport() {
    let f = Fixture::new(2300);
    let c = Client::new();
    let s = c.open(&f);
    let w = c.window(s.entries);
    assert_eq!(s.total, 2300);
    assert_eq!(w.entries.len(), 128);
    let row = &w.entries[0];
    c.send(json!({"v":1,"op":"select","id":row.id,"index":row.index,"listing_revision":s.listing_revision,"audition":true}));
    let selected = c.wait(|s| s.selected_index == 0).1;
    assert_eq!(selected.selected.id, row.id);
    assert!(selected.error.contains("disabled"));
    c.send(json!({"v":1,"op":"search","query":"kick-0007"}));
    let searched = c.wait(|s| s.query == "kick-0007" && !s.searching).1;
    assert_eq!(searched.total, 1);
    let before = searched.listing_revision;
    c.send(
        json!({"v":1,"op":"select","id":row.id,"index":0,"listing_revision":s.listing_revision}),
    );
    let rejected = c.wait(|s| s.error.contains("Stale")).1;
    assert_eq!(rejected.listing_revision, before);
    assert_eq!(rejected.total, 1);
    c.send(json!({"v":1,"op":"open","path":f.0.join("missing")}));
    let failed = c
        .wait(|s| !s.error.is_empty() && !s.error.contains("Stale"))
        .1;
    assert_eq!(failed.total, 1);
}
#[test]
fn repeated_commands_are_ordered_and_idle_has_no_publication_timer() {
    let f = Fixture::new(10);
    let c = Client::new();
    let initial = c.open(&f);
    for request in 1..=32 {
        c.send(json!({"v":1,"op":"stop","request_id":request}));
    }
    let (revision, s) = c.wait(|s| s.last_request_id == 32);
    assert_eq!(s.command_sequence, initial.command_sequence + 32);
    std::thread::sleep(Duration::from_millis(30));
    let mut before = 0;
    assert_eq!(
        unsafe { caliber_context_wake_sequence(c.context, &mut before) },
        CaliberStatus::Ok
    );
    std::thread::sleep(Duration::from_millis(100));
    let mut after = 0;
    unsafe { caliber_context_wake_sequence(c.context, &mut after) };
    assert_eq!(before, after);
    assert_eq!(c.state().unwrap().0, revision);
}
#[test]
fn leases_survive_retirement_and_context_destruction_but_stale_handles_fail() {
    let f = Fixture::new(500);
    let mut c = Client::new();
    let s = c.open(&f);
    let mut lease = CaliberResourceView::default();
    assert_eq!(
        unsafe {
            caliber_context_map_resource(c.context, s.entries.id, s.entries.generation, &mut lease)
        },
        CaliberStatus::Ok
    );
    let saved = unsafe { std::slice::from_raw_parts(lease.data, lease.len) }.to_vec();
    for offset in [128, 256, 384, 0, 128] {
        c.send(json!({"v":1,"op":"window","offset":offset,"listing_revision":s.listing_revision}));
        c.wait(|state| state.offset == offset);
    }
    let mut stale = CaliberResourceView::default();
    let status = unsafe {
        caliber_context_map_resource(c.context, s.entries.id, s.entries.generation, &mut stale)
    };
    assert!(matches!(
        status,
        CaliberStatus::Stale | CaliberStatus::NotFound
    ));
    let mut state_lease = CaliberStatePublication::default();
    unsafe { caliber_context_read_latest_state(c.context, &mut state_lease) };
    c.stop();
    assert_eq!(
        unsafe { punks_dispatch(c.session, std::ptr::null(), 0) },
        CaliberStatus::Stopped
    );
    assert_eq!(unsafe { punks_destroy(c.session) }, CaliberStatus::Ok);
    c.session = std::ptr::null_mut();
    assert_eq!(
        unsafe { std::slice::from_raw_parts(lease.data, lease.len) },
        saved
    );
    assert!(state_lease.len > 0);
    unsafe {
        caliber_resource_release(&mut lease);
        caliber_state_publication_release(&mut state_lease)
    };
    assert!(lease.lease.is_null());
    assert!(state_lease.lease.is_null());
}
#[test]
fn malformed_oversized_and_versioned_input_preserve_valid_domain_state() {
    let f = Fixture::new(1);
    let c = Client::new();
    let s = c.open(&f);
    let malformed = b"{broken";
    assert_eq!(
        unsafe { punks_dispatch(c.session, malformed.as_ptr(), malformed.len()) },
        CaliberStatus::Ok
    );
    c.wait(|s| s.error.starts_with("Invalid command"));
    c.send(json!({"v":99,"op":"up"}));
    let invalid = c.wait(|s| s.error.contains("Unsupported")).1;
    assert_eq!(invalid.directory, s.directory);
    let data = vec![0; MAX_COMMAND + 1];
    assert_eq!(
        unsafe { punks_dispatch(c.session, data.as_ptr(), data.len()) },
        CaliberStatus::LimitExceeded
    );
    let mut session = std::ptr::null_mut();
    let mut context = std::ptr::null_mut();
    assert_eq!(
        unsafe { punks_create(99, 0, &mut session, &mut context) },
        CaliberStatus::UnsupportedVersion
    );
    assert!(session.is_null());
}
#[test]
fn repeated_open_close_and_shutdown_during_search_are_clean() {
    let f = Fixture::new(1000);
    for _ in 0..8 {
        let c = Client::new();
        c.open(&f);
        for q in ["kick-0", "kick-1", "kick-2"] {
            c.send(json!({"v":1,"op":"search","query":q}));
        }
        drop(c);
    }
}
