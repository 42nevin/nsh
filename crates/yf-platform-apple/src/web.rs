//! `<webview>`: a `WKWebView`. WebKit is loaded on first use (`dlopen`)
//! rather than linked, so apps without web views do not pay for it at
//! launch.

use std::{ffi::CStr, sync::OnceLock};

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_ui_kit::{UIView, UIViewAutoresizing};
use yf_core::{EventKind, EventPayload, NodeId, PayloadValue, WebProps, WebSource};
use yf_engine::EngineControl;

use crate::native_view::YFView;

/// Lets pages talk to the app, like React Native's web view.
const BRIDGE: &str = "window.ReactNativeWebView = window.ReactNativeWebView || { postMessage: function (data) { window.webkit.messageHandlers.yf.postMessage(String(data)); } };";

fn webkit() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| {
        let path = c"/System/Library/Frameworks/WebKit.framework/WebKit";
        // SAFETY: loads a system framework; the handle is never closed.
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY) };
        if handle.is_null() {
            log::error!("cannot load WebKit");
        }
        !handle.is_null()
    })
}

fn class(name: &CStr) -> &'static AnyClass {
    AnyClass::get(name).expect("WebKit class")
}

pub(crate) struct WebIvars {
    id: NodeId,
    control: EngineControl,
}

