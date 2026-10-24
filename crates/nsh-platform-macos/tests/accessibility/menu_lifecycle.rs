//! Dependency-only AppKit regression for application-menu callback ownership.
//!
//! Opt in with a fresh `NSH_MENU_LIFECYCLE_DIR`; `NSH_MENU_LIFECYCLE_CASE`
//! selects `replacement` (default), `tracking` or `collection`. No external AX client or
//! product application is used. The existing fixture binary avoids another
//! copy of the bundler and runtime.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

use nsh_platform_macos::{
    AppOptions, AppView, Bundle, ComponentContext, Components, NativeComponent,
    objc2::{
        MainThreadMarker, MainThreadOnly,
        rc::{Retained, autoreleasepool},
    },
    objc2_app_kit::{
        NSApplication, NSBackingStoreType, NSMenu, NSView, NSWindow, NSWindowStyleMask,
    },
    objc2_foundation::{NSArray, NSDate, NSPoint, NSRect, NSRunLoop, NSSize, NSString},
};
use serde_json::{Value, json};

#[derive(Default)]
struct State {
    events: Vec<String>,
    native: Vec<Value>,
    context: Option<ComponentContext>,
}
type Shared = Rc<RefCell<State>>;

struct Probe {
    view: Retained<NSView>,
    shared: Shared,
}

impl NativeComponent for Probe {
    fn view(&self) -> Retained<NSView> {
        self.view.clone()
    }

    fn set_prop(&mut self, _: &str, _: Option<&Value>) {}

    fn command(&mut self, name: &str, _: &Value) {
        self.shared.borrow_mut().events.push(name.to_owned());
        if name == "choose-current" {
            // The current mainMenu is resolved here, before the next queued
            // SetMenu, not captured earlier for the causal regression.
            choose_current(
                &NSApplication::sharedApplication(self.view.mtm()),
                &self.shared,
            );
        }
    }
}

fn count(shared: &Shared, event: &str) -> usize {
    shared
        .borrow()
        .events
        .iter()
        .filter(|e| *e == event)
        .count()
}

fn wait(shared: &Shared, label: &str, done: impl Fn() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        if Instant::now() >= deadline {
            return Err(format!("{label}: {:?}", shared.borrow().events));
        }
        autoreleasepool(|_| {
            NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.01));
        });
    }
    Ok(())
}

fn await_count(shared: &Shared, event: &str, expected: usize) {
    wait(shared, event, || count(shared, event) >= expected).unwrap();
    assert_eq!(count(shared, event), expected, "exact callback count");
}

fn emit(shared: &Shared, event: &str) {
    let context = shared
        .borrow()
        .context
        .clone()
        .expect("live fixture component");
    context.emit(event, Value::Null);
}

fn find(menu: &NSMenu, label: &str) -> Option<(Retained<NSMenu>, isize)> {
    for (index, item) in menu.itemArray().iter().enumerate() {
        if item.title().to_string() == label {
            return Some((Retained::from(menu), index as isize));
        }
        if let Some(child) = item.submenu()
            && let Some(found) = find(&child, label)
        {
            return Some(found);
        }
    }
    None
}

fn current(app: &NSApplication) -> (Retained<NSMenu>, isize) {
    find(&app.mainMenu().expect("installed main menu"), "Choose").expect("fixture menu item")
}

fn choose_current(app: &NSApplication, shared: &Shared) {
    let (menu, index) = current(app);
    menu.update();
    let item = menu.itemAtIndex(index).unwrap();
    shared.borrow_mut().native.push(json!({
        "title": item.title().to_string(), "tag": item.tag(), "enabled": item.isEnabled(),
    }));
    assert!(item.isEnabled(), "the native choice is available");
    menu.performActionForItemAtIndex(index);
}

