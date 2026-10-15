//! The engine thread of one surface.
//!
//! The engine owns a [`nsh_js::Host`] and a [`nsh_core::Core`] on a dedicated
//! thread. Each wake-up (input event, timer, resize) runs JS, drains the
//! microtask queue, applies the committed ops, lays out, and hands one
//! [`MountBatch`] to the platform through a [`MountSink`]. The platform
//! thread never blocks on JS.

mod access;
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
pub use nsh_core::access::{Access, Capabilities};
use nsh_core::{
    ApplyError, Core, EdgeInsets, EventKind, EventPayload, ListBind, MountBatch, NodeId, Op,
    PayloadValue, Size, TextEngine,
};
pub use nsh_js::{
    BYTECODE_VERSION, ColorScheme, Environment, HttpRequest, HttpResponse, Idiom, Insets, Limits,
};
use nsh_js::{Host, HostOptions, JsError, NetworkQueue, RequestId};
/// Signed packages, release-key certificates, revocations and trust stores.
pub use nsh_package as package;
use rustc_hash::FxHashMap;

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
    /// What the code may do.
    pub policy: Policy,
}

/// What a bundle's code may do.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Run without `eval`, the function constructors and writable shared
    /// prototypes (see `nsh_js::HostOptions::harden`). On by default; off for
    /// development bundles.
    pub harden: bool,
    /// What the app declared (`nsh.json` `capabilities`); `None`: no
    /// declaration, everything.
    pub declared: Option<Capabilities>,
    /// What the embedding app grants at most; `None`: whatever the app
    /// declared.
    pub granted: Option<Capabilities>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            harden: true,
            declared: None,
            granted: None,
        }
    }
}

impl Policy {
    /// Development bundles: `eval` and friends stay (debugging); the
    /// capabilities are still enforced, so a missing one shows during
    /// development.
    pub fn development() -> Self {
        Self {
            harden: false,
            ..Self::default()
        }
    }

    /// What the app may reach under this policy.
    pub fn access(&self) -> Access {
        Access {
            declared: self.declared.clone().unwrap_or_else(Capabilities::all),
            granted: self.granted.clone(),
        }
    }
}

/// The code of a bundle.
#[derive(Debug, Clone)]
pub enum BundleCode {
    /// QuickJS bytecode from `nsh_js::compile_bytecode`.
    Bytecode(Vec<u8>),
    /// JS source; slower to start than bytecode.
    Source(String),
}

impl Bundle {
    /// A bytecode bundle.
    ///
    /// # Safety
    ///
    /// `bytes` must be bytecode from `nsh_js::compile_bytecode` for this
    /// runtime's `BYTECODE_VERSION`, from a trusted source: QuickJS does not
    /// validate bytecode, and loading crafted bytecode corrupts memory. Trusted
    /// are the app's own signed resources, a development server the developer
    /// pointed the host at, and verified packages ([`Bundle::from_package`]).
    pub unsafe fn bytecode(bytes: Vec<u8>) -> Self {
        Self {
            code: BundleCode::Bytecode(bytes),
            source_map: None,
            policy: Policy::default(),
        }
    }

    /// A JS source bundle.
    pub fn source(code: String) -> Self {
        Self {
            code: BundleCode::Source(code),
            source_map: None,
            policy: Policy::default(),
        }
    }

    /// Attaches a source map.
    pub fn with_source_map(mut self, map: impl Into<Arc<str>>) -> Self {
        self.source_map = Some(map.into());
        self
    }

    /// Replaces the policy.
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Reads a JS source bundle written by `nsh build` (`bundle.js`), plus
    /// `bundle.js.map` and the app's `capabilities.json` next to it if
    /// present. Bytecode (`bundle.qbc`) is refused: files can be replaced, and
    /// only [`Bundle::read_trusted`] (the app's own resources) or a verified
    /// package may load bytecode.
    pub fn read(path: &std::path::Path) -> io::Result<Bundle> {
        if path.extension().is_some_and(|e| e == "qbc") {
            return Err(io::Error::other(
                "bytecode only loads from the app's own resources or a signed package (.nshpkg); load bundle.js",
            ));
        }
        // SAFETY: not bytecode.
        unsafe { Self::read_trusted(path) }
    }

