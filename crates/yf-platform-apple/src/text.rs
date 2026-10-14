//! CoreText text engine: typesets on the engine thread, draws on the main
//! thread from the finished `CTFrame`, so every text is laid out once.

use std::{cell::RefCell, ffi::c_void, ptr::NonNull};

use objc2::rc::Retained;
use objc2_core_foundation::{
    CFAttributedString, CFIndex, CFMutableAttributedString, CFRange, CFRetained, CFString, CGFloat,
    CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{CGColor, CGPath};
use objc2_core_text::{
    CTFont, CTFrame, CTFramesetter, CTParagraphStyle, CTParagraphStyleSetting,
    CTParagraphStyleSpecifier, CTTextAlignment, kCTFontAttributeName,
    kCTForegroundColorAttributeName, kCTParagraphStyleAttributeName,
};
use objc2_ui_kit::{
    UIFont, UIFontWeight, UIFontWeightBlack, UIFontWeightBold, UIFontWeightHeavy,
    UIFontWeightLight, UIFontWeightMedium, UIFontWeightRegular, UIFontWeightSemibold,
    UIFontWeightThin, UIFontWeightUltraLight,
};
use rustc_hash::FxHashMap;
use yf_core::{AttributedText, Color, Size, TextAlign, TextEngine, TextLayout};

/// Caches system fonts by `(size bits, weight)`. Bounded: an app animating
/// `fontSize` would otherwise grow it without limit.
#[derive(Default)]
struct FontCache {
    fonts: RefCell<FxHashMap<(u32, u16), CFRetained<CTFont>>>,
}

impl FontCache {
    const MAX: usize = 64;

    fn get(&self, size: f32, weight: u16) -> CFRetained<CTFont> {
        let mut fonts = self.fonts.borrow_mut();
        if let Some(font) = fonts.get(&(size.to_bits(), weight)) {
            return font.clone();
        }
        if fonts.len() >= Self::MAX {
            fonts.clear();
        }
        let font = system_font(size, weight);
        fonts.insert((size.to_bits(), weight), font.clone());
        font
    }
}

/// The system font as a `CTFont`. `UIFont` is toll-free bridged with
/// `CTFont`, and the UIKit constructor is the supported way to get the
/// system font at an arbitrary weight.
fn system_font(size: f32, weight: u16) -> CFRetained<CTFont> {
    let font: Retained<UIFont> =
        UIFont::systemFontOfSize_weight(size as CGFloat, font_weight(weight));
    let ptr = Retained::into_raw(font).cast::<CTFont>();
    // SAFETY: toll-free bridging; ownership of the +1 reference moves over.
    unsafe { CFRetained::from_raw(NonNull::new(ptr).expect("non-null font")) }
}

fn font_weight(weight: u16) -> UIFontWeight {
    // SAFETY: UIKit's font weight constants are immutable statics.
    unsafe {
        match weight {
            0..=149 => UIFontWeightUltraLight,
            150..=249 => UIFontWeightThin,
            250..=349 => UIFontWeightLight,
            350..=449 => UIFontWeightRegular,
            450..=549 => UIFontWeightMedium,
            550..=649 => UIFontWeightSemibold,
            650..=749 => UIFontWeightBold,
            750..=849 => UIFontWeightHeavy,
            _ => UIFontWeightBlack,
        }
    }
}

pub(crate) fn cg_color(color: Color) -> CFRetained<CGColor> {
    let (r, g, b, a) = color.to_f64();
    CGColor::new_srgb(r, g, b, a)
}

/// Typesetting state of one text content.
pub struct Prepared {
    framesetter: CFRetained<CTFramesetter>,
}

/// A finished `CTFrame`, drawn by [`crate::text_view::TextView`].
pub(crate) struct CoreTextLayout {
    pub(crate) frame: CFRetained<CTFrame>,
}

// SAFETY: a `CTFrame` is immutable once created, and CoreText objects are
// thread-safe; it is created on the engine thread and only read (drawn) on
// the main thread.
unsafe impl Send for CoreTextLayout {}
// SAFETY: see `Send`.
unsafe impl Sync for CoreTextLayout {}

/// [`TextEngine`] backed by CoreText.
#[derive(Default)]
pub struct CoreTextEngine {
    fonts: FontCache,
}

impl CoreTextEngine {
    fn attributed_string(&self, text: &AttributedText) -> CFRetained<CFMutableAttributedString> {
        let string =
            CFMutableAttributedString::new(None, 0).expect("CFAttributedStringCreateMutable");
        let s = Some(&*string);
        let full = CFRange {
            location: 0,
            length: 0,
        };
        // SAFETY: valid mutable string and range.
        unsafe {
            CFMutableAttributedString::replace_string(
                s,
                full,
                Some(&CFString::from_str(&text.text)),
            )
        };
        let length = text.spans.last().map_or(0, |span| span.end as CFIndex);
        CFMutableAttributedString::begin_editing(s);
        // SAFETY: attribute values have the types CoreText requires for their
        // keys (CTParagraphStyle, CTFont, CGColor), and every range lies within
        // the string because span offsets are UTF-16 units, like CFString.
        unsafe {
            let paragraph = paragraph_style(text);
            CFMutableAttributedString::set_attribute(
                s,
                CFRange {
                    location: 0,
                    length,
                },
                Some(kCTParagraphStyleAttributeName),
                Some(&paragraph),
            );
            for span in &text.spans {
                let range = CFRange {
                    location: span.start as CFIndex,
                    length: (span.end - span.start) as CFIndex,
                };
                let font = self.fonts.get(span.style.font_size, span.style.font_weight);
                CFMutableAttributedString::set_attribute(
                    s,
                    range,
                    Some(kCTFontAttributeName),
                    Some(&font),
                );
                let color = cg_color(span.style.color);
                CFMutableAttributedString::set_attribute(
                    s,
                    range,
                    Some(kCTForegroundColorAttributeName),
                    Some(&color),
                );
            }
        }
        CFMutableAttributedString::end_editing(s);
        string
    }
}

fn paragraph_style(text: &AttributedText) -> CFRetained<CTParagraphStyle> {
    let alignment = match text.align {
        TextAlign::Auto => CTTextAlignment::Natural,
        TextAlign::Left => CTTextAlignment::Left,
        TextAlign::Center => CTTextAlignment::Center,
        TextAlign::Right => CTTextAlignment::Right,
        TextAlign::Justify => CTTextAlignment::Justified,
    };
    let line_height = text.spans.first().map_or(0.0, |s| s.style.line_height) as CGFloat;
    let setting = |spec, value: *const c_void, size| CTParagraphStyleSetting {
        spec,
        valueSize: size,
        value: NonNull::new(value.cast_mut()).expect("non-null setting"),
    };
    let mut settings = vec![setting(
        CTParagraphStyleSpecifier::Alignment,
        (&raw const alignment).cast(),
        size_of::<CTTextAlignment>(),
    )];
    if line_height > 0.0 {
        for spec in [
            CTParagraphStyleSpecifier::MinimumLineHeight,
            CTParagraphStyleSpecifier::MaximumLineHeight,
        ] {
            settings.push(setting(
                spec,
                (&raw const line_height).cast(),
                size_of::<CGFloat>(),
            ));
        }
    }
    // SAFETY: every setting points at a live value of the declared size.
    unsafe { CTParagraphStyle::new(settings.as_ptr(), settings.len()) }
}

impl TextEngine for CoreTextEngine {
    type Prepared = Prepared;

    fn prepare(&self, text: &AttributedText) -> Prepared {
        let string = self.attributed_string(text);
        let string: &CFAttributedString = &string;
        // SAFETY: a valid attributed string.
        Prepared {
            framesetter: unsafe { CTFramesetter::with_attributed_string(string) },
        }
    }

    fn measure(&self, prepared: &Prepared, max_width: Option<f32>) -> Size {
        let constraints = CGSize::new(
            max_width.map_or(CGFloat::MAX, |w| w as CGFloat),
            CGFloat::MAX,
        );
        // SAFETY: whole-string range, no frame attributes, no fit range out-param.
        let size = unsafe {
            prepared.framesetter.suggest_frame_size_with_constraints(
                CFRange {
                    location: 0,
                    length: 0,
                },
                None,
                constraints,
                std::ptr::null_mut(),
            )
        };
        Size::new(size.width as f32, size.height as f32)
    }

    fn layout(&self, prepared: &Prepared, size: Size) -> TextLayout {
        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(size.width as CGFloat, size.height as CGFloat),
        );
        // SAFETY: a valid rectangle path and whole-string range.
        let frame = unsafe {
            let path = CGPath::with_rect(rect, std::ptr::null());
            prepared.framesetter.frame(
                CFRange {
                    location: 0,
                    length: 0,
                },
                &path,
                None,
            )
        };
        TextLayout(std::sync::Arc::new(CoreTextLayout { frame }))
    }
}

/// Warms up font and CoreText caches (process-wide) on a background thread
/// so the first real layout does not pay for them.
pub(crate) fn prewarm() {
    let engine = CoreTextEngine::default();
    for (size, weight) in [(14.0, 400), (17.0, 400), (17.0, 700)] {
        let mut text = AttributedText::default();
        let style = yf_core::ResolvedTextStyle {
            font_size: size,
            font_weight: weight,
            ..Default::default()
        };
        text.push("Hg 0123 ab", style);
        let prepared = engine.prepare(&text);
        let measured = engine.measure(&prepared, Some(100.0));
        let _ = engine.layout(&prepared, measured);
    }
}
