//! One owned window and normal AppKit lifecycle for the external SDK oracle.
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

use nsh_platform_macos::{
    AppOptions, AppView, Bundle, ComponentContext, Components, NativeComponent,
    objc2::{
        DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send,
        rc::Retained,
        runtime::{AnyObject, ProtocolObject},
        sel,
    },
    objc2_app_kit::{
        NSAccessibility, NSAccessibilityUnignoredChildren, NSApplication, NSApplicationDelegate,
        NSBackingStoreType, NSBezelStyle, NSButton, NSButtonType, NSEvent, NSEventModifierFlags,
        NSEventType, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
    },
    objc2_foundation::{
        NSArray, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
    },
};
use serde_json::{Value, json};

#[path = "support/mixed_tsx.rs"]
mod mixed_tsx;
type Events = Rc<RefCell<Vec<Value>>>;

fn system_ticks() -> u64 {
    unsafe extern "C" {
        fn mach_absolute_time() -> u64;
    }
    // SAFETY: Darwin's public monotonic process-independent clock.
    unsafe { mach_absolute_time() }
}
fn atomic_json(path: &std::path::Path, value: &Value) {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value).unwrap())
        .expect("write owned fixture IPC");
    std::fs::rename(temporary, path).expect("publish owned fixture IPC");
}
struct Logger;
impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        eprintln!(
            "NSH_LOG {} {} {}",
            record.level(),
            record.target(),
            record.args()
        );
    }
    fn flush(&self) {}
}
static LOGGER: Logger = Logger;