    /// Like [`Bundle::read`], also reading `bundle.qbc` as bytecode.
    ///
    /// # Safety
    ///
    /// A `.qbc` file must be trusted as [`Bundle::bytecode`] requires (e.g.
    /// inside the app's own signed bundle).
    pub unsafe fn read_trusted(path: &std::path::Path) -> io::Result<Bundle> {
        let bytes = std::fs::read(path)?;
        let bundle = if path.extension().is_some_and(|e| e == "qbc") {
            // SAFETY: guaranteed by the caller.
            unsafe { Bundle::bytecode(bytes) }
        } else {
            Bundle::source(String::from_utf8(bytes).map_err(io::Error::other)?)
        };
        let mut bundle = match std::fs::read_to_string(path.with_file_name("bundle.js.map")) {
            Ok(map) if !map.is_empty() => bundle.with_source_map(map),
            _ => bundle,
        };
        if let Ok(json) = std::fs::read_to_string(path.with_file_name("capabilities.json")) {
            bundle.policy.declared = Some(parse_capabilities(&json).map_err(io::Error::other)?);
        }
        Ok(bundle)
    }
}

impl Bundle {
    /// The bundle of a verified package: its bytecode when it was compiled
    /// for this runtime (else its source), its source map and its declared
    /// capabilities.
    pub fn from_package(package: &nsh_package::VerifiedPackage) -> Result<Bundle, String> {
        let bundle = match package.bytecode() {
            // SAFETY: verified: signed by a certified release key, hashed,
            // and compiled for this runtime.
            Some(bytecode) => unsafe { Bundle::bytecode(bytecode.to_vec()) },
            None => Bundle::source(package.source.clone()),
        };
        let mut bundle = match &package.source_map {
            Some(map) => bundle.with_source_map(map.clone()),
            None => bundle,
        };
        bundle.policy.declared = package
            .capabilities
            .as_ref()
            .map(Capabilities::from_json)
            .transpose()?;
        Ok(bundle)
    }
}

/// A verified package, unpacked ([`load_package`]).
#[derive(Debug)]
pub struct LoadedPackage {
    /// Its bundle.
    pub bundle: Bundle,
    /// Where its assets are (`asset:/…`).
    pub assets: std::path::PathBuf,
    /// Its app.
    pub app: nsh_package::App,
}

/// Verifies the package `bytes` with `trust`, writes its assets under
/// `assets_root` (`<root>/<app id>/<package hash>/`, once per package) and
/// records its build (older builds of the app are refused from now on).
pub fn load_package(
    trust: &mut nsh_package::TrustStore,
    bytes: &[u8],
    assets_root: &std::path::Path,
) -> Result<LoadedPackage, String> {
    let package = trust
        .verify(bytes, BYTECODE_VERSION)
        .map_err(|e| e.to_string())?;
    let bundle = Bundle::from_package(&package)?;
    let dir = assets_root.join(&package.app.id).join(&package.sha256);
    let done = dir.join(".complete");
    if !done.exists() {
        let assets = dir.join("assets");
        for (path, data) in &package.assets {
            // Paths were checked (relative, no `..`) when the package was read.
            let dest = assets.join(path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&dest, data).map_err(|e| format!("{}: {e}", dest.display()))?;
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(&done, b"").map_err(|e| e.to_string())?;
    }
    trust.accept(&package).map_err(|e| e.to_string())?;
    Ok(LoadedPackage {
        bundle,
        assets: dir.join("assets"),
        app: package.app,
    })
}

/// Parses capabilities in their JSON form (`capabilities.json`, the dev
/// protocol, an embedder's grant).
pub fn parse_capabilities(json: &str) -> Result<Capabilities, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("capabilities: {e}"))?;
    Capabilities::from_json(&value)
}

impl Bundle {
    /// The declared capabilities from their JSON form, if any.
    pub fn with_capabilities_json(mut self, json: Option<&str>) -> Result<Self, String> {
        self.policy.declared = json.map(parse_capabilities).transpose()?;
        Ok(self)
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
    /// Log per-commit timings (target `nsh::perf`, level info).
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
    /// The VM's resource limits.
    pub limits: Limits,
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
            .field("limits", &self.limits)
            .finish()
    }
}

enum Message {
    Event(NodeId, EventKind, EventPayload),
    Scroll(NodeId, f32, f32, u32),
    Zoom(NodeId, f32),
    AnimationsFinished(Vec<nsh_anim::Finished>),
    Resize(Size),
    Environment(Environment),
    RootSize(NodeId, Size, EdgeInsets),
    Frame,
    States(NodeId, u8),
    FetchDone(RequestId, Result<HttpResponse, String>),
    Hot(HotUpdate),
    Shutdown,
}

