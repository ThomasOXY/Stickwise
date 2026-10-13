use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Running background jobs, so the UI can cancel them.
#[derive(Default)]
pub struct Jobs {
    next: AtomicU32,
    cancel: Mutex<HashMap<u32, Arc<AtomicBool>>>,
}

impl Jobs {
    pub fn start(&self) -> (u32, Arc<AtomicBool>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let flag = Arc::new(AtomicBool::new(false));
        self.cancel.lock().unwrap().insert(id, flag.clone());
        (id, flag)
    }

    pub fn finish(&self, id: u32) {
        self.cancel.lock().unwrap().remove(&id);
    }

    pub fn cancel(&self, id: u32) {
        if let Some(f) = self.cancel.lock().unwrap().get(&id) {
            f.store(true, Ordering::Relaxed);
        }
    }
}
