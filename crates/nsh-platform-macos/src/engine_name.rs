//! UNAPPLIED diagnostic candidate; positive startup observations only.
use std::{
    cell::Cell,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_START: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static LOAD_VIEW: Cell<Option<usize>> = const { Cell::new(None) };
}

// A copied address only, scoped to the synchronous public load call. No row,
// retained owner, stop/join change, or cross-thread ownership is introduced.
pub(crate) struct LoadScope(Option<usize>);
impl LoadScope {
    pub(crate) fn new(view: usize) -> Self {
        Self(LOAD_VIEW.with(|current| current.replace(Some(view))))
    }
}
impl Drop for LoadScope {
    fn drop(&mut self) {
        LOAD_VIEW.with(|current| current.set(self.0));
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Start {
    key: u64,
    id: u64,
}

fn record(phase: &str, start: Option<Start>, view: Option<usize>, detail: serde_json::Value) {
    let mut tid = 0;
    // SAFETY: Darwin's zero pthread_t selects this thread, and tid is writable.
    let tid_rc = unsafe { libc::pthread_threadid_np(0, &mut tid) };
    let row = serde_json::json!({
        "schema":"p2-nsh-1", "pid":std::process::id(), "phase":phase,
        "key":start.map(|s| s.key), "start":start.map(|s| s.id), "view":view,
        "os_tid":tid, "tid_rc":tid_rc, "detail":detail,
    });
    // A failed or partial row supplies no binding. A later row is not a
    // certificate of losslessness; qualification requires each positive fact.
    let _ = writeln!(std::io::stderr().lock(), "P2-NSH {row}");
}

pub(crate) fn view_created(view: usize, key: u64) {
    record(
        "view.created",
        None,
        Some(view),
        serde_json::json!({"key":key}),
    );
}

/// Call only AFTER the existing self.stop() has returned. Exhaustion does not
/// wrap or suppress ordinary startup; it makes this diagnostic start unbound.
pub(crate) fn next_start(key: u64) -> Option<Start> {
    match NEXT_START.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1)) {
        Ok(id) => Some(Start { key, id }),
        Err(_) => {
            record(
                "unknown.start_exhausted",
                None,
                None,
                serde_json::json!({"key":key}),
            );
            None
        }
    }
}

pub(crate) fn engine_enter(start: Option<Start>) {
    let Some(start) = start else { return };
    // Width follows the u64 namespace, not a presentation tuning value:
    // 4 + 16 + 1 + 16 = 37 ASCII bytes; Darwin permits 63 payload bytes.
    let token = format!("nshK{:016x}S{:016x}", start.key, start.id);
    let name = std::ffi::CString::new(token.as_bytes()).expect("hex token has no NUL");
    let mut readback = [0_u8; 64];
    // SAFETY: setter affects only the current engine thread; both C buffers
    // are live, correctly sized, and the input is NUL terminated.
    let set_rc = unsafe { libc::pthread_setname_np(name.as_ptr()) };
    let get_rc = unsafe {
        libc::pthread_getname_np(
            libc::pthread_self(),
            readback.as_mut_ptr().cast(),
            readback.len(),
        )
    };
    let actual = std::ffi::CStr::from_bytes_until_nul(&readback).ok();
    let exact = get_rc == 0 && actual.is_some_and(|s| s.to_bytes() == token.as_bytes());
    record(
        "engine.named",
        Some(start),
        None,
        serde_json::json!({
            "token":token, "set_rc":set_rc, "get_rc":get_rc, "exact":exact,
            "readback_bytes":actual.map(|s| s.to_bytes()),
        }),
    );
}

pub(crate) fn installed(start: Option<Start>, success: bool) {
    record(
        if success {
            "start.installed"
        } else {
            "start.failed"
        },
        start,
        LOAD_VIEW.with(Cell::get),
        serde_json::Value::Null,
    );
}