struct WindowOwner {
    window: Retained<NSWindow>,
    app_view: Rc<AppView>,
}
impl WindowOwner {
    fn load(&self, mode: &str, phase: &str) {
        let source = include_str!("MixedComposition.tsx")
            .replace("__CASE__", mode)
            .replace("__INITIAL_PHASE__", phase);
        let code = nsh_bundler::compile_module("MixedComposition.tsx", &source, &[])
            .expect("compile generic AX regression TSX");
        self.app_view.load(Bundle::source(code).into());
    }
}
impl Drop for WindowOwner {
    fn drop(&mut self) {
        self.app_view.stop();
        self.window.orderOut(None);
        self.window.setContentView(None);
        self.window.close();
    }
}
fn make_window(
    mtm: MainThreadMarker,
    title: &str,
    mode: &str,
    events: &Events,
    shared: &Rc<RefCell<mixed_tsx::MixedShared>>,
) -> WindowOwner {
    // SAFETY: Main-thread AppKit initializer; this fixture owns the only window.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(640.0, 480.0)),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: Rust retains and releases the owned window explicitly.
    unsafe {
        window.setReleasedWhenClosed(false);
    }
    // Keep the owned AX target visible when the shared desktop changes Space.
    // Explicit hide/close and subtree visibility still follow normal AppKit behavior.
    window.setCollectionBehavior(NSWindowCollectionBehavior::CanJoinAllSpaces);
    window.setTitle(&NSString::from_str(title));
    let components = Components::default();
    mixed_tsx::register(&components, mtm, events, shared);
    let app_view = AppView::new(
        mtm,
        AppOptions {
            components,
            size: (640, 480),
            data_dir: Some(
                std::env::var_os("NSH_EXTERNAL_AX_STATE")
                    .expect("isolated fixture state")
                    .into(),
            ),
            ..Default::default()
        },
    );
    window.setContentView(Some(&app_view.view()));
    let owner = WindowOwner { window, app_view };
    owner.load(mode, "initial");
    owner.window.makeKeyAndOrderFront(None);
    owner
}
fn application_facts(app: &NSApplication) -> Value {
    // SAFETY: Read-only public AppKit registration and lifecycle getters.
    let current: Retained<AnyObject> =
        unsafe { msg_send![class!(NSRunningApplication), currentApplication] };
    // SAFETY: The lookup uses this fixture process's actual PID.
    let registered: Option<Retained<AnyObject>> = unsafe {
        msg_send![class!(NSRunningApplication),
        runningApplicationWithProcessIdentifier: std::process::id() as i32]
    };
    // SAFETY: SDK getters on the retained NSRunningApplication.
    let pid: i32 = unsafe { msg_send![&current, processIdentifier] };
    // SAFETY: Same public retained object and zero-argument BOOL getter.
    let finished: bool = unsafe { msg_send![&current, isFinishedLaunching] };
    // SAFETY: Public policy query only; the fixture never changes policy.
    let policy: isize = unsafe { msg_send![app, activationPolicy] };
    json!({"is_running":app.isRunning(),"activation_policy":policy,"current_pid":pid,
        "current_finished_launching":finished,"registered_for_pid":registered.is_some()})
}
fn stop_application(app: &NSApplication) {
    app.stop(None);
    if let Some(event) = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
        NSEventType::ApplicationDefined,NSPoint::new(0.0,0.0),NSEventModifierFlags::empty(),0.0,0,None,0,0,0) {
        app.postEvent_atStart(&event,true);
    }
}
fn public_text(
    object: &AnyObject,
    selector: nsh_platform_macos::objc2::runtime::Sel,
) -> Option<String> {
    // SAFETY: Public NSObject capability query followed only by NSString-valued AX getters.
    let responds: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if !responds {
        return None;
    }
    // SAFETY: Callers supply only NSString-valued public accessibility getters.
    let text: Option<Retained<NSString>> = unsafe { msg_send![object, performSelector: selector] };
    text.map(|text| text.to_string())
        .filter(|text| !text.trim().is_empty())
}
fn retain_old_public_controls(
    window: &NSWindow,
    directory: &std::path::Path,
    run: &str,
    mode: &str,
    generation: u64,
) -> Result<Vec<Retained<AnyObject>>, String> {
    let retained_window: Retained<NSWindow> = Retained::from(window);
    // SAFETY: NSWindow is an Objective-C object. This consumes its typed +1
    // retain without changing ownership; the traversal releases it normally.
    let root: Retained<AnyObject> = unsafe { Retained::cast_unchecked(retained_window) };
    let mut pending = vec![(root, Vec::<usize>::new())];
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    let mut aliases = Vec::new();
    let expected = [
        ("mixed.native", "Native action"),
        ("mixed.builtin", "Built-in action"),
    ];
    let mut matches: [Vec<Retained<AnyObject>>; 2] = [Vec::new(), Vec::new()];
    let mut failure = None;
    while let Some((object, path)) = pending.pop() {
        if seen.len() >= 1024 || path.len() >= 64 {
            failure = Some("public retention traversal exceeded its finite bounds".to_string());
            break;
        }
        // Address identity only detects duplicate public objects, never selects
        // a logical peer or reaches into NSH's private hierarchy.
        let address = Retained::as_ptr(&object) as usize;
        if !seen.insert(address) {
            aliases.push(json!({"public_path":path,"address_diagnostic":address}));
            continue;
        }
        let id = public_text(&object, sel!(accessibilityIdentifier));
        let role = public_text(&object, sel!(accessibilityRole));
        let label = public_text(&object, sel!(accessibilityLabel));
        let title = public_text(&object, sel!(accessibilityTitle));
        let name = label.as_deref().or(title.as_deref());
        let consistent = label
            .as_ref()
            .zip(title.as_ref())
            .is_none_or(|(label, title)| label == title);
        let mut matched = Vec::new();
        if role.as_deref() == Some("AXButton") && consistent {
            for (index, (wanted_id, wanted_name)) in expected.iter().enumerate() {
                if name == Some(*wanted_name) && id.as_deref().is_none_or(|id| id == *wanted_id) {
                    matches[index].push(object.clone());
                    matched.push(*wanted_id);
                }
            }
        }
        let mut child_count = 0;
        // SAFETY: Public NSAccessibility traversal, not NSH/private node lookup.
        let responds: bool =
            unsafe { msg_send![&object, respondsToSelector: sel!(accessibilityChildren)] };
        if responds {
            // SAFETY: The public selector returns current accessibility objects.
            let children: Option<Retained<NSArray<AnyObject>>> =
                unsafe { msg_send![&object, accessibilityChildren] };
            if let Some(children) = children {
                // SAFETY: Normalize existing public AppKit representations,
                // including single-cell controls, without constructing AX nodes.
                let children = unsafe { NSAccessibilityUnignoredChildren(&children) };
                child_count = children.len();
                for (index, child) in children.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(index);
                    pending.push((child, child_path));
                }
            }
        }
        rows.push(json!({"public_path":path,"runtime_type":format!("{:?}",object.class()),
            "address_diagnostic":address,"identifier":id,"role":role,"own_label":label,"own_title":title,
            "matched_peers":matched,"unignored_child_count":child_count,"generation":generation}));
    }
    let counts = [matches[0].len(), matches[1].len()];
    let valid = failure.is_none() && counts == [1, 1];
    atomic_json(
        &directory.join("public-retention.json"),
        &json!({"run":run,"mode":mode,
        "generation":generation,"expected_peers":[expected[0].0,expected[1].0],"match_counts":counts,
        "rows":rows,"duplicate_public_edges":aliases,"failure":failure,"valid":valid,
        "live_action_selection":"external SDK public tree; this walk only corroborates retained lifetime"}),
    );
    if !valid {
        return Err(format!(
            "public lifetime capture requires one unique object for each peer; counts={counts:?}; see public-retention.json"
        ));
    }
    Ok(matches.into_iter().flatten().collect())
}
struct State {
    app: Retained<NSApplication>,
    owner: Option<WindowOwner>,
    window: Option<Retained<NSWindow>>,
    directory: PathBuf,
    run: String,
    mode: String,
    title: String,
    events: Events,
    shared: Rc<RefCell<mixed_tsx::MixedShared>>,
    generation: u64,
    heartbeat: u64,
    start: Instant,
    deadline: Duration,
    launched: bool,
    ready: bool,
    stopped: bool,
    graceful: bool,
    lifecycle: Vec<Value>,
    lifecycle_sequence: u64,
    pending_ready: Option<(String, u64)>,
    retained: Vec<Retained<AnyObject>>,
    fixture_error: Option<Value>,
}
impl State {
    fn record(&mut self, event: &str) {
        self.lifecycle.push(
            json!({"event":event,"generation":self.generation,"system_ticks":system_ticks(),
            "facts":application_facts(&self.app)}),
        );
    }
    fn ledger(&self) -> Value {
        let mut events = self.events.borrow_mut();
        for event in &mut *events {
            if event["run"].is_null() {
                event["run"] = json!(self.run);
                event["mode"] = json!(self.mode);
                event["pid"] = json!(std::process::id());
                event["generation"] = json!(self.generation);
            }
        }
        json!({"run":self.run,"mode":self.mode,"pid":std::process::id(),"generation":self.generation,
            "events":*events,"mixed":self.shared.borrow().diagnostic(),"lifecycle":self.lifecycle,
            "heartbeat":self.heartbeat,"heartbeat_ticks":system_ticks(),"system_ticks":system_ticks()})
    }
    fn poll_lifecycle(&mut self) {
        if !["mixed-reload", "mixed-stop"].contains(&self.mode.as_str()) {
            return;
        }
        let Some(request) = std::fs::read(self.directory.join("lifecycle.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        else {
            return;
        };
        if request["run"] != self.run
            || request["case"] != self.mode
            || request["sequence"].as_u64() != Some(self.lifecycle_sequence + 1)
        {
            return;
        }
        let operation = request["operation"]
            .as_str()
            .expect("declared fixture lifecycle operation");
        assert!(
            (self.mode == "mixed-reload" && self.lifecycle_sequence == 0 && operation == "reload")
                || (self.mode == "mixed-stop"
                    && ((self.lifecycle_sequence == 0 && operation == "stop")
                        || (self.lifecycle_sequence == 1 && operation == "restart"))),
            "fixed lifecycle plan"
        );
        self.lifecycle_sequence += 1;
        self.record("lifecycle_request");
        if operation != "restart" {
            match retain_old_public_controls(
                &self.owner.as_ref().unwrap().window,
                &self.directory,
                &self.run,
                &self.mode,
                self.generation,
            ) {
                Ok(controls) => self.retained = controls,
                Err(message) => {
                    let error = json!({"run":self.run,"mode":self.mode,"pid":std::process::id(),
                        "sequence":self.lifecycle_sequence,"generation":self.generation,"operation":operation,
                        "error":message,"system_ticks":system_ticks()});
                    atomic_json(&self.directory.join("lifecycle-error.json"), &error);
                    self.fixture_error = Some(error);
                    self.record("lifecycle_oracle_failure");
                    return;
                }
            }
        }
        // Bind all old history before changing root generation.
        let _ = self.ledger();
        if operation == "stop" {
            self.owner.as_ref().unwrap().app_view.stop();
            self.shared.borrow_mut().set_phase("stopped");
            self.record("explicit_app_stop_returned");
            atomic_json(
                &self.directory.join("lifecycle-stopped.json"),
                &json!({"run":self.run,"mode":self.mode,
                "sequence":self.lifecycle_sequence,"generation":self.generation,"heartbeat":self.heartbeat,
                "retained_public_controls":self.retained.len(),"ledger":self.ledger(),"system_ticks":system_ticks()}),
            );
        } else {
            self.generation += 1;
            self.shared.borrow_mut().set_phase("replacement");
            self.owner.as_ref().unwrap().load(&self.mode, "replacement");
            self.record("explicit_app_load_returned");
            self.pending_ready = Some(("replacement".into(), self.lifecycle_sequence));
        }
    }
    fn tick(&mut self) -> bool {
        self.heartbeat += 1;
        if self.ready {
            self.poll_lifecycle();
            mixed_tsx::poll_control(&self.shared, &self.directory, &self.events);
        }
        let _ = self.ledger();
        let mounted = self.owner.is_some()
            && self
                .events
                .borrow()
                .iter()
                .filter(|event| event["kind"] == "ready" && event["generation"] == self.generation)
                .count()
                == 1;
        let initial_empty = self.events.borrow().iter().all(|event| {
            matches!(
                event["kind"].as_str(),
                Some("ready" | "native_mount" | "native_prop" | "ref_bound")
            )
        });
        if !self.ready && self.launched && self.app.isRunning() && mounted && initial_empty {
            self.ready = true;
            self.record("subsequent_lifecycle_ready");
            atomic_json(
                &self.directory.join("ready.json"),
                &json!({"run":self.run,"mode":self.mode,
                "pid":std::process::id(),"window_title":self.title,"generation":self.generation,"mounted":mounted,
                "action_events_empty":true,"did_finish_launching":self.launched,"lifecycle_ready":true,
                "is_running":self.app.isRunning(),"application_facts":application_facts(&self.app),
                "heartbeat":self.heartbeat,"case_budget_seconds":self.deadline.as_secs(),"system_ticks":system_ticks()}),
            );
        }
        if mounted && let Some((phase, sequence)) = self.pending_ready.take() {
            self.record("replacement_root_ready");
            atomic_json(
                &self.directory.join("lifecycle-replacement.json"),
                &json!({"run":self.run,"mode":self.mode,
                "sequence":sequence,"generation":self.generation,"phase":phase,"heartbeat":self.heartbeat,
                "retained_public_controls":self.retained.len(),"ledger":self.ledger(),"system_ticks":system_ticks()}),
            );
        }
        atomic_json(&self.directory.join("ledger.json"), &self.ledger());
        let stop = std::fs::read(self.directory.join("stop.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|stop| stop["run"] == self.run && stop["mode"] == self.mode);
        if stop || self.fixture_error.is_some() || self.start.elapsed() >= self.deadline {
            self.graceful = stop;
            self.stopped = true;
            self.record("stop_requested");
            drop(self.owner.take());
            self.retained.clear();
            atomic_json(&self.directory.join("ledger.json"), &self.ledger());
            return true;
        }
        false
    }
}
struct DelegateState {
    state: Rc<RefCell<State>>,
    timer: RefCell<Option<Retained<AnyObject>>>,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateState]
    #[name = "NSHAccessibilityRegressionDelegate"]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    // SAFETY: These SDK-typed delegate callbacks run on the main thread.
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _: &NSNotification) {
            {
                let mut state = self.ivars().state.borrow_mut();
                state.launched = true; state.record("did_finish_launching");
                let owner = make_window(self.mtm(),&state.title,&state.mode,&state.events,&state.shared);
                state.window = Some(owner.window.clone()); state.owner = Some(owner);
            }
            // SAFETY: The delegate owns the timer and invalidates it before release.
            let timer: Retained<AnyObject> = unsafe { msg_send![class!(NSTimer), scheduledTimerWithTimeInterval:0.01_f64,
                target:self, selector:sel!(fixtureTick:), userInfo:None::<&AnyObject>, repeats:true] };
            self.ivars().timer.replace(Some(timer));
        }
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn keep_until_owned_stop(&self, _: &NSApplication) -> bool { false }
    }
    impl Delegate {
        #[unsafe(method(fixtureTick:))]
        fn tick(&self, _: &AnyObject) {
            if self.ivars().state.borrow_mut().tick() {
                if let Some(timer) = self.ivars().timer.borrow_mut().take() {
                    // SAFETY: This delegate owns the retained public NSTimer.
                    unsafe { let _: () = msg_send![&timer,invalidate]; }
                }
                stop_application(&self.ivars().state.borrow().app);
            }
        }
    }
);
pub(super) fn run() {
    log::set_logger(&LOGGER).expect("fixture logger");
    log::set_max_level(log::LevelFilter::Info);
    let mtm = MainThreadMarker::new().expect("AppKit fixture main thread");
    let app = NSApplication::sharedApplication(mtm);
    let directory = PathBuf::from(
        std::env::var_os("NSH_EXTERNAL_AX_DIR")
            .expect("external SDK client supplies owned IPC directory"),
    );
    let run = std::env::var("NSH_EXTERNAL_AX_NONCE").expect("fresh run nonce");
    let mode = std::env::var("NSH_EXTERNAL_AX_MODE").expect("fixed external case");
    let seconds = match mode.as_str() {
        "mixed-baseline" => 160,
        "mixed-merge" | "mixed-hidden" => 220,
        "mixed-reactive" => 440,
        "mixed-reload" => 240,
        "mixed-stop" => 220,
        _ => panic!("unsupported regression case"),
    };
    let (finished, watchdog) = mpsc::channel();
    std::thread::spawn(move || {
        if watchdog
            .recv_timeout(Duration::from_secs(seconds + 10))
            .is_err()
        {
            eprintln!("accessibility fixture hard deadline");
            std::process::exit(124);
        }
    });
    let shared = Rc::new(RefCell::new(mixed_tsx::MixedShared::new(&run, &mode)));
    let title = format!("External AX {mode} {run}");
    let state = Rc::new(RefCell::new(State {
        app: app.clone(),
        owner: None,
        window: None,
        directory,
        run,
        mode,
        title,
        events: Rc::new(RefCell::new(Vec::new())),
        shared,
        generation: 1,
        heartbeat: 0,
        start: Instant::now(),
        deadline: Duration::from_secs(seconds),
        launched: false,
        ready: false,
        stopped: false,
        graceful: false,
        lifecycle: Vec::new(),
        lifecycle_sequence: 0,
        pending_ready: None,
        retained: Vec::new(),
        fixture_error: None,
    }));
    state.borrow_mut().record("before_run");
    let delegate = Delegate::alloc(mtm).set_ivars(DelegateState {
        state: state.clone(),
        timer: RefCell::new(None),
    });
    // SAFETY: Initialize the retained NSObject subclass after its ivars are set.
    let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
    if let Some(timer) = delegate.ivars().timer.borrow_mut().take() {
        // SAFETY: Invalidate the sole owned timer before releasing the delegate.
        unsafe {
            let _: () = msg_send![&timer, invalidate];
        }
    }
    app.setDelegate(None);
    let mut state = state.borrow_mut();
    drop(state.owner.take());
    state.retained.clear();
    state.record("run_returned");
    let visible = state
        .window
        .as_ref()
        .is_some_and(|window| window.isVisible());
    let detached = state
        .window
        .as_ref()
        .is_some_and(|window| window.contentView().is_none());
    atomic_json(&state.directory.join("ledger.json"), &state.ledger());
    atomic_json(
        &state.directory.join("final.json"),
        &json!({"run":state.run,"mode":state.mode,"pid":std::process::id(),
        "graceful":state.graceful,"window_visible":visible,"content_detached":detached,"did_finish_launching":state.launched,
        "lifecycle_ready":state.ready,"stop_called":state.stopped,"is_running_after_run":app.isRunning(),
        "heartbeat":state.heartbeat,"fixture_error":state.fixture_error,"retained_controls_after_cleanup":state.retained.len(),"ledger":state.ledger(),"system_ticks":system_ticks()}),
    );
    let code =
        if state.fixture_error.is_none() && state.graceful && state.ready && !visible && detached {
            0
        } else {
            1
        };
    let _ = finished.send(());
    std::process::exit(code);
}
