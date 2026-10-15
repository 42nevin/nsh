use std::{cell::RefCell, ptr::NonNull, sync::Arc};

use block2::RcBlock;
use nsh_core::{AttributedText, Rect, TextLayout, TextLink};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, rc::Retained,
};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFIndex, CFNumber, CFRange, CFString, CFType, CGAffineTransform,
    CGFloat, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{CGColor, CGContext};
use objc2_core_text::{CTFont, CTRun, kCTFontAttributeName};
use objc2_foundation::{NSObject, NSString};
use objc2_ui_kit::{
    UIAccessibilityTraitStaticText, UIAccessibilityTraits, UIColor, UIGraphicsGetCurrentContext,
    UILongPressGestureRecognizer, UIPasteboard, UIResponder, UIView,
};

use crate::{
    native_view::NshView,
    text::{CoreTextLayout, LaidLine, decoration_key, strikethrough_key, underline_key},
};

/// Draws the decoration lines CoreText does not: strikethroughs, and
/// underlines with a style, thickness or offset (y-up text space).
fn draw_decorations(ctx: &CGContext, line: &LaidLine) {
    // SAFETY: the line's own run array (of CTRuns) and their attribute
    // dictionaries, whose values have the types set by the text engine.
    unsafe {
        let runs = line.line.glyph_runs();
        for i in 0..runs.count() {
            let run = &*runs.value_at_index(i).cast::<CTRun>();
            let attributes = run.attributes();
            let attributes: &CFDictionary<CFString, CFType> = attributes.cast_unchecked();
            let strike = attributes
                .get(strikethrough_key())
                .and_then(|c| c.downcast::<CGColor>().ok());
            let under = attributes
                .get(underline_key())
                .and_then(|c| c.downcast::<CGColor>().ok());
            if strike.is_none() && under.is_none() {
                continue;
            }
            let Some(font) = attributes
                .get(kCTFontAttributeName)
                .and_then(|f| f.downcast::<CTFont>().ok())
            else {
                continue;
            };
            let (mut style, mut thickness, mut offset) = (0, 0.0, CGFloat::NAN);
            if let Some(d) = attributes
                .get(decoration_key())
                .and_then(|d| d.downcast::<CFArray>().ok())
            {
                let n = |i: CFIndex| {
                    (*d.value_at_index(i).cast::<CFNumber>())
                        .as_cgfloat()
                        .unwrap_or(0.0)
                };
                style = n(0) as i32;
                thickness = n(1);
                offset = n(2);
            }
            if thickness <= 0.0 {
                thickness = font.underline_thickness().max(1.0);
            }
            let range = run.string_range();
            let x = line.origin.x
                + line
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
            if let Some(color) = strike {
                let y = line.origin.y + font.x_height() / 2.0;
                stroke_decoration(ctx, &color, style, x, y, width, thickness);
            }
            if let Some(color) = under {
                // Below the baseline: the font's position, or the offset.
                let below = if offset.is_nan() {
                    -font.underline_position()
                } else {
                    offset + thickness / 2.0
                };
                stroke_decoration(
                    ctx,
                    &color,
                    style,
                    x,
                    line.origin.y - below,
                    width,
                    thickness,
                );
            }
        }
    }
}

/// One decoration line centered on `y` (y-up), `style` as encoded by the
/// text engine: solid, double, dotted, dashed, wavy.
fn stroke_decoration(
    ctx: &CGContext,
    color: &CGColor,
    style: i32,
    x: CGFloat,
    y: CGFloat,
    width: CGFloat,
    thickness: CGFloat,
) {
    let c = Some(ctx);
    CGContext::save_g_state(c);
    CGContext::set_stroke_color_with_color(c, Some(color));
    CGContext::set_line_width(c, thickness);
    let line = |y: CGFloat| {
        CGContext::move_to_point(c, x, y);
        CGContext::add_line_to_point(c, x + width, y);
    };
    match style {
        1 => {
            // Two lines a thickness apart.
            line(y + thickness);
            line(y - thickness);
        }
        2 => {
            let pattern = [0.0, thickness * 2.0];
            // SAFETY: a two-element pattern.
            unsafe { CGContext::set_line_dash(c, 0.0, pattern.as_ptr(), 2) };
            CGContext::set_line_cap(c, objc2_core_graphics::CGLineCap::Round);
            line(y);
        }
        3 => {
            let pattern = [thickness * 3.0, thickness * 2.0];
            // SAFETY: a two-element pattern.
            unsafe { CGContext::set_line_dash(c, 0.0, pattern.as_ptr(), 2) };
            line(y);
        }
        4 => {
            // A sine wave, one period per four thicknesses.
            let amplitude = thickness * 1.5;
            let period = (thickness * 4.0).max(4.0);
            let steps = ((width / period) * 8.0).ceil().max(2.0) as usize;
            CGContext::move_to_point(c, x, y);
            for i in 1..=steps {
                let t = width * i as CGFloat / steps as CGFloat;
                let phase = t / period * std::f64::consts::TAU;
                CGContext::add_line_to_point(c, x + t, y + amplitude * phase.sin());
            }
        }
        _ => line(y),
    }
    CGContext::stroke_path(c);
    CGContext::restore_g_state(c);
}

