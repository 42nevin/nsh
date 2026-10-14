//! Development host support: connects to the `yf` dev server, loads every
//! pushed bundle, forwards logs to the developer's terminal and shows build
//! and runtime errors on screen.

use std::{
    cell::RefCell,
    io::Write,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use dispatch2::DispatchQueue;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
use objc2_core_foundation::{CGFloat, CGSize};
use objc2_foundation::{NSObject, NSString};
use objc2_ui_kit::{
    NSTextAlignment, UIColor, UIDevice, UIEdgeInsets, UIFont, UIFontWeightRegular,
    UIFontWeightSemibold, UILabel, UITapGestureRecognizer, UITextView, UIView, UIViewAutoresizing,
};
use yf_dev::{DevClient, DevEvent, protocol::BundleFormat};
use yf_engine::{BYTECODE_VERSION, Bundle};

use crate::app::with_app;

static CLIENT: OnceLock<DevClient> = OnceLock::new();
/// While connected, logs go to the dev server only (the developer's
/// terminal); otherwise to stderr.
static CONNECTED: AtomicBool = AtomicBool::new(false);

struct DevLogger;

impl log::Log for DevLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target().starts_with("yf")
            || metadata.target() == "js"
            || metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        match CLIENT.get() {
            Some(client) if CONNECTED.load(Ordering::Relaxed) => {
                client.log(record.level().into(), record.target(), &message)
            }
            _ => {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "{:>5} {} {message}",
                    record.level(),
                    record.target()
                );
            }
        }
    }

    fn flush(&self) {}
}

/// Installs a logger that prints to stderr and forwards every line to the
/// dev server once connected. Call first thing in a development host.
pub fn init_logging() {
    static LOGGER: DevLogger = DevLogger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

#[derive(Default)]
struct DevUi {
    status: Option<Retained<UILabel>>,
    overlay: Option<Retained<UIView>>,
    target: Option<Retained<DismissTarget>>,
}

thread_local! {
    static UI: RefCell<DevUi> = RefCell::new(DevUi::default());
}

define_class!(
    /// Dismisses the error overlay on tap.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct DismissTarget;

    impl DismissTarget {
        #[unsafe(method(dismiss:))]
        fn dismiss(&self, _sender: &UITapGestureRecognizer) {
            hide_error();
        }
    }
);

impl DismissTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// Starts the dev client (main thread, after the app state exists).
pub(crate) fn connect(mtm: MainThreadMarker, url: String) {
    let device = device_name(mtm);
    show_status(&format!("Connecting to {url}…"));
    let started = DevClient::start(url, device, BYTECODE_VERSION.to_owned(), |event| {
        match event {
            DevEvent::Connected => CONNECTED.store(true, Ordering::Relaxed),
            DevEvent::Disconnected => CONNECTED.store(false, Ordering::Relaxed),
            _ => {}
        }
        DispatchQueue::main().exec_async(move || handle(event));
    });
    match started {
        Ok(client) => {
            let _ = CLIENT.set(client);
        }
        Err(err) => log::error!("cannot start the dev client: {err}"),
    }
}

fn device_name(mtm: MainThreadMarker) -> String {
    if cfg!(target_abi = "macabi") {
        // UIDevice reports "iPad" under Catalyst.
        "Mac Catalyst".to_owned()
    } else if cfg!(target_abi = "sim") {
        format!("iOS Simulator ({})", UIDevice::currentDevice(mtm).name())
    } else {
        UIDevice::currentDevice(mtm).name().to_string()
    }
}

fn handle(event: DevEvent) {
    match event {
        DevEvent::Connected => hide_status(),
        DevEvent::Disconnected => show_status("Dev server disconnected — reconnecting…"),
        DevEvent::Building => {}
        DevEvent::BuildFailed(message) => show_error("Build failed", &message),
        DevEvent::Bundle {
            version,
            format,
            bytes,
            source_map,
        } => {
            let mut bundle = match format {
                BundleFormat::Bytecode => Bundle::bytecode(bytes),
                BundleFormat::Source => match String::from_utf8(bytes) {
                    Ok(code) => Bundle::source(code),
                    Err(_) => {
                        return show_error(
                            "Invalid bundle",
                            "the dev server sent non-UTF-8 source",
                        );
                    }
                },
            };
            if let Some(map) = source_map {
                bundle = bundle.with_source_map(map);
            }
            log::info!("loading build {version}");
            hide_error();
            hide_status();
            with_app(|app| app.load(bundle));
        }
    }
}

fn container() -> Option<Retained<UIView>> {
    with_app(|app| app.container.clone())
}

fn show_status(text: &str) {
    let Some(container) = container() else {
        return;
    };
    let mtm = container.mtm();
    UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        let label = ui.status.get_or_insert_with(|| {
            let label = UILabel::new(mtm);
            label.setTextAlignment(NSTextAlignment::Center);
            // SAFETY: non-null values for null-resettable properties, and an
            // immutable UIKit font weight constant.
            unsafe {
                label.setTextColor(Some(&UIColor::whiteColor()));
                label.setFont(Some(&UIFont::systemFontOfSize_weight(
                    13.0,
                    UIFontWeightSemibold,
                )));
            }
            label.setBackgroundColor(Some(&UIColor::colorWithRed_green_blue_alpha(
                0.1, 0.1, 0.1, 0.85,
            )));
            label.setTranslatesAutoresizingMaskIntoConstraints(false);
            label
        });
        label.setText(Some(&NSString::from_str(text)));
        if label.superview().is_none() {
            container.addSubview(label);
            // Below the status bar / notch / Catalyst title bar.
            let guide = container.safeAreaLayoutGuide();
            for constraint in [
                label
                    .topAnchor()
                    .constraintEqualToAnchor(&guide.topAnchor()),
                label
                    .leadingAnchor()
                    .constraintEqualToAnchor(&container.leadingAnchor()),
                label
                    .trailingAnchor()
                    .constraintEqualToAnchor(&container.trailingAnchor()),
                label.heightAnchor().constraintEqualToConstant(32.0),
            ] {
                constraint.setActive(true);
            }
        } else {
            container.bringSubviewToFront(label);
        }
    });
}

