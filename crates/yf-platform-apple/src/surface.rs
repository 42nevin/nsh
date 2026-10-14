use std::{
    cell::{Cell, RefCell},
    io,
    sync::Arc,
};

use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, rc::Retained,
};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_foundation::NSObject;
use objc2_ui_kit::{UIResponder, UIView};
use yf_core::{MountBatch, Size};
use yf_engine::{Bundle, Engine, EngineConfig, MountSink};

use crate::{
    image::AssetBase,
    mount::{Mounter, SharedMounter},
    text::CoreTextEngine,
};

type ErrorHandler = Box<dyn Fn(&str)>;
type SharedErrorHandler = Arc<MainThreadBound<RefCell<Option<ErrorHandler>>>>;

/// Forwards mount batches and errors from the engine thread to the main
/// thread.
struct MainThreadSink {
    mounter: SharedMounter,
    on_error: SharedErrorHandler,
}

impl MountSink for MainThreadSink {
    fn submit(&self, batch: MountBatch) {
        let mounter = self.mounter.clone();
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(mounter) = mounter.get(mtm).borrow_mut().as_mut() {
                mounter.apply(batch);
            }
        });
    }

    fn report_error(&self, message: &str) {
        let on_error = self.on_error.clone();
        let message = message.to_owned();
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            if let Some(handler) = on_error.get(mtm).borrow().as_ref() {
                handler(&message);
            }
        });
    }
}

struct Running {
    // Field order: the engine (and its thread) stops before the mounter goes.
    engine: Engine,
    mounter: SharedMounter,
}

pub(crate) struct RootViewIvars {
    /// Bundle waiting for the first non-empty layout.
    pending: RefCell<Option<Bundle>>,
    running: RefCell<Option<Running>>,
    size: Cell<CGSize>,
    options: SurfaceOptions,
    on_error: SharedErrorHandler,
}

define_class!(
    /// The surface root view (node id 1). Starts the engine once it knows
    /// its real size, then reports size changes.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RootViewIvars]
    pub(crate) struct RootView;

    impl RootView {
        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), layoutSubviews] };
            let size = self.bounds().size;
            if size.width <= 0.0 || size.height <= 0.0 || size == self.ivars().size.get() {
                return;
            }
            self.ivars().size.set(size);
            let viewport = Size::new(size.width as f32, size.height as f32);
            if let Some(running) = self.ivars().running.borrow().as_ref() {
                running.engine.control().resize(viewport);
                return;
            }
            let pending = self.ivars().pending.borrow_mut().take();
            if let Some(bundle) = pending {
                self.start(bundle, viewport);
            }
        }
    }
);

impl RootView {
    fn new(
        mtm: MainThreadMarker,
        frame: CGRect,
        bundle: Bundle,
        options: SurfaceOptions,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RootViewIvars {
            pending: RefCell::new(Some(bundle)),
            running: RefCell::new(None),
            size: Cell::new(CGSize::new(0.0, 0.0)),
            options,
            on_error: Arc::new(MainThreadBound::new(RefCell::new(None), mtm)),
        });
        // SAFETY: UIView's designated initializer.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn start(&self, bundle: Bundle, viewport: Size) {
        let mtm = self.mtm();
        let mounter: SharedMounter = Arc::new(MainThreadBound::new(RefCell::new(None), mtm));
        let config = EngineConfig {
            bundle,
            viewport,
            trace: self.ivars().options.trace,
        };
        let sink = MainThreadSink {
            mounter: mounter.clone(),
            on_error: self.ivars().on_error.clone(),
        };
        let engine = match Engine::start(config, CoreTextEngine::default, sink) {
            Ok(engine) => engine,
            Err(err) => return log::error!("failed to start the engine: {err}"),
        };
        // Batches are queued on the main queue, so none can run before the
        // mounter is installed here.
        let root = Retained::into_super(self.retain());
        let asset_base = self.ivars().options.asset_base.clone();
        *mounter.get(mtm).borrow_mut() = Some(Mounter::new(
            mtm,
            root,
            engine.control(),
            mounter.clone(),
            asset_base,
        ));
        *self.ivars().running.borrow_mut() = Some(Running { engine, mounter });
    }

    /// Stops the engine and removes all native views.
    fn stop(&self) {
        let running = self.ivars().running.borrow_mut().take();
        if let Some(Running { engine, mounter }) = running {
            drop(engine);
            // Queued batches become no-ops.
            mounter.get(self.mtm()).borrow_mut().take();
        }
        for view in self.subviews() {
            view.removeFromSuperview();
        }
    }
}

/// Options of a [`Surface`].
#[derive(Debug, Clone, Default)]
pub struct SurfaceOptions {
    /// Log per-commit timings.
    pub trace: bool,
    /// Where `asset:/` image URIs (bundled assets) are loaded from.
    pub asset_base: Option<AssetBase>,
}

/// One running yf app hosted in a `UIView`.
///
/// Add [`Surface::view`] to any view hierarchy; the app starts once the view
/// has a non-empty size and lays out to its bounds. Dropping the surface
/// stops its engine.
pub struct Surface {
    view: Retained<RootView>,
}

impl Surface {
    /// Creates a surface for `bundle`. `frame` is the initial view frame.
    pub fn new(
        mtm: MainThreadMarker,
        bundle: Bundle,
        frame: CGRect,
        options: SurfaceOptions,
    ) -> io::Result<Surface> {
        // Font and text-layout caches are process-wide: warm them while UIKit
        // finishes setting up the window.
        std::thread::Builder::new()
            .name("yf-prewarm".into())
            .spawn(crate::text::prewarm)?;
        Ok(Surface {
            view: RootView::new(mtm, frame, bundle, options),
        })
    }

    /// The root view to add to a hierarchy.
    pub fn view(&self) -> &UIView {
        &self.view
    }

    /// Receives fatal app errors (bundle failed to load, uncaught JS
    /// exceptions) on the main thread. Errors are logged either way.
    pub fn set_error_handler(&self, handler: impl Fn(&str) + 'static) {
        *self.view.ivars().on_error.get(self.view.mtm()).borrow_mut() = Some(Box::new(handler));
    }

    /// Replaces the running app with `bundle` (state is reset).
    pub fn reload(&self, bundle: Bundle) {
        self.view.stop();
        let size = self.view.ivars().size.get();
        if size.width > 0.0 && size.height > 0.0 {
            self.view
                .start(bundle, Size::new(size.width as f32, size.height as f32));
        } else {
            *self.view.ivars().pending.borrow_mut() = Some(bundle);
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.view.stop();
    }
}
