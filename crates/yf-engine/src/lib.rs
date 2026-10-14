//! The engine thread of one surface.
//!
//! The engine owns a [`yf_js::Host`] and a [`yf_core::Core`] on a dedicated
//! thread. Each wake-up (input event, timer, resize) runs JS, drains the
//! microtask queue, applies the committed ops, lays out, and hands one
//! [`MountBatch`] to the platform through a [`MountSink`]. The platform
//! thread never blocks on JS.

mod symbolicate;

use std::{
    cell::RefCell,
    io,
    rc::Rc,
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use yf_core::{Core, EventKind, EventPayload, ListBind, MountBatch, NodeId, Op, Size, TextEngine};
pub use yf_js::BYTECODE_VERSION;
use yf_js::{Host, JsError};

use crate::symbolicate::Symbolicator;

/// Stack size of the engine thread. QuickJS recursion (and deep Solid
/// component trees) needs more than the 512 KiB default of secondary
/// threads on Apple platforms.
const ENGINE_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Minimum time between idle cycle collections. QuickJS frees acyclic
/// garbage immediately; only cycles (such as detached node subtrees, whose
/// parent/child links reference each other) wait for the collector.
const IDLE_GC_INTERVAL: Duration = Duration::from_secs(1);

/// An app bundle: its code plus, in development, a source map.
#[derive(Debug, Clone)]
pub struct Bundle {
    /// The code.
    pub code: BundleCode,
    /// Source map (JSON) used to show app source locations in stack traces.
    pub source_map: Option<Arc<str>>,
}

/// The code of a bundle.
#[derive(Debug, Clone)]
pub enum BundleCode {
    /// QuickJS bytecode from `yf_js::compile_bytecode`.
    Bytecode(Vec<u8>),
    /// JS source; slower to start than bytecode.
    Source(String),
}

impl Bundle {
    /// A bytecode bundle.
    pub fn bytecode(bytes: Vec<u8>) -> Self {
        Self {
            code: BundleCode::Bytecode(bytes),
            source_map: None,
        }
    }

    /// A JS source bundle.
    pub fn source(code: String) -> Self {
        Self {
            code: BundleCode::Source(code),
            source_map: None,
        }
    }

    /// Attaches a source map.
    pub fn with_source_map(mut self, map: impl Into<Arc<str>>) -> Self {
        self.source_map = Some(map.into());
        self
    }
}

/// Receives mount batches on the engine thread. Implementations forward
/// them to the UI thread.
pub trait MountSink: Send + 'static {
    /// Called once per commit, in order.
    fn submit(&self, batch: MountBatch);

    /// A fatal app error: the bundle failed to load, or an exception escaped
    /// a JS callback. Errors are always logged; development hosts also show
    /// them on screen.
    fn report_error(&self, _message: &str) {}
}

/// Engine settings.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// The app bundle.
    pub bundle: Bundle,
    /// Initial size of the root view.
    pub viewport: Size,
    /// Log per-commit timings (target `yf::perf`, level info).
    pub trace: bool,
}

enum Message {
    Event(NodeId, EventKind, EventPayload),
    Scroll(NodeId, f32, f32),
    AnimationsFinished(Vec<yf_anim::Finished>),
    Resize(Size),
    Shutdown,
}

/// Rounds of "list asks for items -> JS renders them -> layout" folded into
/// one frame. Bounded so a list that never settles cannot stall the engine;
/// leftovers continue next frame.
const MAX_LIST_ROUNDS: usize = 8;

/// A cheap, cloneable handle for sending input to the engine from any thread.
#[derive(Clone)]
pub struct EngineControl {
    tx: Sender<Message>,
}

impl EngineControl {
    /// Delivers an input event (and its data) to a node.
    pub fn dispatch_event(&self, id: NodeId, event: EventKind, payload: EventPayload) {
        let _ = self.tx.send(Message::Event(id, event, payload));
    }

    /// Reports the content offset of a scroll view or list. Cheap to call
    /// on every scroll frame: offsets are coalesced per engine frame.
    pub fn scroll(&self, id: NodeId, x: f32, y: f32) {
        let _ = self.tx.send(Message::Scroll(id, x, y));
    }

    /// Reports native animations that finished or were interrupted.
    pub fn animations_finished(&self, finished: Vec<yf_anim::Finished>) {
        let _ = self.tx.send(Message::AnimationsFinished(finished));
    }

    /// Resizes the root view.
    pub fn resize(&self, size: Size) {
        let _ = self.tx.send(Message::Resize(size));
    }
}

/// A running engine. Dropping it stops the thread.
pub struct Engine {
    control: EngineControl,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the engine thread. `make_text_engine` runs on that thread.
    pub fn start<M, F, S>(config: EngineConfig, make_text_engine: F, sink: S) -> io::Result<Engine>
    where
        M: TextEngine + 'static,
        F: FnOnce() -> M + Send + 'static,
        S: MountSink,
    {
        let (tx, rx) = crossbeam_channel::unbounded();
        let thread = thread::Builder::new()
            .name("yf-engine".into())
            .stack_size(ENGINE_STACK_SIZE)
            .spawn(move || run(config, make_text_engine(), sink, rx))?;
        Ok(Engine {
            control: EngineControl { tx },
            thread: Some(thread),
        })
    }

