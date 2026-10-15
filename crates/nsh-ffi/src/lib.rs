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
    path::{Path, PathBuf},
};

use nsh_engine::{
    Bundle,
    package::{PublicKey, Revocations, TrustStore},
};
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
        ..SurfaceOptions::default()
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

/// Runs a bundle written by `nsh build`: assets are read from the `assets`
/// directory next to it. Replaces the running app. Bytecode (`bundle.qbc`)
/// only loads from inside the app's own bundle; elsewhere (downloads,
/// documents) only JS source (`bundle.js`) does — signed bytecode comes in
/// packages ([`nsh_app_load_package`]). Returns false (and logs why) if the
/// file cannot be read or is refused.
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
    let read = if nsh_platform_apple::in_app_bundle(path) {
        // SAFETY: a resource of the (system-verified, read-only) app bundle.
        unsafe { Bundle::read_trusted(path) }
    } else {
        Bundle::read(path)
    };
    match read {
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

/// The keys, revocations and accepted builds that signed packages are
/// verified against (opaque to C).
pub struct NshTrust {
    store: TrustStore,
    /// Where packages' assets are extracted.
    packages: PathBuf,
}

/// Creates a trust store kept in `state_dir` (created if needed): the
/// newest revocation list and the highest build accepted per app persist
/// there, and packages' assets are extracted under it. Trusts no key until
/// [`nsh_trust_add_root`]. Returns `NULL` (and logs why) if its state cannot
/// be read. Free with [`nsh_trust_destroy`].
///
/// # Safety
///
/// `state_dir` must be a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_trust_create(state_dir: *const c_char) -> *mut NshTrust {
    // SAFETY: guaranteed by the caller.
    let Ok(dir) = unsafe { CStr::from_ptr(state_dir) }.to_str() else {
        log::error!("trust state directory is not UTF-8");
        return std::ptr::null_mut();
    };
    let dir = PathBuf::from(dir);
    match TrustStore::open(&dir) {
        Ok(store) => Box::into_raw(Box::new(NshTrust {
            store,
            packages: dir.join("packages"),
        })),
        Err(err) => {
            log::error!("cannot open the trust store in {}: {err}", dir.display());
            std::ptr::null_mut()
        }
    }
}

/// Trusts a root public key (base64, as `nsh keygen` writes it in
/// `<name>.pub`): packages whose release key it certified load. Returns
/// false for an invalid key.
///
/// # Safety
///
/// `trust` must come from [`nsh_trust_create`]; `public_key` must be a
/// NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_trust_add_root(
    trust: *mut NshTrust,
    public_key: *const c_char,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let (trust, key) = unsafe { (&mut *trust, CStr::from_ptr(public_key)) };
    match key
        .to_str()
        .map_err(|e| e.to_string())
        .and_then(|k| PublicKey::from_base64(k).map_err(|e| e.to_string()))
    {
        Ok(key) => {
            trust.store.add_root(key);
            true
        }
        Err(err) => {
            log::error!("invalid root key: {err}");
            false
        }
    }
}

/// Applies a revocation list (the JSON `nsh revoke` writes) signed by a
/// trusted root and newer than the one the store has. Returns false (and
/// logs why) if it is invalid, untrusted or not newer.
///
/// # Safety
///
/// `trust` must come from [`nsh_trust_create`]; `json` must be a
/// NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_trust_update_revocations(
    trust: *mut NshTrust,
    json: *const c_char,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let (trust, json) = unsafe { (&mut *trust, CStr::from_ptr(json)) };
    let result = json
        .to_str()
        .map_err(|e| e.to_string())
        .and_then(|json| Revocations::from_json(json).map_err(|e| e.to_string()))
        .and_then(|list| {
            trust
                .store
                .update_revocations(list)
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(_) => true,
        Err(err) => {
            log::error!("revocation list refused: {err}");
            false
        }
    }
}

/// Frees `trust`.
///
/// # Safety
///
/// `trust` must come from [`nsh_trust_create`] and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_trust_destroy(trust: *mut NshTrust) {
    if !trust.is_null() {
        // SAFETY: guaranteed by the caller.
        drop(unsafe { Box::from_raw(trust) });
    }
}

/// Verifies the signed package at `path` (`.nshpkg`, written by
/// `nsh publish`) with `trust` and runs it, replacing the running app: its
/// release key must be certified by a trusted root for the app's id, not
/// revoked, and its build not older than one accepted before. Its bytecode
/// is used when it was compiled for this runtime. Returns false if it is
/// refused (the reason goes to the log and the error handler; the running
/// app stays).
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`], `trust` from
/// [`nsh_trust_create`]; `path` must be a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_load_package(
    app: *mut NshApp,
    trust: *mut NshTrust,
    path: *const c_char,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let (app, trust, path) = unsafe { (&*app, &mut *trust, CStr::from_ptr(path)) };
    let Ok(path) = path.to_str() else {
        log::error!("package path is not UTF-8");
        return false;
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            log::error!("cannot read package {path}: {err}");
            return false;
        }
    };
    app.view
        .load_package(&mut trust.store, &bytes, &trust.packages)
        .is_ok()
}

/// What the apps `app` loads are granted at most: JSON in the form of
/// `nsh.json`'s `capabilities` (`NULL`: what each app declares). Applies from
/// the next load. Returns false (and changes nothing) for an invalid grant.
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`]; `json` must be `NULL` or a
/// NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_set_granted_capabilities(
    app: *mut NshApp,
    json: *const c_char,
) -> bool {
    // SAFETY: guaranteed by the caller.
    let app = unsafe { &*app };
    let granted = if json.is_null() {
        None
    } else {
        // SAFETY: guaranteed by the caller.
        let Ok(json) = unsafe { CStr::from_ptr(json) }.to_str() else {
            log::error!("grant is not UTF-8");
            return false;
        };
        match nsh_engine::parse_capabilities(json) {
            Ok(caps) => Some(caps),
            Err(err) => {
                log::error!("invalid grant: {err}");
                return false;
            }
        }
    };
    app.view.set_granted_capabilities(granted);
    true
}

/// The VM limits of the apps `app` loads next: heap (MB), stack (KB), the
/// time budgets (ms) of loading and of every other call into JS, and the
/// node quota; 0 keeps a default.
///
/// # Safety
///
/// `app` must come from [`nsh_app_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nsh_app_set_limits(
    app: *mut NshApp,
    memory_mb: u32,
    stack_kb: u32,
    load_ms: u32,
    call_ms: u32,
    max_nodes: u32,
) {
    // SAFETY: guaranteed by the caller.
    let app = unsafe { &*app };
    let mut limits = nsh_engine::Limits::default();
    if memory_mb > 0 {
        limits.memory_bytes = (memory_mb as usize) << 20;
    }
    if stack_kb > 0 {
        limits.stack_bytes = ((stack_kb as usize) << 10).min(limits.stack_bytes);
    }
    if load_ms > 0 {
        limits.load_budget = std::time::Duration::from_millis(load_ms.into());
    }
    if call_ms > 0 {
        limits.call_budget = std::time::Duration::from_millis(call_ms.into());
    }
    if max_nodes > 0 {
        limits.max_nodes = max_nodes as usize;
    }
    app.view.set_limits(limits);
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