#[derive(Default)]
pub(crate) struct TextViewIvars {
    layout: RefCell<Option<(TextLayout, Rect)>>,
    /// The content, for accessibility and the block's shadow.
    text: RefCell<Option<Arc<AttributedText>>>,
    /// `selectable`: the long press that offers to copy the text.
    select: RefCell<Option<Retained<UILongPressGestureRecognizer>>>,
    /// The copy menu shows: the text is highlighted.
    selected: std::cell::Cell<bool>,
    /// Observes the menu hiding while it shows.
    menu_observer: RefCell<
        Option<Retained<objc2::runtime::ProtocolObject<dyn objc2_foundation::NSObjectProtocol>>>,
    >,
}

define_class!(
    /// Draws a `CTFrame` laid out on the engine thread. No text layout
    /// happens on the main thread.
    #[unsafe(super(NshView, UIView, UIResponder, NSObject))]
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

        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            self.selectable()
        }

        #[unsafe(method(canPerformAction:withSender:))]
        fn can_perform_action(&self, action: objc2::runtime::Sel, _sender: Option<&objc2::runtime::AnyObject>) -> bool {
            self.selectable() && action == objc2::sel!(copy:)
        }

        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&objc2::runtime::AnyObject>) {
            if let Some(text) = self.ivars().text.borrow().as_ref() {
                // Without the placeholders of attachments.
                let copied = text.text.replace(nsh_core::ATTACHMENT_CHAR, "");
                // SAFETY: a plain setter.
                unsafe { UIPasteboard::generalPasteboard().setString(Some(&NSString::from_str(&copied))) };
            }
            self.set_selected(false);
        }

        #[unsafe(method(resignFirstResponder))]
        fn resign_first_responder(&self) -> bool {
            self.set_selected(false);
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), resignFirstResponder] }
        }

        #[unsafe(method(handleSelect:))]
        fn handle_select(&self, recognizer: &UILongPressGestureRecognizer) {
            if recognizer.state() != objc2_ui_kit::UIGestureRecognizerState::Began || !self.selectable() {
                return;
            }
            self.becomeFirstResponder();
            self.set_selected(true);
            // UIEditMenuInteraction needs iOS 16; the menu controller works
            // everywhere.
            #[allow(deprecated)]
            {
                let menu = objc2_ui_kit::UIMenuController::sharedMenuController(self.mtm());
                menu.showMenuFromView_rect(self, self.bounds());
            }
            // The highlight goes when the menu does.
            let weak = objc2::rc::Weak::from_retained(&self.retain());
            let observer = RcBlock::new(move |_: NonNull<objc2_foundation::NSNotification>| {
                if let Some(view) = weak.load() {
                    view.set_selected(false);
                }
            });
            let center = objc2_foundation::NSNotificationCenter::defaultCenter();
            // SAFETY: the observer removes itself in its first call below
            // via `set_selected(false)` being idempotent; the block only
            // holds a weak reference.
            #[allow(deprecated)]
            let token = unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(objc2_ui_kit::UIMenuControllerDidHideMenuNotification),
                    None,
                    Some(&objc2_foundation::NSOperationQueue::mainQueue()),
                    &observer,
                )
            };
            *self.ivars().menu_observer.borrow_mut() = Some(token);
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: CGRect) {
            let layout = self.ivars().layout.borrow();
            let Some((layout, content)) = layout.as_ref() else { return };
            let Some(text) = layout.downcast::<CoreTextLayout>() else { return };
            let Some(ctx) = UIGraphicsGetCurrentContext() else { return };
            let c = Some(&*ctx);
            if self.ivars().selected.get() {
                // The whole text is what the menu copies.
                let tint = self
                    .tintColor()
                    .unwrap_or_else(UIColor::systemBlueColor)
                    .colorWithAlphaComponent(0.25);
                tint.setFill();
                objc2_ui_kit::UIRectFill(CGRect::new(
                    CGPoint::new(content.x as CGFloat, content.y as CGFloat),
                    CGSize::new(content.width as CGFloat, content.height as CGFloat),
                ));
            }
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
                    draw_decorations(&ctx, line);
                }
            }
            CGContext::restore_g_state(c);
        }
    }
);

