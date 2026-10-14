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
use rustc_hash::FxHashMap;
pub use yf_core::ControlSizes;
use yf_core::{
    Core, EdgeInsets, EventKind, EventPayload, ListBind, MountBatch, NodeId, Op, PayloadValue,
    Size, TextEngine,
};
pub use yf_js::{BYTECODE_VERSION, ColorScheme, Environment, HttpRequest, HttpResponse, Insets};
use yf_js::{Host, JsError, NetworkQueue, RequestId};

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

    /// Reads a bundle written by `yf build`: `bundle.qbc` (bytecode) or any
    /// other file as JS source, plus `bundle.js.map` next to it if present.
    pub fn read(path: &std::path::Path) -> io::Result<Bundle> {
        let bytes = std::fs::read(path)?;
        let bundle = if path.extension().is_some_and(|e| e == "qbc") {
            Bundle::bytecode(bytes)
        } else {
            Bundle::source(String::from_utf8(bytes).map_err(io::Error::other)?)
        };
        Ok(
            match std::fs::read_to_string(path.with_file_name("bundle.js.map")) {
                Ok(map) if !map.is_empty() => bundle.with_source_map(map),
                _ => bundle,
            },
        )
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

    /// JS asked for a display frame (`requestAnimationFrame`): call
    /// [`EngineControl::frame`] at the next vsync. Requests are not repeated
    /// until that call.
    fn request_frame(&self) {}

    /// A development update ([`EngineControl::hot_update`]) could not be
    /// applied in place: reload the latest full bundle.
    fn hot_update_failed(&self) {}
}

/// Completes a request started by [`Network::fetch`]: a response, or a
/// message describing a network failure.
pub type FetchDone = Box<dyn FnOnce(Result<HttpResponse, String>) + Send>;

/// Cancels a request started by [`Network::fetch`].
pub type CancelFetch = Box<dyn FnOnce() + Send>;

/// The platform's HTTP stack, used by JS `fetch`.
pub trait Network: Send + Sync + 'static {
    /// Starts `request`. `done` is called once, on any thread; after a
    /// cancellation it may still be called (and is then ignored).
    fn fetch(&self, request: HttpRequest, done: FetchDone) -> CancelFetch;
}

/// Reads the app's bundled files (`asset:/…` URIs): from the app's
/// resources, or from the dev server during development.
pub trait Assets: Send + Sync + 'static {
    /// The bytes of `uri`. Blocking; called on the engine thread.
    fn read(&self, uri: &str) -> Result<Vec<u8>, String>;
}

/// Engine settings.
#[derive(Clone)]
pub struct EngineConfig {
    /// The app bundle.
    pub bundle: Bundle,
    /// Initial size of the root view.
    pub viewport: Size,
    /// Log per-commit timings (target `yf::perf`, level info).
    pub trace: bool,
    /// The HTTP stack behind `fetch`; without one, requests fail.
    pub network: Option<Arc<dyn Network>>,
    /// Source of bundled files such as fonts; without one, they fail to load.
    pub assets: Option<Arc<dyn Assets>>,
    /// The surface's initial environment (its `window` is `viewport`).
    pub environment: Environment,
    /// Height of the platform's screen header bar, normal and with a large
    /// title (status bar excluded).
    pub header_heights: (f32, f32),
    /// Natural sizes of the platform's controls.
    pub control_sizes: ControlSizes,
}

impl std::fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineConfig")
            .field("bundle", &self.bundle)
            .field("viewport", &self.viewport)
            .field("trace", &self.trace)
            .field("network", &self.network.is_some())
            .field("assets", &self.assets.is_some())
            .field("environment", &self.environment)
            .field("header_heights", &self.header_heights)
            .field("control_sizes", &self.control_sizes)
            .finish()
    }
}

enum Message {
    Event(NodeId, EventKind, EventPayload),
    Scroll(NodeId, f32, f32, u32),
    AnimationsFinished(Vec<yf_anim::Finished>),
    Resize(Size),
    Environment(Environment),
    RootSize(NodeId, Size, EdgeInsets),
    Frame,
    FetchDone(RequestId, Result<HttpResponse, String>),
    Hot(HotUpdate),
    Shutdown,
}

