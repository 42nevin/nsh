//! DRAFT ONLY: opt-in construction identity records, no JS boundary.
use std::{
    ffi::c_void,
    io::Write,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

const LIMIT: u64 = 256;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static NEXT_START: AtomicU64 = AtomicU64::new(1);
static INCOMPLETE: AtomicBool = AtomicBool::new(false);
static WRITE_ERRORS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub(crate) struct Start {
    surface: u64,
    id: u64,
}

unsafe extern "C" {
    fn pthread_threadid_np(thread: *mut c_void, thread_id: *mut u64) -> i32;
}

fn record(phase: &str, view: Option<usize>, surface: Option<u64>, start: Option<u64>) {
    let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    if seq >= LIMIT {
        INCOMPLETE.store(true, Ordering::Relaxed);
        if seq > LIMIT {
            return;
        }
    }
    let mut tid = 0;
    // SAFETY: pthread.h accepts null for the caller and a writable uint64.
    let rc = unsafe { pthread_threadid_np(std::ptr::null_mut(), &mut tid) };
    if rc != 0 || surface == Some(0) || view == Some(0) {
        INCOMPLETE.store(true, Ordering::Relaxed);
    }
    let row = serde_json::json!({
        "schema":1, "seq":seq, "pid":std::process::id(), "phase":phase,
        "view":view, "surface":surface, "start":start,
        "os_tid":tid, "tid_api_rc":rc,
        "incomplete":INCOMPLETE.load(Ordering::Relaxed),
        "through_seq":(phase == "stop.checkpoint").then_some(seq),
        "write_errors":WRITE_ERRORS.load(Ordering::Relaxed),
    });
    // No diagnostic borrow/lock survives this call or spans Engine::start/stop.
    if writeln!(std::io::stderr().lock(), "H1-NSH {row}").is_err() {
        WRITE_ERRORS.fetch_add(1, Ordering::Relaxed);
        INCOMPLETE.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn view_created(view: usize, surface: u64) {
    record("view.created", Some(view), Some(surface), None);
}

pub(crate) fn start_attempt(surface: u64) -> Option<Start> {
    match NEXT_START.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1)) {
        Ok(id) => {
            record("start.attempt", None, Some(surface), Some(id));
            Some(Start { surface, id })
        }
        Err(_) => {
            INCOMPLETE.store(true, Ordering::Relaxed);
            record("unknown.start_overflow", None, Some(surface), None);
            None
        }
    }
}

pub(crate) fn engine_enter(start: Option<Start>) {
    record(
        "engine.enter",
        None,
        start.map(|s| s.surface),
        start.map(|s| s.id),
    );
}

pub(crate) fn start_returned(start: Option<Start>, success: bool) {
    let phase = if success {
        "start.success"
    } else {
        "start.failed"
    };
    record(phase, None, start.map(|s| s.surface), start.map(|s| s.id));
}

/// Covers allocated NSH sequences through this row before the Main-only join.
pub(crate) fn stop_checkpoint(surface: u64) {
    record("stop.checkpoint", None, Some(surface), None);
}
