//! The Objective-C object receiving every native callback of a surface:
//! gesture actions, pull-to-refresh, scroll view delegate calls and display
//! link ticks. Views are identified by their `tag` (the node id).

use dispatch2::DispatchQueue;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_quartz_core::CADisplayLink;
use objc2_ui_kit::{
    UIGestureRecognizer, UIGestureRecognizerState, UIPanGestureRecognizer, UIRefreshControl,
    UIScrollView, UIScrollViewDelegate,
};
use yf_core::{EventKind, EventPayload, NodeId};
use yf_engine::EngineControl;

use crate::mount::SharedMounter;

pub(crate) struct TargetIvars {
    control: EngineControl,
    mounter: SharedMounter,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetIvars]
    pub(crate) struct EventTarget;

    unsafe impl NSObjectProtocol for EventTarget {}

    impl EventTarget {
        #[unsafe(method(handleTap:))]
        fn handle_tap(&self, recognizer: &UIGestureRecognizer) {
            if let Some(view) = recognizer.view() {
                self.dispatch(view.tag() as NodeId, EventKind::Press, EventPayload::None);
            }
        }

        #[unsafe(method(handleLongPress:))]
        fn handle_long_press(&self, recognizer: &UIGestureRecognizer) {
            if recognizer.state() == UIGestureRecognizerState::Began
                && let Some(view) = recognizer.view()
            {
                self.dispatch(view.tag() as NodeId, EventKind::LongPress, EventPayload::None);
            }
        }

        #[unsafe(method(handlePan:))]
        fn handle_pan(&self, recognizer: &UIPanGestureRecognizer) {
            let Some(view) = recognizer.view() else { return };
            let superview = view.superview();
            let t = recognizer.translationInView(superview.as_deref());
            let v = recognizer.velocityInView(superview.as_deref());
            let id = view.tag() as NodeId;
            let state = recognizer.state();
            self.with_mounter(move |m| m.pan(id, state, t, v));
        }

        #[unsafe(method(handleRefresh:))]
        fn handle_refresh(&self, control: &UIRefreshControl) {
            if let Some(scroll) = control.superview() {
                self.dispatch(scroll.tag() as NodeId, EventKind::Refresh, EventPayload::None);
            }
        }

        #[unsafe(method(tick:))]
        fn tick(&self, _link: &CADisplayLink) {
            self.with_mounter(|m| m.tick());
        }
    }

    unsafe impl UIScrollViewDelegate for EventTarget {
        #[unsafe(method(scrollViewDidScroll:))]
        fn scroll_view_did_scroll(&self, scroll: &UIScrollView) {
            let id = scroll.tag() as NodeId;
            let offset = scroll.contentOffset();
            // Lists need offsets to pick rows; the engine coalesces them.
            self.ivars().control.scroll(id, offset.x as f32, offset.y as f32);
            self.with_mounter(move |m| m.scrolled(id, offset));
        }

        #[unsafe(method(scrollViewDidEndDecelerating:))]
        fn scroll_view_did_end_decelerating(&self, scroll: &UIScrollView) {
            self.scroll_ended(scroll);
        }

                #[unsafe(method(scrollViewDidEndDragging:willDecelerate:))]
        fn scroll_view_did_end_dragging(&self, scroll: &UIScrollView, decelerate: bool) {
            let (id, offset) = (scroll.tag() as NodeId, scroll.contentOffset());
            self.with_mounter(move |m| m.drag_ended(id, offset));
            if !decelerate {
                self.scroll_ended(scroll);
            }
        }
    }
);

impl EventTarget {
    pub(crate) fn new(
        mtm: MainThreadMarker,
        control: EngineControl,
        mounter: SharedMounter,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars { control, mounter });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn dispatch(&self, id: NodeId, event: EventKind, payload: EventPayload) {
        self.ivars().control.dispatch_event(id, event, payload);
    }

    fn scroll_ended(&self, scroll: &UIScrollView) {
        let offset = scroll.contentOffset();
        self.dispatch(
            scroll.tag() as NodeId,
            EventKind::ScrollEnd,
            EventPayload::Scroll {
                x: offset.x as f32,
                y: offset.y as f32,
            },
        );
    }

    /// Runs `f` on the mounter. UIKit can call back synchronously while the
    /// mounter applies a batch (e.g. setting `contentOffset`); then the call
    /// is deferred to the next main-queue turn instead of re-entering.
    fn with_mounter(&self, f: impl FnOnce(&mut crate::mount::Mounter) + Send + 'static) {
        let mtm = self.mtm();
        let shared = &self.ivars().mounter;
        let busy = match shared.get(mtm).try_borrow_mut() {
            Ok(mut guard) => {
                if let Some(m) = guard.as_mut() {
                    f(m);
                }
                return;
            }
            Err(_) => true,
        };
        if busy {
            let shared = shared.clone();
            DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue");
                if let Some(m) = shared.get(mtm).borrow_mut().as_mut() {
                    f(m);
                }
            });
        }
    }
}