fn report(directory: &Path, shared: &Shared, stage: &str, error: Option<&str>) {
    let state = shared.borrow();
    let result = json!({
        "stage":stage, "error":error, "pid":std::process::id(),
        "events":state.events, "native_choices":state.native,
    });
    let temp = directory.join("result.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    std::fs::rename(temp, directory.join("result.json")).unwrap();
}

struct Fixture {
    app_view: Rc<AppView>,
    window: Retained<NSWindow>,
    code: String,
}

impl Fixture {
    fn reload(&self) {
        self.app_view.load(Bundle::source(self.code.clone()).into());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.app_view.stop();
        self.window.orderOut(None);
        self.window.setContentView(None);
        self.window.close();
    }
}

fn fixture(mtm: MainThreadMarker, directory: &Path, shared: &Shared) -> Fixture {
    let components = Components::default();
    let recorded = shared.clone();
    components.register(
        "MenuLifecycleProbe",
        Rc::new(move |context| {
            recorded.borrow_mut().context = Some(context);
            Box::new(Probe {
                view: NSView::new(mtm),
                shared: recorded.clone(),
            })
        }),
    );
    let app_view = AppView::new(
        mtm,
        AppOptions {
            components,
            data_dir: Some(directory.join("state")),
            size: (480, 300),
            ..Default::default()
        },
    );
    // SAFETY: A main-thread AppKit window with explicit Rust ownership.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(80.0, 80.0), NSSize::new(480.0, 300.0)),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: The fixture, not AppKit's close callback, owns this retain.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("NSH menu lifetime regression"));
    window.setContentView(Some(&app_view.view()));
    window.orderFront(None);
    let code =
        nsh_bundler::compile_module("MenuLifecycle.tsx", include_str!("MenuLifecycle.tsx"), &[])
            .expect("compile the dependency fixture");
    let fixture = Fixture {
        app_view,
        window,
        code,
    };
    fixture.reload();
    fixture
}

fn replacement(app: &NSApplication, fixture: &Fixture, shared: &Shared, directory: &Path) {
    choose_current(app, shared);
    await_count(shared, "old", 1);
    let (disabled, index) = find(&app.mainMenu().unwrap(), "Disabled").unwrap();
    disabled.update();
    assert!(!disabled.itemAtIndex(index).unwrap().isEnabled());
    disabled.performActionForItemAtIndex(index);
    emit(shared, "barrier");
    await_count(shared, "barrier", 1);
    assert_eq!(count(shared, "forbidden"), 0);
    drop(disabled);

    emit(shared, "replace");
    await_count(shared, "installed", 1);
    choose_current(app, shared);
    await_count(shared, "new-1", 1);
    report(directory, shared, "new-callback-control-passed", None);
    await_count(shared, "old", 2);
    report(directory, shared, "current-native-replacement-passed", None);

    // A separate ownership probe deliberately holds a native submenu, as
    // AppKit can while tracking. This is not the causal current-menu probe.
    let (held, index) = current(app);
    emit(shared, "install");
    await_count(shared, "installed", 2);
    held.performActionForItemAtIndex(index);
    await_count(shared, "new-1", 2);
    drop(held);
    emit(shared, "clear");
    await_count(shared, "cleared", 1);
    await_count(shared, "new-2", 1);
    emit(shared, "install");
    await_count(shared, "installed", 3);
    let before = current(app).0.itemAtIndex(current(app).1).unwrap().tag();
    emit(shared, "invalid");
    await_count(shared, "invalid-rejected", 1);
    let (menu, index) = current(app);
    assert_eq!(menu.itemAtIndex(index).unwrap().tag(), before);
    drop(menu);
    choose_current(app, shared);
    await_count(shared, "new-3", 1);

    fixture.reload();
    await_count(shared, "ready", 2);
    let (retired, index) = current(app);
    let retired_id = retired.itemAtIndex(index).unwrap().tag();
    fixture.reload();
    await_count(shared, "ready", 3);
    let (live, live_index) = current(app);
    assert_eq!(
        live.itemAtIndex(live_index).unwrap().tag(),
        retired_id,
        "the fixture must exercise reused IDs in different engines"
    );
    retired.performActionForItemAtIndex(index);
    emit(shared, "barrier");
    await_count(shared, "barrier", 2);
    assert_eq!(
        count(shared, "old"),
        2,
        "retired menu cannot call the new engine"
    );
    choose_current(app, shared);
    await_count(shared, "old", 3);
    fixture.app_view.stop();
    retired.performActionForItemAtIndex(index);
    live.performActionForItemAtIndex(live_index);
    assert_eq!(count(shared, "old"), 3, "stopped engines reject input");
}

fn tracking(app: &NSApplication, fixture: &Fixture, shared: &Shared) {
    let (menu, index) = current(app);
    let held = menu.clone();
    let state = shared.clone();
    let failure = Rc::new(RefCell::new(None));
    let outcome = failure.clone();
    let block = block2::RcBlock::new(move || {
        emit(&state, "install");
        let result = wait(&state, "replacement while tracking", || {
            count(&state, "installed") == 1
        });
        if result.is_ok() {
            held.performActionForItemAtIndex(index);
        }
        *outcome.borrow_mut() = result.err();
        held.cancelTracking();
    });
    // SAFETY: Foundation's common modes constant, used on the AppKit thread.
    let modes = NSArray::from_slice(&[unsafe {
        nsh_platform_macos::objc2_foundation::NSRunLoopCommonModes
    }]);
    // SAFETY: The block owns its captures and runs on the same main thread.
    unsafe { NSRunLoop::mainRunLoop().performInModes_block(&modes, &block) };
    menu.popUpMenuPositioningItem_atLocation_inView(
        None,
        NSPoint::new(10.0, 10.0),
        Some(&fixture.app_view.view()),
    );
    assert!(failure.borrow().is_none(), "{:?}", failure.borrow());
    await_count(shared, "old", 1);
    choose_current(app, shared);
    await_count(shared, "new-1", 1);
}

fn collection(shared: &Shared) {
    emit(shared, "burst");
    await_count(shared, "burst-cleared", 1);
    emit(shared, "collect");
    await_count(shared, "collected-0", 1);
}

pub(super) fn run(directory: PathBuf) {
    assert!(
        directory.is_absolute(),
        "an explicit owned evidence path is required"
    );
    std::fs::create_dir_all(&directory).unwrap();
    assert!(
        !directory.join("result.json").exists(),
        "never overwrite an earlier run"
    );
    let (done, deadline) = mpsc::channel();
    std::thread::spawn(move || {
        if deadline.recv_timeout(Duration::from_secs(90)).is_err() {
            eprintln!("menu lifecycle fixture exceeded its owned 90-second deadline");
            std::process::exit(124);
        }
    });
    let mtm = MainThreadMarker::new().expect("AppKit main thread");
    let app = NSApplication::sharedApplication(mtm);
    let shared = Rc::new(RefCell::new(State::default()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let fixture = fixture(mtm, &directory, &shared);
        await_count(&shared, "ready", 1);
        match std::env::var("NSH_MENU_LIFECYCLE_CASE")
            .as_deref()
            .unwrap_or("replacement")
        {
            "replacement" => replacement(&app, &fixture, &shared, &directory),
            "tracking" => tracking(&app, &fixture, &shared),
            "collection" => collection(&shared),
            other => panic!("unknown menu lifecycle case {other}"),
        }
        drop(fixture);
    }));
    let error = result.as_ref().err().map(|error| {
        error
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| error.downcast_ref::<&str>().map(|e| (*e).to_owned()))
            .unwrap_or_else(|| "fixture panic".into())
    });
    report(
        &directory,
        &shared,
        if error.is_none() { "passed" } else { "failed" },
        error.as_deref(),
    );
    let _ = done.send(());
    if result.is_err() {
        std::process::exit(1);
    }
    println!(
        "PASS native menu lifecycle: {}",
        directory.join("result.json").display()
    );
}
