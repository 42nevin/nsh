//! A terminated web content process reaches the app.
//!
//! WebKit asks its navigation delegate, `webViewWebContentProcessDidTerminate:`;
//! the web view's handler must report it through the app's error listener
//! with the stable termination code, and not reload the page.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use nsh_core::{EventKind, EventPayload, PayloadValue, element::codes, prelude::*};
use nsh_platform_macos as platform;
use platform::{
    objc2::{
        MainThreadMarker, MainThreadOnly, msg_send,
        rc::Retained,
        runtime::{AnyObject, Bool},
    },
    objc2_app_kit::{NSApplication, NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask},
    objc2_foundation::{NSDate, NSPoint, NSRect, NSSize, NSString},
};

// The app's listeners run on the engine thread, not the AppKit thread that
// drives this check: the shared cell must cross threads.
static ERRORS: Mutex<Vec<EventPayload>> = Mutex::new(Vec::new());

#[component]
fn TerminatingWeb() -> impl IntoElement {
    view().flex_1().child(
        web_view_html("<html><body>termination fixture</body></html>", None)
            .on(EventKind::Error, move |payload: &EventPayload| {
                ERRORS.lock().unwrap().push(payload.clone())
            }),
    )
}

fn pump(ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < deadline {
        platform::objc2_foundation::NSRunLoop::currentRunLoop()
            .runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.005));
    }
}

/// The `WKWebView` under `view`, if it is made and attached.
fn web_view_in(view: &NSView) -> Option<Retained<AnyObject>> {
    // SAFETY: WebKit's documented class and AppKit's view hierarchy; the
    // retained view outlives the message sent to it.
    unsafe {
        if let Some(class) = platform::objc2::runtime::AnyClass::get(c"WKWebView") {
            let is_web: Bool = msg_send![view, isKindOfClass: class];
            if is_web.as_bool() {
                return Retained::retain(view as *const NSView as *mut AnyObject);
            }
        }
        view.subviews().iter().find_map(|child| web_view_in(&child))
    }
}

// `AppOptions` is non-exhaustive: its fields are set after `default()`.
#[allow(clippy::field_reassign_with_default)]
pub(super) fn run() {
    ERRORS.lock().unwrap().clear();
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: The window is created on the process main thread.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(760.0, 300.0), NSSize::new(480.0, 300.0)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: This scope retains the window until after it closes.
    unsafe {
        window.setReleasedWhenClosed(false);
    }
    window.setTitle(&NSString::from_str("NSH web content termination"));
    let mut options = platform::AppOptions::default();
    options.size = (480, 300);
    let surface = platform::AppView::new(mtm, options);
    window.setContentView(Some(&surface.view()));
    surface.load(platform::App::Rust(platform::Root::of::<TerminatingWeb>()));
    window.makeKeyAndOrderFront(None);
    app.activate();

    let deadline = Instant::now() + Duration::from_secs(10);
    let web = loop {
        if let Some(web) = web_view_in(&surface.view()) {
            break web;
        }
        assert!(Instant::now() < deadline, "the web view must mount");
        pump(10);
    };
    // Loading the fixture needs no network: any error here is a failure of
    // the test's environment, and would hide the termination's own event.
    assert!(
        ERRORS.lock().unwrap().is_empty(),
        "loading the termination fixture must not report an error"
    );

    // SAFETY: WKWebView's navigation delegate (the handler under test) and
    // WKNavigationDelegate's documented termination method.
    let delegate: Option<Retained<AnyObject>> = unsafe { msg_send![&*web, navigationDelegate] };
    let delegate = delegate.expect("the web view must have its navigation delegate");
    let handles: Bool = unsafe {
        msg_send![
            &*delegate,
            respondsToSelector: platform::objc2::sel!(webViewWebContentProcessDidTerminate:)
        ]
    };
    assert!(
        handles.as_bool(),
        "the web view's navigation delegate must handle webViewWebContentProcessDidTerminate:"
    );
    unsafe {
        let _: () = msg_send![&*delegate, webViewWebContentProcessDidTerminate: &*web];
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while ERRORS.lock().unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "a terminated web content process must report an error"
        );
        pump(10);
    }
    let errors = ERRORS.lock().unwrap().clone();
    let [error] = &errors[..] else {
        panic!("a termination reports one error: {errors:?}");
    };
    let EventPayload::Map(fields) = error else {
        panic!("a termination reports message and code: {error:?}");
    };
    let field = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, value)| value)
    };
    assert!(
        matches!(field("message"), Some(PayloadValue::Str(message)) if !message.trim().is_empty()),
        "a termination reports a message: {fields:?}"
    );
    assert_eq!(
        field("code"),
        Some(&PayloadValue::Str(codes::WEBVIEW_TERMINATED.into())),
        "a termination reports the termination code: {fields:?}"
    );
    println!("web termination: code {}", codes::WEBVIEW_TERMINATED);

    surface.stop();
    window.close();
}
