use std::cell::RefCell;

use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_core_foundation::{CGAffineTransform, CGFloat, CGRect};
use objc2_core_graphics::CGContext;
use objc2_foundation::NSObject;
use objc2_ui_kit::{UIGraphicsGetCurrentContext, UIResponder, UIView};
use yf_core::{Rect, TextLayout};

use crate::text::CoreTextLayout;

#[derive(Default)]
pub(crate) struct TextViewIvars {
    layout: RefCell<Option<(TextLayout, Rect)>>,
}

define_class!(
    /// Draws a `CTFrame` laid out on the engine thread. No text layout
    /// happens on the main thread.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TextViewIvars]
    pub(crate) struct TextView;

    impl TextView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: CGRect) {
            let layout = self.ivars().layout.borrow();
            let Some((layout, content)) = layout.as_ref() else { return };
            let Some(frame) = layout.downcast::<CoreTextLayout>() else { return };
            let Some(ctx) = UIGraphicsGetCurrentContext() else { return };
            let c = Some(&*ctx);
            CGContext::save_g_state(c);
            // CoreText draws in a y-up space with the origin at the bottom of
            // the frame's path.
            CGContext::set_text_matrix(c, CGAffineTransform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 });
            CGContext::translate_ctm(c, content.x as CGFloat, (content.y + content.height) as CGFloat);
            CGContext::scale_ctm(c, 1.0, -1.0);
            // SAFETY: a valid frame and context.
            unsafe { frame.frame.draw(&ctx) };
            CGContext::restore_g_state(c);
        }
    }
);

impl TextView {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TextViewIvars::default());
        // SAFETY: NSObject's designated initializer (UIView's `init` calls
        // `initWithFrame:` with a zero frame).
        let view: Retained<Self> = unsafe { msg_send![super(this), init] };
        view.setOpaque(false);
        view
    }

    pub(crate) fn set_layout(&self, layout: TextLayout, content: Rect) {
        *self.ivars().layout.borrow_mut() = Some((layout, content));
        self.setNeedsDisplay();
    }
}
