//! `TouchTracker`: reports touches to the JS responder system (`onTouch`).
//!
//! One tracker sits on the surface root and on each modal's root. It only
//! observes: it never recognizes in a way that blocks other gestures, and
//! views still get their touches. Touches start being tracked when the view
//! under them, or one around it, listens to `onTouch`.

use std::cell::{Cell, RefCell};

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, rc::Retained,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSSet};
use objc2_ui_kit::{UIEvent, UIGestureRecognizer, UIGestureRecognizerState, UITouch, UIView};
use yf_core::{NodeId, TouchEvent, TouchPhase, TouchPoint};

use crate::target::EventTarget;

/// A finger being tracked.
pub(crate) struct Tracked {
    touch: Retained<UITouch>,
    id: u32,
    target: NodeId,
    /// The target's view, for `locationX/Y`.
    view: Retained<UIView>,
    path: Vec<NodeId>,
}

pub(crate) struct TrackerIvars {
    target: Retained<EventTarget>,
    tracked: RefCell<Vec<Tracked>>,
}

thread_local! {
    /// Finger identifiers, unique across trackers.
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
}

define_class!(
    #[unsafe(super(UIGestureRecognizer, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TrackerIvars]
    pub(crate) struct TouchTracker;

    unsafe impl NSObjectProtocol for TouchTracker {}

    impl TouchTracker {
        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &NSSet<UITouch>, _event: &UIEvent) {
            let mut began = Vec::new();
            for touch in touches.iter() {
                let Some(view) = touch.view() else { continue };
                let point = touch.locationInView(Some(&view));
                let Some((target, target_view, path)) =
                    self.ivars().target.touch_target(&view, point)
                else {
                    continue;
                };
                let id = NEXT_ID.get();
                NEXT_ID.set(id.wrapping_add(1));
                began.push(Tracked {
                    touch: touch.retain(),
                    id,
                    target,
                    view: target_view,
                    path,
                });
            }
            if began.is_empty() {
                return;
            }
            let changed: Vec<u32> = began.iter().map(|t| t.id).collect();
            self.ivars().tracked.borrow_mut().extend(began);
            self.report(TouchPhase::Start, &changed);
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, touches: &NSSet<UITouch>, _event: &UIEvent) {
            let changed = self.ids_of(touches);
            if !changed.is_empty() {
                self.report(TouchPhase::Move, &changed);
            }
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, touches: &NSSet<UITouch>, _event: &UIEvent) {
            self.lift(touches, TouchPhase::End);
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, touches: &NSSet<UITouch>, _event: &UIEvent) {
            self.lift(touches, TouchPhase::Cancel);
        }

        #[unsafe(method(reset))]
        fn reset(&self) {
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), reset] };
            self.cancel_all();
        }

        #[unsafe(method(canBePreventedByGestureRecognizer:))]
        fn can_be_prevented(&self, _other: &UIGestureRecognizer) -> bool {
            false
        }

        #[unsafe(method(canPreventGestureRecognizer:))]
        fn can_prevent(&self, _other: &UIGestureRecognizer) -> bool {
            false
        }
    }
);

impl TouchTracker {
    pub(crate) fn new(mtm: MainThreadMarker, target: &Retained<EventTarget>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TrackerIvars {
            target: target.clone(),
            tracked: RefCell::new(Vec::new()),
        });
        // SAFETY: the designated initializer, without an action.
        let this: Retained<Self> = unsafe {
            msg_send![super(this), initWithTarget: Option::<&NSObject>::None, action: Option::<objc2::runtime::Sel>::None]
        };
        this.setCancelsTouchesInView(false);
        this.setDelaysTouchesBegan(false);
        this.setDelaysTouchesEnded(false);
        this
    }

    /// A native gesture (a scroll view) took the touches over: JS gets a
    /// cancel and hears no more of them.
    pub(crate) fn cancel_all(&self) {
        if self.ivars().tracked.borrow().is_empty() {
            return;
        }
        let changed: Vec<u32> = self.ivars().tracked.borrow().iter().map(|t| t.id).collect();
        self.report(TouchPhase::Cancel, &changed);
        self.ivars().tracked.borrow_mut().clear();
    }

    fn ids_of(&self, touches: &NSSet<UITouch>) -> Vec<u32> {
        let tracked = self.ivars().tracked.borrow();
        touches
            .iter()
            .filter_map(|touch| {
                tracked
                    .iter()
                    .find(|t| std::ptr::eq::<UITouch>(&*t.touch, &*touch))
                    .map(|t| t.id)
            })
            .collect()
    }

    fn lift(&self, touches: &NSSet<UITouch>, phase: TouchPhase) {
        let changed = self.ids_of(touches);
        if !changed.is_empty() {
            self.report(phase, &changed);
            self.ivars()
                .tracked
                .borrow_mut()
                .retain(|t| !changed.contains(&t.id));
        }
        if self.ivars().tracked.borrow().is_empty() {
            // Done with this sequence (a possible recognizer would wait).
            // SAFETY: `setState:` is the subclass API of UIGestureRecognizer.
            let _: () = unsafe { msg_send![self, setState: UIGestureRecognizerState::Failed] };
        }
    }

    /// Sends the tracked fingers, `changed` marking those the event is
    /// about (with their paths when they `start`).
    fn report(&self, phase: TouchPhase, changed: &[u32]) {
        let Some(root) = self.view() else { return };
        let tracked = self.ivars().tracked.borrow();
        let mut timestamp = 0.0;
        let points = tracked
            .iter()
            .map(|t| {
                let page = t.touch.locationInView(Some(&root));
                let local = t.touch.locationInView(Some(&t.view));
                timestamp = t.touch.timestamp() * 1000.0;
                let is_changed = changed.contains(&t.id);
                TouchPoint {
                    id: t.id,
                    target: t.target,
                    path: if phase == TouchPhase::Start && is_changed {
                        t.path.clone()
                    } else {
                        Vec::new()
                    },
                    page_x: page.x as f32,
                    page_y: page.y as f32,
                    location_x: local.x as f32,
                    location_y: local.y as f32,
                    changed: is_changed,
                }
            })
            .collect();
        drop(tracked);
        self.ivars().target.touches(Box::new(TouchEvent {
            phase,
            timestamp,
            points,
        }));
    }
}