impl TextView {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TextViewIvars::default());
        // SAFETY: NshView's `initWithFrame:` initializes its ivars, then UIView.
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
        let selectable = text.selectable;
        *self.ivars().layout.borrow_mut() = Some((layout, content));
        *self.ivars().text.borrow_mut() = Some(text);
        let mut select = self.ivars().select.borrow_mut();
        match (select.is_some(), selectable) {
            (false, true) => {
                // SAFETY: `handleSelect:` is implemented by the view, which
                // the recognizer is attached to.
                let r = unsafe {
                    UILongPressGestureRecognizer::initWithTarget_action(
                        UILongPressGestureRecognizer::alloc(self.mtm()),
                        Some(self),
                        Some(objc2::sel!(handleSelect:)),
                    )
                };
                self.addGestureRecognizer(&r);
                self.setUserInteractionEnabled(true);
                *select = Some(r);
            }
            (true, false) => {
                if let Some(r) = select.take() {
                    self.removeGestureRecognizer(&r);
                }
                drop(select);
                self.set_selected(false);
            }
            _ => {}
        }
        self.setNeedsDisplay();
    }

    fn selectable(&self) -> bool {
        self.ivars()
            .text
            .borrow()
            .as_ref()
            .is_some_and(|t| t.selectable)
    }

    fn set_selected(&self, on: bool) {
        if self.ivars().selected.replace(on) != on {
            self.setNeedsDisplay();
        }
        if !on && let Some(token) = self.ivars().menu_observer.borrow_mut().take() {
            // SAFETY: an observer token from this center.
            unsafe {
                objc2_foundation::NSNotificationCenter::defaultCenter()
                    .removeObserver(objc2::runtime::ProtocolObject::as_ref(&token))
            };
        }
    }

    /// Whether the text has pressable spans.
    pub(crate) fn has_links(&self) -> bool {
        self.ivars()
            .text
            .borrow()
            .as_ref()
            .is_some_and(|t| !t.links.is_empty())
    }

    /// Whether a pressable span listens to long presses.
    pub(crate) fn links_long_press(&self) -> bool {
        self.ivars().text.borrow().as_ref().is_some_and(|t| {
            t.links
                .iter()
                .any(|l| l.events.contains(nsh_core::EventKind::LongPress))
        })
    }

    /// The pressable span under `point` (view coordinates).
    pub(crate) fn link_at(&self, point: CGPoint) -> Option<TextLink> {
        let text = self.ivars().text.borrow();
        let text = text.as_ref().filter(|t| !t.links.is_empty())?;
        let layout = self.ivars().layout.borrow();
        let (layout, content) = layout.as_ref()?;
        let lines = &layout.downcast::<CoreTextLayout>()?.lines;
        // CoreText's y-up space from the bottom of the box.
        let x = point.x - content.x as CGFloat;
        let y = (content.y + content.height) as CGFloat - point.y;
        let mut best: Option<(&LaidLine, CGFloat)> = None;
        for line in lines {
            let (mut ascent, mut descent) = (0.0, 0.0);
            // SAFETY: valid out pointers.
            let width = unsafe {
                line.line
                    .typographic_bounds(&mut ascent, &mut descent, std::ptr::null_mut())
            } as CGFloat;
            let (top, bottom) = (line.origin.y + ascent, line.origin.y - descent);
            if x < line.origin.x - 4.0 || x > line.origin.x + width + 4.0 {
                continue;
            }
            let distance = if y > top {
                y - top
            } else if y < bottom {
                bottom - y
            } else {
                0.0
            };
            // A little slack above and below the glyphs.
            if distance <= 4.0 && best.is_none_or(|(_, d)| distance < d) {
                best = Some((line, distance));
            }
        }
        let (line, _) = best?;
        let lx = x - line.origin.x;
        // SAFETY: a valid line; no secondary offset wanted.
        let index = unsafe {
            let index = line.line.string_index_for_position(CGPoint::new(lx, 0.0));
            // The caret index nearest to the point: the character is the
            // one before it when the point is left of the caret.
            let caret = line
                .line
                .offset_for_string_index(index, std::ptr::null_mut());
            if lx < caret && index > 0 {
                index - 1
            } else {
                index
            }
        };
        text.link_at(index.max(0) as u32).copied()
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