/// A development update that replaces the app's changed modules in place,
/// keeping the JS context, the views and the state of everything else
/// (made by `yf_bundler::HotBundle::patch`).
#[derive(Debug, Clone)]
pub struct HotUpdate {
    /// Script name in stack traces, unique per update (e.g. `hot-3.js`).
    pub name: String,
    /// The script.
    pub code: String,
    /// Its source map (JSON).
    pub source_map: Option<Arc<str>>,
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

    /// Reports the content offset of a scroll view or list, and how many
    /// scrolls the core asked for (`MountItem::ScrollBy` items and
    /// `Command::ScrollTo` commands for the view) were applied before it.
    /// Cheap to call on every scroll frame: offsets are coalesced per
    /// engine frame.
    pub fn scroll(&self, id: NodeId, x: f32, y: f32, applied: u32) {
        let _ = self.tx.send(Message::Scroll(id, x, y, applied));
    }

    /// Reports native animations that finished or were interrupted.
    pub fn animations_finished(&self, finished: Vec<yf_anim::Finished>) {
        let _ = self.tx.send(Message::AnimationsFinished(finished));
    }

    /// Resizes the root view.
    pub fn resize(&self, size: Size) {
        let _ = self.tx.send(Message::Resize(size));
    }

    /// Reports a change of the surface's environment (safe area, keyboard,
    /// appearance...).
    pub fn set_environment(&self, environment: Environment) {
        let _ = self.tx.send(Message::Environment(environment));
    }

    /// The platform presents root `id` (a modal) at `size`, with `insets`
    /// of it covered by system UI.
    pub fn set_root_size(&self, id: NodeId, size: Size, insets: EdgeInsets) {
        let _ = self.tx.send(Message::RootSize(id, size, insets));
    }

    /// Applies a development update in place. When it cannot be applied,
    /// [`MountSink::hot_update_failed`] asks the platform to reload.
    pub fn hot_update(&self, update: HotUpdate) {
        let _ = self.tx.send(Message::Hot(update));
    }

    /// A display frame began, after [`MountSink::request_frame`]: runs the
    /// `requestAnimationFrame` callbacks.
    pub fn frame(&self) {
        let _ = self.tx.send(Message::Frame);
    }
}

