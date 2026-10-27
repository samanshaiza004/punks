use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::JoinHandle;

pub type WakeCallback = Arc<dyn Fn() + Send + Sync + 'static>;
pub(crate) struct Workers {
    pub cancel: Arc<AtomicBool>,
    pub wake: WakeCallback,
    handles: Vec<JoinHandle<()>>,
}
impl Workers {
    pub fn new(wake: WakeCallback) -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            wake,
            handles: vec![],
        }
    }
    pub fn spawn(&mut self, work: impl FnOnce(Arc<AtomicBool>, WakeCallback) + Send + 'static) {
        let mut i = 0;
        while i < self.handles.len() {
            if self.handles[i].is_finished() {
                let _ = self.handles.swap_remove(i).join();
            } else {
                i += 1;
            }
        }
        let cancel = self.cancel.clone();
        let wake = self.wake.clone();
        self.handles.push(std::thread::spawn(move || {
            work(cancel, wake.clone());
            wake();
        }));
    }
    pub fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Release);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}
pub(crate) struct Completion<T> {
    tx: mpsc::SyncSender<T>,
    wake: WakeCallback,
}
impl<T> Completion<T> {
    pub fn channel(capacity: usize, wake: WakeCallback) -> (Self, mpsc::Receiver<T>) {
        let (tx, rx) = mpsc::sync_channel(capacity);
        (Self { tx, wake }, rx)
    }
    pub fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        let result = self.tx.send(value);
        (self.wake)();
        result
    }
}
