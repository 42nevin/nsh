//! C ABI for embedding nsh apps in existing iOS apps.
//!
//! Built as a static library (`libnsh.a`) and shipped with `include/nsh.h`
//! and a module map, so Swift can `import NativeShell`. Every function must be
//! called on the main thread.
//!
//! On non-UIKit targets this crate is empty.

#![cfg(target_os = "ios")]

use std::{
    ffi::{CStr, c_char, c_void},
    path::Path,
};

use nsh_engine::Bundle;
use nsh_platform_apple::{AppView, AssetBase, SurfaceOptions};
use objc2::MainThreadMarker;
use objc2_core_foundation::CGRect;
use objc2_ui_kit::UIView;

/// An embedded nsh app (opaque to C).
pub struct NshApp {
    view: AppView,
}

fn main_thread() -> MainThreadMarker {
    MainThreadMarker::new().expect("nsh functions must be called on the main thread")
}

fn init_logging() {
    #[cfg(feature = "dev")]
    nsh_platform_apple::dev::init_logging();
    #[cfg(not(feature = "dev"))]
    {
        struct Stderr;
        impl log::Log for Stderr {
            fn enabled(&self, m: &log::Metadata<'_>) -> bool {
                m.level() <= log::Level::Warn || m.target() == "js"
            }
            fn log(&self, r: &log::Record<'_>) {
                if self.enabled(r.metadata()) {
                    eprintln!("[nsh] {} {}: {}", r.level(), r.target(), r.args());
                }
            }
            fn flush(&self) {}
        }
        static LOGGER: Stderr = Stderr;
        // Keep the host app's logger if it installed one.
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    }
}

/// Creates an app view with `frame` (it then follows its superview's
/// size). `trace` logs per-commit timings. Free with [`nsh_app_destroy`].
#[unsafe(no_mangle)]
pub extern "C" fn nsh_app_create(frame: CGRect, trace: bool) -> *mut NshApp {
    init_logging();
    let options = SurfaceOptions {
        trace,
        asset_base: None,
    };
    let view = AppView::new(main_thread(), frame, options);
    Box::into_raw(Box::new(NshApp { view }))
}

/// The view to add to a hierarchy. Owned by `app`.
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`] and not be destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_view(app: *mut NshApp) -> *mut UIView {
    // SAFETY: guaranteed by the caller.
    let app = unsafe { &*app };
    std::ptr::from_ref(app.view.view()).cast_mut()
}

/// Runs a bundle written by `nsh build` (`bundle.qbc`, or `bundle.js`):
/// assets are read from the `assets` directory next to it. Replaces the
/// running app. Returns false (and logs why) if the file cannot be read.
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`]; `path` must be a NUL-terminated
/// UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_load_bundle(app: *mut NshApp, path: *const c_char) -> bool {
    // SAFETY: guaranteed by the caller.
    let (app, path) = unsafe { (&*app, CStr::from_ptr(path)) };
    let Ok(path) = path.to_str() else {
        log::error!("bundle path is not UTF-8");
        return false;
    };
    let path = Path::new(path);
    match Bundle::read(path) {
        Ok(bundle) => {
            app.view.set_asset_base(
                path.parent()
                    .map(|dir| AssetBase::Directory(dir.join("assets"))),
            );
            app.view.load(bundle);
            true
        }
        Err(err) => {
            log::error!("cannot read bundle {}: {err}", path.display());
            false
        }
    }
}

/// Development: loads from a `nsh dev` server (`ws://host:port/ws`) and
/// reloads on every change, showing errors over the app. Returns false if
/// the library was built without development support.
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`]; `url` must be a NUL-terminated
/// UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_connect_dev_server(app: *mut NshApp, url: *const c_char) -> bool {
    // SAFETY: guaranteed by the caller.
    let (app, url) = unsafe { (&*app, CStr::from_ptr(url)) };
    #[cfg(feature = "dev")]
    {
        let Ok(url) = url.to_str() else {
            return false;
        };
        app.view.connect_dev_server(url.to_owned());
        true
    }
    #[cfg(not(feature = "dev"))]
    {
        let _ = (app, url);
        log::error!("this build of nsh has no development support");
        false
    }
}

/// Receives fatal app errors (load failures, uncaught JS exceptions) on the
/// main thread. `context` is passed back unchanged.
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`]; `handler` must be safe to call
/// with `context` until `app` is destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_set_error_handler(
    app: *mut NshApp,
    handler: Option<unsafe extern "C" fn(context: *mut c_void, message: *const c_char)>,
    context: *mut c_void,
) {
    // SAFETY: guaranteed by the caller.
    let app = unsafe { &*app };
    let Some(handler) = handler else { return };
    app.view.set_error_handler(move |message| {
        let message = std::ffi::CString::new(message.replace('\0', " ")).unwrap_or_default();
        // SAFETY: guaranteed by the caller of `nsh_app_set_error_handler`.
        unsafe { handler(context, message.as_ptr()) };
    });
}

/// The status bar style the app asked for, for a host view controller's
/// `preferredStatusBarStyle`.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_status_bar_style() -> objc2_ui_kit::UIStatusBarStyle {
    main_thread();
    nsh_platform_apple::preferred_status_bar_style()
}

/// Whether the app hid the status bar, for `prefersStatusBarHidden`.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_status_bar_hidden() -> bool {
    main_thread();
    nsh_platform_apple::prefers_status_bar_hidden()
}

/// How status bar changes animate, for `preferredStatusBarUpdateAnimation`.
#[unsafe(no_mangle)]
pub extern "C" fn nsh_status_bar_animation() -> objc2_ui_kit::UIStatusBarAnimation {
    main_thread();
    nsh_platform_apple::status_bar_animation()
}

/// Stops the app, removes its view and frees `app`.
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`] and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_destroy(app: *mut NshApp) {
    if app.is_null() {
        return;
    }
    // SAFETY: guaranteed by the caller.
    let app = unsafe { Box::from_raw(app) };
    app.view.view().removeFromSuperview();
}