/// A running engine. Dropping it stops the thread.
pub struct Engine {
    control: EngineControl,
    thread: Option<JoinHandle<()>>,
    /// Interrupts running JS when the engine stops.
    stop: Arc<std::sync::atomic::AtomicBool>,
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
        let engine_tx = tx.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let engine_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("yf-engine".into())
            .stack_size(ENGINE_STACK_SIZE)
            .spawn(move || run(config, make_text_engine(), sink, engine_tx, rx, engine_stop))?;
        Ok(Engine {
            control: EngineControl { tx },
            thread: Some(thread),
            stop,
        })
    }

    /// A handle for sending input from other threads.
    pub fn control(&self) -> EngineControl {
        self.control.clone()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Dropped on the platform's main thread: a script still running is
        // aborted rather than waited for.
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
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
    tx: Sender<Message>,
    rx: Receiver<Message>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    raise_thread_priority();
    let sink = Rc::new(sink);
    let mut host = match Host::new() {
        Ok(host) => {
            host.set_stop_flag(stop);
            host
        }
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
    let mut core = Core::new(text_engine, config.viewport);
    core.set_font_scale(config.environment.font_scale);
    core.set_header_heights(config.header_heights.0, config.header_heights.1);
    core.set_control_sizes(config.control_sizes);
    core.set_safe_area(insets(&config.environment));
    let mut state = State {
        core,
        frame_requested: false,
        ops: Vec::new(),
        binds: Vec::new(),
        events: Vec::new(),
        trace: config.trace,
        committed_since_gc: false,
        hot: Vec::new(),
        scroll_events: FxHashMap::default(),
        net: Net {
            network: config.network.clone(),
            tx,
            queue: NetworkQueue::default(),
            inflight: FxHashMap::default(),
        },
    };
    let mut last_gc = Instant::now();

    let mut environment = config.environment;
    environment.window = (config.viewport.width, config.viewport.height);
    host.set_environment(&environment);
    let start = Instant::now();
    let loaded: Result<(), JsError> = match &config.bundle.code {
        BundleCode::Bytecode(bytes) => host.eval_bytecode(bytes),
        BundleCode::Source(code) => host.eval_source(code),
    };
    if let Err(err) = loaded {
        report("bundle failed", &err.message);
    }
    let eval_us = start.elapsed().as_micros();
    state.load_fonts(&host, config.assets.as_deref(), &report);
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
    state.net.pump(&host);
    state.request_frame(&host, &*sink);

    loop {
        // Wake for the next timer, or for an idle collection if one is due.
        let gc_deadline = state.committed_since_gc.then(|| last_gc + IDLE_GC_INTERVAL);
        let deadline = [host.next_timer(), gc_deadline, state.next_scroll_event()]
            .into_iter()
            .flatten()
            .min();
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
        state.dispatch_scroll_events(&host);
        for update in std::mem::take(&mut state.hot) {
            if let Some(map) = update.source_map {
                symbolicator.borrow_mut().add(&update.name, map);
            }
            let start = Instant::now();
            match host.eval_script(&update.name, &update.code) {
                Ok(true) => log::info!("hot update applied in {}µs", start.elapsed().as_micros()),
                Ok(false) => sink.hot_update_failed(),
                // The app keeps the modules that ran; the next update can
                // fix the rest.
                Err(err) => report("hot update failed", &err.message),
            }
        }
        host.run_timers(Instant::now());
        state.load_fonts(&host, config.assets.as_deref(), &report);
        if let Some(batch) = state.commit(&host) {
            sink.submit(batch);
        }
        state.net.pump(&host);
        state.request_frame(&host, &*sink);
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

struct ScrollEvent {
    x: f32,
    y: f32,
    content: Size,
    viewport: Size,
    throttle: Duration,
    last: Option<Instant>,
    /// An offset not reported yet.
    pending: bool,
}

struct State<M: TextEngine> {
    core: Core<M>,
    /// A display frame was requested and has not begun yet.
    frame_requested: bool,
    ops: Vec<Op>,
    /// List render requests not yet sent to JS.
    binds: Vec<ListBind>,
    /// Events raised by the core (e.g. `onEndReached`, `onLayout`).
    events: Vec<(NodeId, EventKind, EventPayload)>,
    trace: bool,
    committed_since_gc: bool,
    net: Net,
    /// Development updates to apply.
    hot: Vec<HotUpdate>,
    /// `onScroll` events: the latest offset per scroll view, sent once per
    /// frame at most (and not more often than the view's throttle).
    scroll_events: FxHashMap<NodeId, ScrollEvent>,
}

/// Requests between JS and the platform's network stack.
struct Net {
    network: Option<Arc<dyn Network>>,
    tx: Sender<Message>,
    queue: NetworkQueue,
    inflight: FxHashMap<RequestId, CancelFetch>,
}

impl Net {
    /// Hands the requests JS made to the network, and cancels aborted ones.
    fn pump(&mut self, host: &Host) {
        host.take_network(&mut self.queue);
        if self.queue.is_empty() {
            return;
        }
        for id in self.queue.cancels.drain(..) {
            if let Some(cancel) = self.inflight.remove(&id) {
                cancel();
            }
        }
        for (id, request) in self.queue.requests.drain(..) {
            let Some(network) = &self.network else {
                let _ = self.tx.send(Message::FetchDone(
                    id,
                    Err("this host has no network stack".to_owned()),
                ));
                continue;
            };
            let tx = self.tx.clone();
            let done: FetchDone = Box::new(move |result| {
                let _ = tx.send(Message::FetchDone(id, result));
            });
            self.inflight.insert(id, network.fetch(request, done));
        }
    }
}

impl<M: TextEngine> State<M> {
    /// When a throttled `onScroll` is due.
    fn next_scroll_event(&self) -> Option<Instant> {
        self.scroll_events
            .values()
            .filter(|e| e.pending)
            .filter_map(|e| e.last.map(|t| t + e.throttle))
            .min()
    }

    /// Sends the `onScroll` events that are due.
    fn dispatch_scroll_events(&mut self, host: &Host) {
        if self.scroll_events.is_empty() {
            return;
        }
        let now = Instant::now();
        // Views that stopped listening (or are gone).
        let core = &self.core;
        self.scroll_events
            .retain(|&id, _| core.scroll_listener(id).is_some());
        for (&id, e) in &mut self.scroll_events {
            if !e.pending || e.last.is_some_and(|t| now < t + e.throttle) {
                continue;
            }
            e.pending = false;
            e.last = Some(now);
            let n = |v: f32| PayloadValue::Number(v as f64);
            host.dispatch_event(
                id,
                EventKind::Scroll,
                &EventPayload::Map(vec![
                    ("x", n(e.x)),
                    ("y", n(e.y)),
                    ("contentWidth", n(e.content.width)),
                    ("contentHeight", n(e.content.height)),
                    ("width", n(e.viewport.width)),
                    ("height", n(e.viewport.height)),
                ]),
            );
        }
    }

    /// Asks the platform for a frame when JS waits for one.
    fn request_frame(&mut self, host: &Host, sink: &dyn MountSink) {
        if !self.frame_requested && host.wants_frame() {
            self.frame_requested = true;
            sink.request_frame();
        }
    }

    /// Registers fonts imported by JS. Blocking: text using them must not be
    /// measured with a fallback font first.
    fn load_fonts(
        &mut self,
        host: &Host,
        assets: Option<&dyn Assets>,
        report: &dyn Fn(&str, &str),
    ) {
        let mut fonts = Vec::new();
        host.take_fonts(&mut fonts);
        for uri in fonts {
            let result = match assets {
                Some(assets) => assets.read(&uri),
                None => Err("this host cannot load assets".to_owned()),
            }
            .and_then(|data| self.core.register_font(&data));
            if let Err(err) = result {
                report("font failed", &format!("{uri}: {err}"));
            }
        }
    }

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
            for (id, event, payload) in self.events.drain(..) {
                host.dispatch_event(id, event, &payload);
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

fn insets(env: &Environment) -> EdgeInsets {
    let a = env.safe_area;
    EdgeInsets {
        top: a.top,
        right: a.right,
        bottom: a.bottom,
        left: a.left,
    }
}

/// Returns false on shutdown.
fn handle<M: TextEngine>(host: &Host, state: &mut State<M>, message: Message) -> bool {
    match message {
        Message::Event(id, event, payload) => {
            if state.trace {
                log::info!(target: "yf::perf", "event {event:?} on view {id}");
            }
            // The core tracks input text for layout and to recognize stale
            // values from JS.
            if let (EventKind::ChangeText, EventPayload::Text { text, count }) = (event, &payload) {
                state.core.input_changed(id, text, *count);
            }
            // Likewise for switches and sliders the user moved.
            if event == EventKind::ValueChange {
                match payload {
                    EventPayload::Value(PayloadValue::Bool(on)) => {
                        state.core.control_changed(id, if on { 1.0 } else { 0.0 })
                    }
                    EventPayload::Value(PayloadValue::Number(v)) => {
                        state.core.control_changed(id, v as f32)
                    }
                    _ => {}
                }
            }
            host.dispatch_event(id, event, &payload)
        }
        Message::Scroll(id, x, y, applied) => {
            state.core.set_scroll_offset(id, x, y, applied);
            if let Some((throttle_ms, content, viewport)) = state.core.scroll_listener(id) {
                let event = state.scroll_events.entry(id).or_insert(ScrollEvent {
                    x,
                    y,
                    content,
                    viewport,
                    throttle: Duration::from_secs_f32(throttle_ms / 1000.0),
                    last: None,
                    pending: true,
                });
                (event.x, event.y, event.content, event.viewport) = (x, y, content, viewport);
                event.throttle = Duration::from_secs_f32(throttle_ms / 1000.0);
                event.pending = true;
            }
        }
        Message::AnimationsFinished(finished) => host.animations_finished(&finished),
        Message::Resize(size) => state.core.set_viewport(size),
        Message::Environment(env) => {
            state.core.set_font_scale(env.font_scale);
            state.core.set_safe_area(insets(&env));
            host.set_environment(&env);
        }
        Message::RootSize(id, size, insets) => state.core.set_root_size(id, size, insets),
        Message::Frame => {
            state.frame_requested = false;
            host.run_frame();
        }
        Message::FetchDone(id, result) => {
            // Results of cancelled requests are dropped.
            if state.net.inflight.remove(&id).is_some() || state.net.network.is_none() {
                host.finish_fetch(id, result);
            }
        }
        Message::Hot(update) => state.hot.push(update),
        Message::Shutdown => return false,
    }
    true
}