fn hide_status() {
    UI.with(|ui| {
        if let Some(label) = ui.borrow_mut().status.take() {
            label.removeFromSuperview();
        }
    });
}

/// Shows a full-screen error over the app; tap to dismiss.
pub(crate) fn show_error(title: &str, message: &str) {
    let Some(container) = container() else { return };
    let mtm = container.mtm();
    hide_error();
    let overlay = UIView::initWithFrame(UIView::alloc(mtm), container.bounds());
    overlay.setAutoresizingMask(
        UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
    );
    overlay.setBackgroundColor(Some(&UIColor::colorWithRed_green_blue_alpha(
        0.55, 0.08, 0.08, 0.97,
    )));

    let text = UITextView::new(mtm);
    text.setFrame(overlay.bounds());
    text.setAutoresizingMask(
        UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
    );
    text.setEditable(false);
    text.setBackgroundColor(Some(&UIColor::clearColor()));
    text.setTextColor(Some(&UIColor::whiteColor()));
    // SAFETY: reading an immutable UIKit constant.
    text.setFont(Some(&UIFont::monospacedSystemFontOfSize_weight(
        12.0,
        unsafe { UIFontWeightRegular },
    )));
    text.setTextContainerInset(UIEdgeInsets {
        top: 48.0,
        left: 16.0,
        bottom: 24.0,
        right: 16.0,
    });
    text.setText(Some(&NSString::from_str(&format!(
        "{title}\n\n{message}\n\n(tap to dismiss)"
    ))));
    // Keep code frames intact: no wrapping, scroll sideways instead.
    let container_text = text.textContainer();
    container_text.setWidthTracksTextView(false);
    container_text.setSize(CGSize::new(4000.0, CGFloat::MAX));
    overlay.addSubview(&text);

    UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        let target = ui
            .target
            .get_or_insert_with(|| DismissTarget::new(mtm))
            .clone();
        // SAFETY: `dismiss:` is implemented by `DismissTarget`, kept alive in `UI`.
        let tap = unsafe {
            UITapGestureRecognizer::initWithTarget_action(
                UITapGestureRecognizer::alloc(mtm),
                Some(&target),
                Some(sel!(dismiss:)),
            )
        };
        overlay.addGestureRecognizer(&tap);
        container.addSubview(&overlay);
        ui.overlay = Some(overlay);
    });
}

fn hide_error() {
    UI.with(|ui| {
        if let Some(overlay) = ui.borrow_mut().overlay.take() {
            overlay.removeFromSuperview();
        }
    });
}
