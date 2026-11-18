use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

static TOTAL: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
pub struct Progress {
    total: u64,
    done: u64,
}

pub fn start(bytes: u64) {
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(bytes, Ordering::Relaxed);
}

pub fn advance(bytes: u64) {
    DONE.fetch_add(bytes, Ordering::Relaxed);
}

pub fn read() -> Progress {
    let total = TOTAL.load(Ordering::Relaxed);
    Progress {
        total,
        done: DONE.load(Ordering::Relaxed).min(total),
    }
}