define_class!(
    /// Navigation delegate and script message handler of one web view.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = WebIvars]
    pub(crate) struct WebHandler;

    unsafe impl NSObjectProtocol for WebHandler {}

    impl WebHandler {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive_message(&self, _controller: &AnyObject, message: &AnyObject) {
            // SAFETY: `body` of a WKScriptMessage.
            let body: Option<Retained<AnyObject>> = unsafe { msg_send![message, body] };
            let data = body.map(|b| describe(&b)).unwrap_or_default();
            self.send(EventKind::Message, EventPayload::Map(vec![("data", PayloadValue::Str(data))]));
        }

        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn did_start(&self, web: &AnyObject, _navigation: Option<&AnyObject>) {
            let state = navigation_state(web, true);
            self.send(EventKind::LoadStart, state.clone());
            self.send(EventKind::NavigationStateChange, state);
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, web: &AnyObject, _navigation: Option<&AnyObject>) {
            let state = navigation_state(web, false);
            self.send(EventKind::Load, state.clone());
            self.send(EventKind::NavigationStateChange, state);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(&self, _web: &AnyObject, _navigation: Option<&AnyObject>, error: &AnyObject) {
            self.failed(error);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(&self, _web: &AnyObject, _navigation: Option<&AnyObject>, error: &AnyObject) {
            self.failed(error);
        }
    }
);

impl WebHandler {
    fn new(mtm: MainThreadMarker, id: NodeId, control: EngineControl) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WebIvars { id, control });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn send(&self, event: EventKind, payload: EventPayload) {
        let ivars = self.ivars();
        ivars.control.dispatch_event(ivars.id, event, payload);
    }

    fn failed(&self, error: &AnyObject) {
        // SAFETY: `localizedDescription` of an NSError.
        let message: Retained<NSString> = unsafe { msg_send![error, localizedDescription] };
        self.send(
            EventKind::Error,
            EventPayload::Map(vec![("message", PayloadValue::Str(message.to_string()))]),
        );
    }
}

/// `{ url, title, loading, canGoBack, canGoForward }` of a WKWebView.
fn navigation_state(web: &AnyObject, loading: bool) -> EventPayload {
    // SAFETY: WKWebView getters.
    let (url, title, back, forward) = unsafe {
        let url: Option<Retained<NSURL>> = msg_send![web, URL];
        let title: Option<Retained<NSString>> = msg_send![web, title];
        let back: Bool = msg_send![web, canGoBack];
        let forward: Bool = msg_send![web, canGoForward];
        (url, title, back.as_bool(), forward.as_bool())
    };
    let url = url
        .and_then(|u| u.absoluteString())
        .map(|s| s.to_string())
        .unwrap_or_default();
    EventPayload::Map(vec![
        ("url", PayloadValue::Str(url)),
        (
            "title",
            PayloadValue::Str(title.map(|t| t.to_string()).unwrap_or_default()),
        ),
        ("loading", PayloadValue::Bool(loading)),
        ("canGoBack", PayloadValue::Bool(back)),
        ("canGoForward", PayloadValue::Bool(forward)),
    ])
}

fn describe(object: &AnyObject) -> String {
    // SAFETY: every NSObject has a description.
    let d: Retained<NSString> = unsafe { msg_send![object, description] };
    d.to_string()
}

/// A JS string literal for `s`.
fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A web view: a `YFView` placed by the core, holding the WKWebView.
pub(crate) struct WebHost {
    pub(crate) view: Retained<YFView>,
    web: Option<Retained<AnyObject>>,
    handler: Retained<WebHandler>,
    props: WebProps,
}

impl WebHost {
    pub(crate) fn new(mtm: MainThreadMarker, id: NodeId, control: EngineControl) -> Self {
        let view = YFView::new(mtm);
        let handler = WebHandler::new(mtm, id, control);
        let web = webkit().then(|| {
            // SAFETY: WebKit's documented API, sent dynamically; the handler
            // is unregistered when the host is dropped.
            unsafe {
                let config: Retained<AnyObject> = msg_send![class(c"WKWebViewConfiguration"), new];
                let content: Retained<AnyObject> = msg_send![&config, userContentController];
                let _: () = msg_send![&content, addScriptMessageHandler: &*handler, name: &*NSString::from_str("yf")];
                let alloc: Allocated<AnyObject> = msg_send![class(c"WKWebView"), alloc];
                let web: Retained<AnyObject> = msg_send![alloc, initWithFrame: CGRect::new(CGPoint::ZERO, CGSize::ZERO), configuration: &*config];
                let _: () = msg_send![&web, setNavigationDelegate: &*handler];
                let ui: &UIView = &*(Retained::as_ptr(&web) as *const UIView);
                ui.setAutoresizingMask(UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight);
                ui.setFrame(view.bounds());
                view.addSubview(ui);
                web
            }
        });
        Self {
            view,
            web,
            handler,
            props: WebProps {
                // Forces the first `configure` to load.
                source: WebSource::Uri("".into()),
                ..WebProps::default()
            },
        }
    }

    pub(crate) fn configure(&mut self, p: WebProps) {
        let Some(web) = &self.web else { return };
        // SAFETY: WebKit's documented API.
        unsafe {
            let scripts_changed = p.injected_js != self.props.injected_js
                || p.injected_js_before != self.props.injected_js_before
                || self.props.source == WebSource::Uri("".into());
            if scripts_changed {
                let config: Retained<AnyObject> = msg_send![web, configuration];
                let content: Retained<AnyObject> = msg_send![&config, userContentController];
                let _: () = msg_send![&content, removeAllUserScripts];
                let add = |source: &str, at_end: bool| {
                    let alloc: Allocated<AnyObject> = msg_send![class(c"WKUserScript"), alloc];
                    let script: Retained<AnyObject> = msg_send![
                        alloc,
                        initWithSource: &*NSString::from_str(source),
                        injectionTime: at_end as isize,
                        forMainFrameOnly: Bool::YES
                    ];
                    let _: () = msg_send![&content, addUserScript: &*script];
                };
                add(BRIDGE, false);
                if let Some(js) = &p.injected_js_before {
                    add(js, false);
                }
                if let Some(js) = &p.injected_js {
                    add(js, true);
                }
            }
            let ua = p.user_agent.as_deref().map(NSString::from_str);
            let _: () = msg_send![web, setCustomUserAgent: ua.as_deref()];
            let scroll: Retained<AnyObject> = msg_send![web, scrollView];
            let _: () = msg_send![&scroll, setScrollEnabled: Bool::new(p.scroll_enabled)];
            let _: () = msg_send![web, setAllowsBackForwardNavigationGestures: Bool::new(p.back_forward_gestures)];
            let config: Retained<AnyObject> = msg_send![web, configuration];
            let prefs: Retained<AnyObject> = msg_send![&config, defaultWebpagePreferences];
            let _: () =
                msg_send![&prefs, setAllowsContentJavaScript: Bool::new(p.javascript_enabled)];
            if p.source != self.props.source || scripts_changed {
                match &p.source {
                    WebSource::Uri(uri) => {
                        if let Some(url) = NSURL::URLWithString(&NSString::from_str(uri)) {
                            let request: Retained<AnyObject> =
                                msg_send![class(c"NSURLRequest"), requestWithURL: &*url];
                            let _: Option<Retained<AnyObject>> =
                                msg_send![web, loadRequest: &*request];
                        }
                    }
                    WebSource::Html { html, base_url } => {
                        let base = base_url
                            .as_deref()
                            .and_then(|b| NSURL::URLWithString(&NSString::from_str(b)));
                        let _: Option<Retained<AnyObject>> = msg_send![web, loadHTMLString: &*NSString::from_str(html), baseURL: base.as_deref()];
                    }
                    WebSource::None => {}
                }
            }
        }
        self.props = p;
    }

    /// `reload()`, `goBack()`, `goForward()`, `stopLoading()`,
    /// `injectJavaScript()` and `postMessage()`.
    pub(crate) fn command(&self, command: &yf_core::Command) {
        let Some(web) = &self.web else { return };
        let script = match command {
            yf_core::Command::InjectJavaScript(js) => Some(js.clone()),
            yf_core::Command::PostMessage(data) => Some(format!(
                "window.dispatchEvent(new MessageEvent('message', {{ data: {} }}));",
                js_string(data)
            )),
            _ => None,
        };
        // SAFETY: WebKit's documented API.
        unsafe {
            match (command, script) {
                (_, Some(js)) => {
                    let _: () = msg_send![web, evaluateJavaScript: &*NSString::from_str(&js), completionHandler: std::ptr::null::<AnyObject>()];
                }
                (yf_core::Command::Reload, _) => {
                    let _: Option<Retained<AnyObject>> = msg_send![web, reload];
                }
                (yf_core::Command::GoBack, _) => {
                    let _: Option<Retained<AnyObject>> = msg_send![web, goBack];
                }
                (yf_core::Command::GoForward, _) => {
                    let _: Option<Retained<AnyObject>> = msg_send![web, goForward];
                }
                (yf_core::Command::StopLoading, _) => {
                    let _: () = msg_send![web, stopLoading];
                }
                _ => {}
            }
        }
    }
}

impl Drop for WebHost {
    fn drop(&mut self) {
        let Some(web) = &self.web else { return };
        // SAFETY: breaks the cycle web view -> controller -> handler.
        unsafe {
            let _: () = msg_send![web, setNavigationDelegate: std::ptr::null::<AnyObject>()];
            let config: Retained<AnyObject> = msg_send![web, configuration];
            let content: Retained<AnyObject> = msg_send![&config, userContentController];
            let _: () =
                msg_send![&content, removeScriptMessageHandlerForName: &*NSString::from_str("yf")];
        }
        let _ = &self.handler;
    }
}