    /// A handle for sending input from other threads.
    pub fn control(&self) -> EngineControl {
        self.control.clone()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.control.tx.send(Message::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The engine thread turns input into frames: it is as latency-critical as
/// the UI thread. Without this, Apple platforms may run it on efficiency
/// cores at low clock speeds.
fn raise_thread_priority() {
    #[cfg(target_vendor = "apple")]
    // SAFETY: changes the QoS class of the calling thread only.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

fn run<M: TextEngine, S: MountSink>(
    config: EngineConfig,
    text_engine: M,
    sink: S,
    rx: Receiver<Message>,
) {
    raise_thread_priority();
    let sink = Rc::new(sink);
    let mut host = match Host::new() {
        Ok(host) => host,
        Err(err) => {
            log::error!("failed to start JS: {err}");
            return sink.report_error(&err.message);
        }
    };
    let symbolicator = Rc::new(RefCell::new(Symbolicator::new(
        config.bundle.source_map.clone(),
    )));
    let report = {
        let sink = sink.clone();
        let symbolicator = symbolicator.clone();
        move |what: &str, message: &str| {
            let message = symbolicator.borrow_mut().rewrite(message);
            log::error!(target: "js", "{what}: {message}");
            sink.report_error(&message);
        }
    };
    let uncaught = report.clone();
    host.set_error_handler(Box::new(move |err| {
        uncaught("uncaught exception", &err.message)
    }));
    let mut state = State {
        core: Core::new(text_engine, config.viewport),
        ops: Vec::new(),
        binds: Vec::new(),
        events: Vec::new(),
        trace: config.trace,
        committed_since_gc: false,
    };
    let mut last_gc = Instant::now();

    let start = Instant::now();
    let loaded: Result<(), JsError> = match &config.bundle.code {
        BundleCode::Bytecode(bytes) => host.eval_bytecode(bytes),
        BundleCode::Source(code) => host.eval_source(code),
    };
    if let Err(err) = loaded {
        report("bundle failed", &err.message);
    }
    let eval_us = start.elapsed().as_micros();
    if let Some(batch) = state.commit(&host) {
        if state.trace {
            log::info!(
                target: "yf::perf",
                "first frame: eval {eval_us}µs, total {}µs",
                start.elapsed().as_micros()
            );
        }
        sink.submit(batch);
    }

    loop {
        // Wake for the next timer, or for an idle collection if one is due.
        let gc_deadline = state.committed_since_gc.then(|| last_gc + IDLE_GC_INTERVAL);
        let deadline = match (host.next_timer(), gc_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let message = match deadline {
            Some(deadline) => match rx.recv_deadline(deadline) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(message) => Some(message),
                Err(_) => return,
            },
        };
        if let Some(message) = message {
            if !handle(&host, &mut state, message) {
                return;
            }
            // Coalesce everything already queued into this frame.
            while let Ok(message) = rx.try_recv() {
                if !handle(&host, &mut state, message) {
                    return;
                }
            }
        }
        host.run_timers(Instant::now());
        if let Some(batch) = state.commit(&host) {
            sink.submit(batch);
        }
        if state.committed_since_gc && rx.is_empty() && last_gc.elapsed() >= IDLE_GC_INTERVAL {
            // Idle: collect cycles so released nodes free their views.
            host.collect_garbage();
            last_gc = Instant::now();
            state.committed_since_gc = false;
            if let Some(batch) = state.commit(&host) {
                sink.submit(batch);
            }
        }
    }
}

struct State<M: TextEngine> {
    core: Core<M>,
    ops: Vec<Op>,
    /// List render requests not yet sent to JS.
    binds: Vec<ListBind>,
    /// Events raised by the core (e.g. `onEndReached`).
    events: Vec<(NodeId, EventKind)>,
    trace: bool,
    committed_since_gc: bool,
}

impl<M: TextEngine> State<M> {
    /// Produces one frame: applies JS ops, lays out, and lets lists pull
    /// the items they need, merging everything into a single batch.
    fn commit(&mut self, host: &Host) -> Option<MountBatch> {
        let start = Instant::now();
        let mut merged: Option<MountBatch> = None;
        let mut op_count = 0;
        let mut js = std::time::Duration::ZERO;
        for _ in 0..MAX_LIST_ROUNDS {
            // Requests from the previous round (or frame) first.
            let js_start = Instant::now();
            for bind in self.binds.drain(..) {
                host.bind_list(bind.list, &bind.binds);
            }
            for (id, event) in self.events.drain(..) {
                host.dispatch_event(id, event, &EventPayload::None);
            }
            js += js_start.elapsed();
            host.take_ops(&mut self.ops);
            op_count += self.ops.len();
            for op in self.ops.drain(..) {
                if let Err(err) = self.core.apply(op) {
                    log::error!("invalid op: {err:?}");
                }
            }
            if let Some(batch) = self.core.commit() {
                match &mut merged {
                    Some(m) => m.items.extend(batch.items),
                    None => merged = Some(batch),
                }
            }
            self.core.take_list_binds(&mut self.binds);
            self.core.take_events(&mut self.events);
            if self.binds.is_empty() && self.events.is_empty() {
                break;
            }
        }
        self.committed_since_gc |= op_count > 0;
        if self.trace && (op_count > 0 || merged.is_some()) {
            log::info!(
                target: "yf::perf",
                "commit: {op_count} ops, {}µs (list js {}µs), {} mount items, {} nodes",
                start.elapsed().as_micros(),
                js.as_micros(),
                merged.as_ref().map_or(0, |b| b.items.len()),
                self.core.node_count(),
            );
        }
        merged
    }
}

/// Returns false on shutdown.
fn handle<M: TextEngine>(host: &Host, state: &mut State<M>, message: Message) -> bool {
    match message {
        Message::Event(id, event, payload) => {
            if state.trace {
                log::info!(target: "yf::perf", "event {event:?} on view {id}");
            }
            host.dispatch_event(id, event, &payload)
        }
        Message::Scroll(id, x, y) => state.core.set_scroll_offset(id, x, y),
        Message::AnimationsFinished(finished) => host.animations_finished(&finished),
        Message::Resize(size) => state.core.set_viewport(size),
        Message::Shutdown => return false,
    }
    true
}
