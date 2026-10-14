use std::{cell::RefCell, sync::Arc};

use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_core_foundation::{
    CFDictionary, CFRange, CFString, CFType, CGAffineTransform, CGFloat, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{CGColor, CGContext};
use objc2_core_text::{CTFont, CTRun, kCTFontAttributeName};
use objc2_foundation::{NSObject, NSString};
use objc2_ui_kit::{
    UIAccessibilityTraitStaticText, UIAccessibilityTraits, UIColor, UIGraphicsGetCurrentContext,
    UIResponder, UIView,
};
use yf_core::{AttributedText, Rect, TextLayout};

use crate::{
    native_view::YFView,
    text::{CoreTextLayout, LaidLine, strikethrough_key},
};

/// Draws the strikethrough runs of `line` (CoreText draws underlines only).
fn draw_strikethrough(ctx: &CGContext, line: &LaidLine) {
    // SAFETY: the line's own run array (of CTRuns) and their attribute
    // dictionaries, whose values have the types set by the text engine.
    unsafe {
        let runs = line.line.glyph_runs();
        for i in 0..runs.count() {
            let run = &*runs.value_at_index(i).cast::<CTRun>();
            let attributes = run.attributes();
            let attributes: &CFDictionary<CFString, CFType> = attributes.cast_unchecked();
            let Some(color) = attributes.get(strikethrough_key()) else {
                continue;
            };
            let color = color
                .downcast_ref::<CGColor>()
                .expect("strikethrough color");
            let Some(font) = attributes
                .get(kCTFontAttributeName)
                .and_then(|f| f.downcast::<CTFont>().ok())
            else {
                continue;
            };
            let range = run.string_range();
            let x = line
                .line
                .offset_for_string_index(range.location, std::ptr::null_mut());
            let width = run.typographic_bounds(
                CFRange {
                    location: 0,
                    length: 0,
                },
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            let thickness = font.underline_thickness().max(1.0);
            let y = line.origin.y + font.x_height() / 2.0 - thickness / 2.0;
            CGContext::set_fill_color_with_color(Some(ctx), Some(color));
            CGContext::fill_rect(
                Some(ctx),
                CGRect::new(
                    CGPoint::new(line.origin.x + x, y),
                    CGSize::new(width, thickness),
                ),
            );
        }
    }
}

#[derive(Default)]
pub(crate) struct TextViewIvars {
    layout: RefCell<Option<(TextLayout, Rect)>>,
    /// The content, for accessibility and the block's shadow.
    text: RefCell<Option<Arc<AttributedText>>>,
}

define_class!(
    /// Draws a `CTFrame` laid out on the engine thread. No text layout
    /// happens on the main thread.
    #[unsafe(super(YFView, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TextViewIvars]
    pub(crate) struct TextView;

    impl TextView {
        #[unsafe(method(isAccessibilityElement))]
        fn is_accessibility_element(&self) -> bool {
            // SAFETY: calling the superclass implementation.
            let explicit: bool = unsafe { msg_send![super(self), isAccessibilityElement] };
            explicit || (!self.hidden_from_accessibility() && self.has_text())
        }

        #[unsafe(method_id(accessibilityLabel))]
        fn accessibility_label(&self) -> Option<Retained<NSString>> {
            // SAFETY: calling the superclass implementation (the explicit label).
            let explicit: Option<Retained<NSString>> = unsafe { msg_send![super(self), accessibilityLabel] };
            explicit.or_else(|| {
                self.ivars().text.borrow().as_ref().map(|t| NSString::from_str(&t.text))
            })
        }

        #[unsafe(method(accessibilityTraits))]
        fn accessibility_traits(&self) -> UIAccessibilityTraits {
            // SAFETY: calling the superclass implementation; an immutable constant.
            let traits: UIAccessibilityTraits = unsafe { msg_send![super(self), accessibilityTraits] };
            traits | unsafe { UIAccessibilityTraitStaticText }
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: CGRect) {
            let layout = self.ivars().layout.borrow();
            let Some((layout, content)) = layout.as_ref() else { return };
            let Some(text) = layout.downcast::<CoreTextLayout>() else { return };
            let Some(ctx) = UIGraphicsGetCurrentContext() else { return };
            let c = Some(&*ctx);
            CGContext::save_g_state(c);
            // CoreText draws in a y-up space with the origin at the bottom of
            // the box.
            CGContext::set_text_matrix(c, CGAffineTransform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 });
            CGContext::translate_ctm(c, content.x as CGFloat, (content.y + content.height) as CGFloat);
            CGContext::scale_ctm(c, 1.0, -1.0);
            if let Some(shadow) = self.ivars().text.borrow().as_ref().and_then(|t| t.shadow) {
                let (r, g, b, a) = shadow.color.to_f64();
                let color = UIColor::colorWithRed_green_blue_alpha(r, g, b, a);
                // The context is flipped: a positive offset must still go down.
                CGContext::set_shadow_with_color(
                    c,
                    CGSize::new(shadow.offset_x as CGFloat, -shadow.offset_y as CGFloat),
                    shadow.radius as CGFloat,
                    // SAFETY: a plain getter.
                    Some(&*unsafe { color.CGColor() }),
                );
            }
            for line in &text.lines {
                CGContext::set_text_position(c, line.origin.x, line.origin.y);
                // SAFETY: a valid line and context.
                unsafe { line.line.draw(&ctx) };
                if text.strikethrough {
                    draw_strikethrough(&ctx, line);
                }
            }
            CGContext::restore_g_state(c);
        }
    }
);

impl TextView {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TextViewIvars::default());
        // SAFETY: YFView's `initWithFrame:` initializes its ivars, then UIView.
        let view: Retained<Self> = unsafe {
            msg_send![super(this), initWithFrame: CGRect::new(CGPoint::ZERO, CGSize::ZERO)]
        };
        view.setOpaque(false);
        // Text can arrive before the view's frame (a list cell laid out
        // before it is placed): draw again when the size changes.
        view.setContentMode(objc2_ui_kit::UIViewContentMode::Redraw);
        view
    }

    pub(crate) fn set_layout(&self, text: Arc<AttributedText>, layout: TextLayout, content: Rect) {
        *self.ivars().layout.borrow_mut() = Some((layout, content));
        *self.ivars().text.borrow_mut() = Some(text);
        self.setNeedsDisplay();
    }

    fn has_text(&self) -> bool {
        self.ivars()
            .text
            .borrow()
            .as_ref()
            .is_some_and(|t| !t.text.is_empty())
    }

    fn hidden_from_accessibility(&self) -> bool {
        // SAFETY: a plain getter.
        unsafe { msg_send![self, accessibilityElementsHidden] }
    }
}