/// A development update that replaces the app's changed modules in place,
/// keeping the JS context, the views and the state of everything else
/// (made by `nsh_bundler::HotBundle::patch`).
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

    /// Reports the zoom scale of a zoomable scroll view (before the offset
    /// the zoom changed); `onScroll` carries it.
    pub fn zoom(&self, id: NodeId, scale: f32) {
        let _ = self.tx.send(Message::Zoom(id, scale));
    }

    /// Reports native animations that finished or were interrupted.
    pub fn animations_finished(&self, finished: Vec<nsh_anim::Finished>) {
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

    /// Reports the interaction states of view `id` that the core asked to
    /// track ([`nsh_core::MountItem::TrackStates`]; bits of
    /// [`nsh_core::tailwind::state`]). Class styles that depend on them
    /// (`active:`, `hover:`, `focus:`) update without JS.
    pub fn set_states(&self, id: NodeId, states: u8) {
        let _ = self.tx.send(Message::States(id, states));
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
            .name("nsh-engine".into())
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

fn run<M: TextEngine + 'static, S: MountSink>(
    config: EngineConfig,
    text_engine: M,
    sink: S,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    raise_thread_priority();
    let sink = Rc::new(sink);
    let options = HostOptions {
        limits: config.limits,
        harden: config.bundle.policy.harden,
    };
    let mut host = match Host::with_options(options) {
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
    // Shared with canvas `measureText`, which measures on this thread.
    let text_engine = Rc::new(text_engine);
    let measurer = text_engine.clone();
    host.set_text_measurer(Box::new(move |font, text| {
        nsh_core::canvas::measure_line(&*measurer, font, text)
    }));
    let access = Arc::new(config.bundle.policy.access());
    let mut core = Core::new(text_engine, config.viewport);
    core.set_max_nodes(config.limits.max_nodes);
    core.set_access(access.clone());
    core.set_font_scale(config.environment.font_scale);
    core.set_class_environment(class_environment(&config.environment));
    core.set_header_heights(config.header_heights.0, config.header_heights.1);
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
        touch_move: None,
        net: Net {
            network: config.network.clone(),
            tx,
            queue: NetworkQueue::default(),
            inflight: FxHashMap::default(),
            access: access.clone(),
            followed: FxHashMap::default(),
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
    // A VM stopped for good (out of memory, over a quota) is reported once;
    // its views stay as they were and no JS runs any more.
    let mut stopped = false;
    let mut check_stopped = |host: &Host| {
        if !stopped && let Some(reason) = host.fatal() {
            stopped = true;
            report("stopped", &reason);
        }
    };
    state.load_fonts(&host, config.assets.as_deref(), &report);
    if let Some(batch) = state.commit(&host) {
        if state.trace {
            log::info!(
                target: "nsh::perf",
                "first frame: eval {eval_us}µs, total {}µs",
                start.elapsed().as_micros()
            );
        }
        sink.submit(batch);
    }
    state.net.pump(&host);
    state.request_frame(&host, &*sink);
    check_stopped(&host);

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
        state.dispatch_touch_move(&host);
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
        check_stopped(&host);
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
    /// The zoom scale.
    zoom: f32,
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
    /// Touch moves not delivered yet: those queued in one frame become one.
    touch_move: Option<(NodeId, Box<nsh_core::TouchEvent>)>,
}

/// Requests between JS and the platform's network stack.
struct Net {
    network: Option<Arc<dyn Network>>,
    tx: Sender<Message>,
    queue: NetworkQueue,
    inflight: FxHashMap<RequestId, CancelFetch>,
    /// What the app may reach.
    access: Arc<nsh_core::access::Access>,
    /// Requests whose redirects the engine follows (to check each hop).
    followed: FxHashMap<RequestId, access::Followed>,
}

impl Net {
    /// Hands the requests JS made to the network, and cancels aborted ones.
    fn pump(&mut self, host: &Host) {
        host.take_network(&mut self.queue);
        if self.queue.is_empty() {
            return;
        }
        for id in self.queue.cancels.drain(..) {
            self.followed.remove(&id);
            if let Some(cancel) = self.inflight.remove(&id) {
                cancel();
            }
        }
        let requests = std::mem::take(&mut self.queue.requests);
        for (id, request) in requests {
            if self.access.is_unrestricted() {
                self.start(id, request);
            } else if !self.access.allows_http(&request.method, &request.url) {
                host.finish_fetch(id, Err(access::refused(&request.method, &request.url)));
            } else if request.follow_redirects {
                let hop = access::hop(&request);
                self.followed
                    .insert(id, access::Followed { request, hops: 0 });
                self.start(id, hop);
            } else {
                self.start(id, request);
            }
        }
    }

    /// Hands one request (or redirect hop) to the network.
    fn start(&mut self, id: RequestId, request: HttpRequest) {
        let Some(network) = &self.network else {
            let _ = self.tx.send(Message::FetchDone(
                id,
                Err("this host has no network stack".to_owned()),
            ));
            return;
        };
        let tx = self.tx.clone();
        let done: FetchDone = Box::new(move |result| {
            let _ = tx.send(Message::FetchDone(id, result));
        });
        self.inflight.insert(id, network.fetch(request, done));
    }

    /// A hop of request `id` answered: its final result, or `None` when the
    /// next hop was started (or the request was cancelled).
    fn finished(
        &mut self,
        id: RequestId,
        result: Result<HttpResponse, String>,
    ) -> Option<Result<HttpResponse, String>> {
        // Results of cancelled requests are dropped.
        if self.inflight.remove(&id).is_none() && self.network.is_some() {
            return None;
        }
        let Some(followed) = self.followed.get_mut(&id) else {
            return Some(result);
        };
        match access::next(&self.access, followed, result) {
            access::Next::Done(result) => {
                self.followed.remove(&id);
                Some(result)
            }
            access::Next::Hop(request) => {
                self.start(id, request);
                None
            }
        }
    }
}

impl<M: TextEngine> State<M> {
    /// Sends the pending touch move.
    fn dispatch_touch_move(&mut self, host: &Host) {
        if let Some((id, touch)) = self.touch_move.take() {
            host.dispatch_event(id, EventKind::Touch, &EventPayload::Touch(touch));
        }
    }

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
                    ("zoomScale", n(e.zoom)),
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
            // A stop drops the rest of the ops (`drain` clears them).
            for op in self.ops.drain(..) {
                match self.core.apply(op) {
                    Ok(()) => {}
                    Err(err) if err.is_fatal() => {
                        host.stop(match err {
                            ApplyError::TooManyNodes(max) => {
                                format!("the app created more than {max} nodes and was stopped")
                            }
                            other => {
                                format!("the app sent an invalid op ({other:?}) and was stopped")
                            }
                        });
                        break;
                    }
                    Err(err) => log::error!("invalid op: {err:?}"),
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
                target: "nsh::perf",
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
/// What class variants read from the environment.
fn class_environment(env: &Environment) -> nsh_core::tailwind::ClassEnvironment {
    nsh_core::tailwind::ClassEnvironment {
        dark: env.color_scheme == nsh_js::ColorScheme::Dark,
        platform: if cfg!(target_os = "android") {
            nsh_core::tailwind::Platform::Android
        } else {
            nsh_core::tailwind::Platform::Ios
        },
        reduce_motion: env.reduce_motion,
    }
}

fn handle<M: TextEngine>(host: &Host, state: &mut State<M>, message: Message) -> bool {
    match message {
        Message::Event(id, event, payload) => {
            if state.trace {
                log::info!(target: "nsh::perf", "event {event:?} on view {id}");
            }
            // The core tracks input text for layout and to recognize stale
            // values from JS.
            if let (EventKind::ChangeText, EventPayload::Text { text, count }) = (event, &payload) {
                state.core.input_changed(id, text, *count);
            }
            if let EventPayload::Touch(touch) = payload {
                if touch.phase == nsh_core::TouchPhase::Move {
                    match &mut state.touch_move {
                        Some((_, pending)) => pending.merge_move(*touch),
                        None => state.touch_move = Some((id, touch)),
                    }
                    return true;
                }
                // Moves before a start, end or cancel go first.
                state.dispatch_touch_move(host);
                host.dispatch_event(id, event, &EventPayload::Touch(touch));
                return true;
            }
            host.dispatch_event(id, event, &payload)
        }
        Message::Scroll(id, x, y, applied) => {
            state.core.set_scroll_offset(id, x, y, applied);
            if let Some((throttle_ms, content, viewport, zoom)) = state.core.scroll_listener(id) {
                let event = state.scroll_events.entry(id).or_insert(ScrollEvent {
                    x,
                    y,
                    content,
                    viewport,
                    zoom,
                    throttle: Duration::from_secs_f32(throttle_ms / 1000.0),
                    last: None,
                    pending: true,
                });
                (event.x, event.y, event.content, event.viewport, event.zoom) =
                    (x, y, content, viewport, zoom);
                event.throttle = Duration::from_secs_f32(throttle_ms / 1000.0);
                event.pending = true;
            }
        }
        Message::Zoom(id, scale) => state.core.set_zoom(id, scale),
        Message::AnimationsFinished(finished) => host.animations_finished(&finished),
        Message::Resize(size) => state.core.set_viewport(size),
        Message::Environment(env) => {
            state.core.set_font_scale(env.font_scale);
            state.core.set_class_environment(class_environment(&env));
            state.core.set_safe_area(insets(&env));
            host.set_environment(&env);
        }
        Message::RootSize(id, size, insets) => state.core.set_root_size(id, size, insets),
        Message::Frame => {
            state.frame_requested = false;
            host.run_frame();
        }
        Message::States(id, states) => state.core.set_states(id, states),
        Message::FetchDone(id, result) => {
            if let Some(result) = state.net.finished(id, result) {
                host.finish_fetch(id, result);
            }
        }
        Message::Hot(update) => state.hot.push(update),
        Message::Shutdown => return false,
    }
    true
}
