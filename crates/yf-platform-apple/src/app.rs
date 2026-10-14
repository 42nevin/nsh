use std::{cell::RefCell, sync::Mutex};

use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class, rc::Retained};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use objc2_ui_kit::{
    UIApplication, UIApplicationDelegate, UIResponder, UIScene, UISceneConfiguration,
    UISceneConnectionOptions, UISceneDelegate, UISceneSession, UIView, UIViewAutoresizing,
    UIViewController, UIWindow, UIWindowScene, UIWindowSceneDelegate,
    UIWindowSceneSessionRoleApplication,
};
use yf_engine::Bundle;

use crate::{
    image::AssetBase,
    surface::{Surface, SurfaceOptions},
};

/// Size of the Mac Catalyst window: an iPhone-sized simulator.
const CATALYST_WINDOW: CGSize = CGSize {
    width: 390.0,
    height: 844.0,
};

/// Where a standalone app gets its JS from.
#[derive(Debug, Clone)]
pub enum AppSource {
    /// A bundle shipped with (or passed to) the app.
    Bundle(Bundle),
    /// A dev server (`ws://host:port/ws`); bundles are pushed on every
    /// rebuild.
    #[cfg(feature = "dev")]
    DevServer(String),
}

/// Options for [`run_app`].
#[derive(Debug, Clone, Default)]
pub struct AppOptions {
    /// Log per-commit timings.
    pub trace: bool,
    /// Where bundled image assets are (for [`AppSource::Bundle`]); dev
    /// servers serve them themselves.
    pub asset_base: Option<AssetBase>,
}

/// Launch parameters, handed from [`run_app`] to the scene delegate that
/// UIKit instantiates by class.
static LAUNCH: Mutex<Option<(AppSource, AppOptions)>> = Mutex::new(None);

/// Main-thread state of the standalone app.
pub(crate) struct AppState {
    _window: Retained<UIWindow>,
    /// Hosts the surface and, in development, overlays above it.
    pub(crate) container: Retained<UIView>,
    surface: Option<Surface>,
    options: AppOptions,
}

thread_local! {
    static APP: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

/// Runs `f` with the app state (main thread only).
pub(crate) fn with_app<R>(f: impl FnOnce(&mut AppState) -> R) -> Option<R> {
    APP.with(|app| app.borrow_mut().as_mut().map(f))
}

impl AppState {
    /// Shows `bundle`, replacing the running app if there is one.
    pub(crate) fn load(&mut self, bundle: Bundle) {
        if let Some(surface) = &self.surface {
            return surface.reload(bundle);
        }
        let mtm = self.container.mtm();
        let options = SurfaceOptions {
            trace: self.options.trace,
            asset_base: self.options.asset_base.clone(),
        };
        let surface = match Surface::new(mtm, bundle, self.container.bounds(), options) {
            Ok(surface) => surface,
            Err(err) => return log::error!("failed to start the engine: {err}"),
        };
        surface.view().setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        #[cfg(feature = "dev")]
        surface.set_error_handler(|message| crate::dev::show_error("Uncaught error", message));
        // Below development overlays.
        self.container.insertSubview_atIndex(surface.view(), 0);
        self.surface = Some(surface);
    }
}

define_class!(
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl UIApplicationDelegate for AppDelegate {
        #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
        fn configuration_for_scene(
            &self,
            _application: &UIApplication,
            session: &UISceneSession,
            _options: &UISceneConnectionOptions,
        ) -> Retained<UISceneConfiguration> {
            let config = UISceneConfiguration::initWithName_sessionRole(
                UISceneConfiguration::alloc(self.mtm()),
                None,
                &session.role(),
            );
            // SAFETY: `SceneDelegate` implements `UIWindowSceneDelegate`.
            unsafe { config.setDelegateClass(Some(SceneDelegate::class())) };
            config
        }
    }
);

define_class!(
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    struct SceneDelegate;

    unsafe impl NSObjectProtocol for SceneDelegate {}

    unsafe impl UISceneDelegate for SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(
            &self,
            scene: &UIScene,
            session: &UISceneSession,
            _options: &UISceneConnectionOptions,
        ) {
            // SAFETY: reading an immutable UIKit constant.
            if *session.role() != *unsafe { UIWindowSceneSessionRoleApplication } {
                return;
            }
            let Some(scene) = scene.downcast_ref::<UIWindowScene>() else {
                return;
            };
            connect(self.mtm(), scene);
        }
    }

    unsafe impl UIWindowSceneDelegate for SceneDelegate {}
);

fn connect(mtm: MainThreadMarker, scene: &UIWindowScene) {
    let Some((source, options)) = LAUNCH.lock().expect("launch lock").take() else {
        return log::error!("no app to launch");
    };
    if let Some(restrictions) = scene.sizeRestrictions() {
        // Only Mac Catalyst has resizable windows.
        restrictions.setMinimumSize(CATALYST_WINDOW);
        restrictions.setMaximumSize(CATALYST_WINDOW);
    }
    let window = UIWindow::initWithWindowScene(UIWindow::alloc(mtm), scene);
    let controller = UIViewController::new(mtm);
    let container = controller.view().expect("view controller has a view");
    window.setRootViewController(Some(&controller));
    window.makeKeyAndVisible();
    APP.with(|app| {
        *app.borrow_mut() = Some(AppState {
            _window: window,
            container,
            surface: None,
            options,
        })
    });
    match source {
        AppSource::Bundle(bundle) => {
            with_app(|app| app.load(bundle));
        }
        #[cfg(feature = "dev")]
        AppSource::DevServer(url) => {
            // `ws://host:port/ws` -> assets at `http://host:port/assets/`.
            if let Some(rest) = url.strip_prefix("ws://")
                && let Some((host, _)) = rest.split_once('/')
            {
                with_app(|app| {
                    app.options.asset_base = Some(AssetBase::Http(format!("http://{host}/assets/")))
                });
            }
            crate::dev::connect(mtm, url)
        }
    }
}

/// Runs a standalone app full-window. Never returns.
///
/// Must be called on the main thread, before any other UIKit use.
pub fn run_app(source: AppSource, options: AppOptions) -> ! {
    let mtm = MainThreadMarker::new().expect("run_app must be called on the main thread");
    *LAUNCH.lock().expect("launch lock") = Some((source, options));
    let delegate = NSString::from_str(
        AppDelegate::class()
            .name()
            .to_str()
            .expect("class name is UTF-8"),
    );
    UIApplication::main(None, Some(&delegate), mtm)
}
