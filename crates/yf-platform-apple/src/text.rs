//! CoreText text engine: typesets on the engine thread, draws on the main
//! thread from finished `CTLine`s, so every text is laid out once.

use std::{cell::RefCell, ffi::c_void, ptr::NonNull};

use objc2::rc::Retained;
use objc2_core_foundation::{
    CFArray, CFAttributedString, CFDictionary, CFIndex, CFMutableAttributedString, CFNumber,
    CFRange, CFRetained, CFString, CFType, CFURL, CGFloat, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{CGColor, CGPath};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontManagerRegisterFontsForURL, CTFontManagerScope,
    CTFontSymbolicTraits, CTFrame, CTFramesetter, CTLine, CTLineTruncationType, CTParagraphStyle,
    CTParagraphStyleSetting, CTParagraphStyleSpecifier, CTTextAlignment, CTTypesetter,
    CTWritingDirection, kCTFontAttributeName, kCTFontFamilyNameAttribute,
    kCTFontFeatureSettingsAttribute, kCTFontOpenTypeFeatureTag, kCTFontOpenTypeFeatureValue,
    kCTFontTraitsAttribute, kCTFontWeightTrait, kCTForegroundColorAttributeName,
    kCTKernAttributeName, kCTParagraphStyleAttributeName, kCTUnderlineColorAttributeName,
    kCTUnderlineStyleAttributeName,
};
use objc2_ui_kit::{
    UIFont, UIFontWeight, UIFontWeightBlack, UIFontWeightBold, UIFontWeightHeavy,
    UIFontWeightLight, UIFontWeightMedium, UIFontWeightRegular, UIFontWeightSemibold,
    UIFontWeightThin, UIFontWeightUltraLight,
};
use rustc_hash::FxHashMap;
use yf_core::{
    AttributedText, Color, EllipsizeMode, FontFamily, ResolvedTextStyle, Size, TextAlign,
    TextEngine, TextLayout,
};

/// Attribute carrying the strikethrough color: CoreText draws underlines
/// but not strikethroughs, so [`crate::text_view::TextView`] draws those.
pub(crate) fn strikethrough_key() -> &'static CFString {
    static KEY: std::sync::OnceLock<CFStringKey> = std::sync::OnceLock::new();
    &KEY.get_or_init(|| CFStringKey(CFString::from_static_str("YFStrikethroughColor")))
        .0
}

/// Attribute carrying the color of an underline CoreText cannot draw
/// (styled, thick or offset): [`crate::text_view::TextView`] draws it.
pub(crate) fn underline_key() -> &'static CFString {
    static KEY: std::sync::OnceLock<CFStringKey> = std::sync::OnceLock::new();
    &KEY.get_or_init(|| CFStringKey(CFString::from_static_str("YFUnderlineColor")))
        .0
}

/// Attribute carrying how drawn decoration lines look: a `CFArray` of
/// [style, thickness (0: the font's), offset (NaN: the font's)].
pub(crate) fn decoration_key() -> &'static CFString {
    static KEY: std::sync::OnceLock<CFStringKey> = std::sync::OnceLock::new();
    &KEY.get_or_init(|| CFStringKey(CFString::from_static_str("YFDecorationStyle")))
        .0
}

struct CFStringKey(CFRetained<CFString>);
// SAFETY: CFStrings are immutable and thread-safe.
unsafe impl Send for CFStringKey {}
// SAFETY: as above.
unsafe impl Sync for CFStringKey {}

/// Identifies a font: size bits, weight, family, italic, OpenType features.
type FontKey = (u32, u16, Option<FontFamily>, bool, u8);

/// Caches fonts. Bounded: an app animating `fontSize` would otherwise grow
/// it without limit.
#[derive(Default)]
struct FontCache {
    fonts: RefCell<FxHashMap<FontKey, CFRetained<CTFont>>>,
}

impl FontCache {
    const MAX: usize = 64;

    fn get(&self, style: &ResolvedTextStyle) -> CFRetained<CTFont> {
        let key = (
            style.font_size.to_bits(),
            style.font_weight,
            style.font_family,
            style.italic,
            style.font_variant.0,
        );
        let mut fonts = self.fonts.borrow_mut();
        if let Some(font) = fonts.get(&key) {
            return font.clone();
        }
        if fonts.len() >= Self::MAX {
            fonts.clear();
        }
        let font = make_font(style);
        fonts.insert(key, font.clone());
        font
    }

    fn clear(&self) {
        self.fonts.borrow_mut().clear();
    }
}

fn make_font(style: &ResolvedTextStyle) -> CFRetained<CTFont> {
    let size = style.font_size as CGFloat;
    let font = style
        .font_family
        .and_then(|family| {
            let name = family.name();
            generic_font(&name, size, style.font_weight)
                .unwrap_or_else(|| family_font(&name, size, style.font_weight))
        })
        .unwrap_or_else(|| system_font(size, style.font_weight));
    let font = if style.italic {
        let italic = CTFontSymbolicTraits::TraitItalic;
        // SAFETY: a valid font; size 0 keeps the size, null matrix keeps it.
        unsafe { font.copy_with_symbolic_traits(0.0, std::ptr::null(), italic, italic) }
            // No italic face: keep the upright one, as web browsers do (they
            // synthesize an oblique; CoreText cannot).
            .unwrap_or(font)
    } else {
        font
    };
    with_features(font, style.font_variant)
}

/// `font` with the OpenType features of `variant` (`fontVariant`).
fn with_features(font: CFRetained<CTFont>, variant: yf_core::FontVariant) -> CFRetained<CTFont> {
    if variant.0 == 0 {
        return font;
    }
    let one = CFNumber::new_i32(1);
    let features: Vec<CFRetained<CFDictionary<CFString, CFType>>> = variant
        .tags()
        .map(|tag| {
            let tag = CFString::from_str(tag);
            // SAFETY: immutable CoreText constants.
            let keys = unsafe { [kCTFontOpenTypeFeatureTag, kCTFontOpenTypeFeatureValue] };
            CFDictionary::from_slices(&keys, &[tag.as_ref(), one.as_ref()])
        })
        .collect();
    let features = CFArray::from_retained_objects(&features);
    // SAFETY: an immutable CoreText constant.
    let key = unsafe { kCTFontFeatureSettingsAttribute };
    let attributes = CFDictionary::<CFString, CFType>::from_slices(&[key], &[features.as_ref()]);
    // SAFETY: attribute values have the types CoreText documents; size 0
    // and a null matrix keep the font's.
    unsafe {
        let descriptor = CTFontDescriptor::with_attributes(attributes.as_opaque());
        font.copy_with_attributes(0.0, std::ptr::null(), Some(&descriptor))
    }
}

/// The font of `style` as a `UIFont` (toll-free bridged with `CTFont`), for
/// UIKit text views such as inputs.
pub(crate) fn ui_font(style: &ResolvedTextStyle) -> Retained<UIFont> {
    let font = make_font(style);
    let ptr = CFRetained::into_raw(font).as_ptr().cast::<UIFont>();
    // SAFETY: toll-free bridging; ownership of the +1 reference moves over.
    unsafe { Retained::from_raw(ptr).expect("non-null font") }
}

/// CSS generic families (`font-mono`, `font-serif`): the system font's
/// designs. `Some(None)` is the plain system font; `None` is not generic.
fn generic_font(family: &str, size: CGFloat, weight: u16) -> Option<Option<CFRetained<CTFont>>> {
    use objc2_ui_kit::{
        UIFontDescriptorSystemDesign, UIFontDescriptorSystemDesignRounded,
        UIFontDescriptorSystemDesignSerif,
    };
    let design: &UIFontDescriptorSystemDesign = match family {
        "system-ui" | "sans-serif" | "ui-sans-serif" | "-apple-system" => return Some(None),
        "monospace" | "ui-monospace" => {
            let font = UIFont::monospacedSystemFontOfSize_weight(size, font_weight(weight));
            return Some(Some(bridge(font)));
        }
        // SAFETY: immutable UIKit constants.
        "serif" | "ui-serif" => unsafe { UIFontDescriptorSystemDesignSerif },
        "ui-rounded" => unsafe { UIFontDescriptorSystemDesignRounded },
        _ => return None,
    };
    let base = UIFont::systemFontOfSize_weight(size, font_weight(weight));
    // SAFETY: a plain getter of a valid font.
    let descriptor = unsafe { base.fontDescriptor() };
    let font = descriptor
        .fontDescriptorWithDesign(design)
        .map(|d| UIFont::fontWithDescriptor_size(&d, size))
        .unwrap_or(base);
    Some(Some(bridge(font)))
}

/// A `UIFont` as the `CTFont` it is (toll-free bridged).
fn bridge(font: Retained<UIFont>) -> CFRetained<CTFont> {
    let ptr = Retained::into_raw(font).cast::<CTFont>();
    // SAFETY: toll-free bridging; ownership of the +1 reference moves over.
    unsafe { CFRetained::from_raw(NonNull::new(ptr).expect("non-null font")) }
}

/// A font of an installed or registered family, or `None` (logged) if the
/// family does not exist.
fn family_font(family: &str, size: CGFloat, weight: u16) -> Option<CFRetained<CTFont>> {
    let name = CFString::from_str(family);
    let weight = CFNumber::new_cgfloat(font_weight(weight));
    let traits = CFDictionary::<CFString, CFType>::from_slices(
        // SAFETY: immutable CoreText constants.
        &[unsafe { kCTFontWeightTrait }],
        &[weight.as_ref()],
    );
    // SAFETY: immutable CoreText constants.
    let keys = unsafe { [kCTFontFamilyNameAttribute, kCTFontTraitsAttribute] };
    let attributes =
        CFDictionary::<CFString, CFType>::from_slices(&keys, &[name.as_ref(), traits.as_ref()]);
    // SAFETY: attribute values have the types CoreText documents.
    let font = unsafe {
        let descriptor = CTFontDescriptor::with_attributes(attributes.as_opaque());
        CTFont::with_font_descriptor(&descriptor, size, std::ptr::null())
    };
    // An unknown family silently resolves to a default font: detect it.
    // SAFETY: a valid font.
    let resolved = unsafe { font.family_name() }.to_string();
    if resolved.eq_ignore_ascii_case(family) {
        Some(font)
    } else {
        log::warn!("font family `{family}` is not available; using the system font");
        None
    }
}

/// The system font as a `CTFont`. `UIFont` is toll-free bridged with
/// `CTFont`, and the UIKit constructor is the supported way to get the
/// system font at an arbitrary weight.
fn system_font(size: CGFloat, weight: u16) -> CFRetained<CTFont> {
    let font: Retained<UIFont> = UIFont::systemFontOfSize_weight(size, font_weight(weight));
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
    string: CFRetained<CFMutableAttributedString>,
    /// String length in UTF-16 units.
    length: CFIndex,
    max_lines: u32,
    ellipsize: EllipsizeMode,
    align: TextAlign,
}

/// A finished line and its origin (CoreText's y-up space, from the bottom of
/// the box).
pub(crate) struct LaidLine {
    pub(crate) line: CFRetained<CTLine>,
    pub(crate) origin: CGPoint,
}

/// Finished lines, drawn by [`crate::text_view::TextView`].
pub(crate) struct CoreTextLayout {
    pub(crate) lines: Vec<LaidLine>,
    /// Some run carries a strikethrough.
    pub(crate) strikethrough: bool,
}

// SAFETY: lines are immutable once created, and CoreText objects are
// thread-safe; they are created on the engine thread and only read (drawn)
// on the main thread.
unsafe impl Send for CoreTextLayout {}
// SAFETY: see `Send`.
unsafe impl Sync for CoreTextLayout {}

/// [`TextEngine`] backed by CoreText.
#[derive(Default)]
pub struct CoreTextEngine {
    fonts: FontCache,
}

const WHOLE: CFRange = CFRange {
    location: 0,
    length: 0,
};

impl CoreTextEngine {
    fn attributed_string(&self, text: &AttributedText) -> CFRetained<CFMutableAttributedString> {
        let string =
            CFMutableAttributedString::new(None, 0).expect("CFAttributedStringCreateMutable");
        let s = Some(&*string);
        // SAFETY: valid mutable string and range.
        unsafe {
            CFMutableAttributedString::replace_string(
                s,
                WHOLE,
                Some(&CFString::from_str(&text.text)),
            )
        };
        let length = text.spans.last().map_or(0, |span| span.end as CFIndex);
        CFMutableAttributedString::begin_editing(s);
        let set = |range: CFRange, key: &CFString, value: &CFType| {
            // SAFETY: values have the types CoreText requires for their keys
            // (checked at each call), and every range lies within the string
            // because span offsets are UTF-16 units, like CFString.
            unsafe { CFMutableAttributedString::set_attribute(s, range, Some(key), Some(value)) }
        };
        let paragraph = paragraph_style(text);
        // SAFETY: immutable CoreText constants (here and below).
        set(
            CFRange {
                location: 0,
                length,
            },
            unsafe { kCTParagraphStyleAttributeName },
            paragraph.as_ref(),
        );
        for span in &text.spans {
            let range = CFRange {
                location: span.start as CFIndex,
                length: (span.end - span.start) as CFIndex,
            };
            let style = &span.style;
            set(
                range,
                unsafe { kCTFontAttributeName },
                self.fonts.get(style).as_ref(),
            );
            set(
                range,
                unsafe { kCTForegroundColorAttributeName },
                cg_color(style.color).as_ref(),
            );
            if style.letter_spacing != 0.0 {
                let kern = CFNumber::new_cgfloat(style.letter_spacing as CGFloat);
                set(range, unsafe { kCTKernAttributeName }, kern.as_ref());
            }
            let decoration = cg_color(style.decoration_color.unwrap_or(style.color));
            let custom = style.decoration_style != yf_core::DecorationStyle::Solid
                || style.decoration_thickness.is_some()
                || style.underline_offset.is_some();
            if custom && !style.decoration.is_none() {
                let code = match style.decoration_style {
                    yf_core::DecorationStyle::Solid => 0,
                    yf_core::DecorationStyle::Double => 1,
                    yf_core::DecorationStyle::Dotted => 2,
                    yf_core::DecorationStyle::Dashed => 3,
                    yf_core::DecorationStyle::Wavy => 4,
                };
                let numbers = [
                    CFNumber::new_i32(code),
                    CFNumber::new_cgfloat(style.decoration_thickness.unwrap_or(0.0) as CGFloat),
                    CFNumber::new_cgfloat(
                        style
                            .underline_offset
                            .map_or(CGFloat::NAN, |o| o as CGFloat),
                    ),
                ];
                let array = CFArray::from_retained_objects(&numbers);
                set(range, decoration_key(), array.as_ref());
            }
            if style.decoration.underline && custom {
                set(range, underline_key(), decoration.as_ref());
            } else if style.decoration.underline {
                // kCTUnderlineStyleSingle.
                let single = CFNumber::new_i32(1);
                set(
                    range,
                    unsafe { kCTUnderlineStyleAttributeName },
                    single.as_ref(),
                );
                set(
                    range,
                    unsafe { kCTUnderlineColorAttributeName },
                    decoration.as_ref(),
                );
            }
            if style.decoration.line_through {
                set(range, strikethrough_key(), decoration.as_ref());
            }
        }
        CFMutableAttributedString::end_editing(s);
        string
    }
}

fn paragraph_style(text: &AttributedText) -> CFRetained<CTParagraphStyle> {
    let alignment = match text.align {
        // React Native: the right side in right-to-left layouts.
        TextAlign::Auto if text.rtl => CTTextAlignment::Right,
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
    // `writingDirection`; natural (from the content) by default.
    let direction = match text.writing_direction {
        None => CTWritingDirection::Natural,
        Some(false) => CTWritingDirection::LeftToRight,
        Some(true) => CTWritingDirection::RightToLeft,
    };
    let mut settings = vec![
        setting(
            CTParagraphStyleSpecifier::Alignment,
            (&raw const alignment).cast(),
            size_of::<CTTextAlignment>(),
        ),
        setting(
            CTParagraphStyleSpecifier::BaseWritingDirection,
            (&raw const direction).cast(),
            size_of::<CTWritingDirection>(),
        ),
    ];
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

impl Prepared {
    fn typesetter(&self) -> CFRetained<CTTypesetter> {
        // SAFETY: a valid framesetter.
        unsafe { self.framesetter.typesetter() }
    }

    /// Length (UTF-16 units) of the first `lines` lines at `width`.
    fn lines_length(&self, lines: u32, width: f64) -> CFIndex {
        let typesetter = self.typesetter();
        let mut end = 0;
        for _ in 0..lines {
            if end >= self.length {
                break;
            }
            // SAFETY: `end` lies within the string.
            let count = unsafe { typesetter.suggest_line_break(end, width) };
            if count <= 0 {
                break;
            }
            end += count;
        }
        end
    }

    /// `line` (starting at `start`) cut to `width` with an ellipsis, as
    /// `ellipsizeMode` says, standing for the rest of the text.
    fn truncate(&self, start: CFIndex, width: f64) -> Option<CFRetained<CTLine>> {
        let kind = match self.ellipsize {
            EllipsizeMode::Tail => CTLineTruncationType::End,
            EllipsizeMode::Head => CTLineTruncationType::Start,
            EllipsizeMode::Middle => CTLineTruncationType::Middle,
            EllipsizeMode::Clip => return None,
        };
        // SAFETY: ranges lie within the string; the token copies the
        // attributes of the last visible character.
        unsafe {
            let rest = self.typesetter().line(CFRange {
                location: start,
                length: self.length - start,
            });
            let token_string =
                CFMutableAttributedString::new(None, 0).expect("CFAttributedStringCreateMutable");
            CFMutableAttributedString::replace_string(
                Some(&token_string),
                WHOLE,
                Some(&CFString::from_static_str("\u{2026}")),
            );
            let source: &CFAttributedString = &self.string;
            let attributes = CFAttributedString::attributes(
                source,
                (self.length - 1).max(start),
                std::ptr::null_mut(),
            );
            if let Some(attributes) = attributes {
                CFMutableAttributedString::set_attributes(
                    Some(&token_string),
                    CFRange {
                        location: 0,
                        length: 1,
                    },
                    Some(&attributes),
                    false,
                );
            }
            let token = CTLine::with_attributed_string(&token_string);
            rest.truncated_line(width, kind, Some(&token))
        }
    }

    fn flush_factor(&self) -> CGFloat {
        match self.align {
            TextAlign::Center => 0.5,
            TextAlign::Right => 1.0,
            TextAlign::Auto | TextAlign::Left | TextAlign::Justify => 0.0,
        }
    }
}

impl TextEngine for CoreTextEngine {
    type Prepared = Prepared;

    fn prepare(&self, text: &AttributedText) -> Prepared {
        let string = self.attributed_string(text);
        let attributed: &CFAttributedString = &string;
        // SAFETY: a valid attributed string.
        let framesetter = unsafe { CTFramesetter::with_attributed_string(attributed) };
        Prepared {
            framesetter,
            length: text.text.encode_utf16().count() as CFIndex,
            string,
            max_lines: text.max_lines,
            ellipsize: text.ellipsize,
            align: text.align,
        }
    }

    fn measure(&self, prepared: &Prepared, max_width: Option<f32>) -> Size {
        let width = max_width.map_or(CGFloat::MAX, |w| w as CGFloat);
        let range = if prepared.max_lines > 0 {
            CFRange {
                location: 0,
                length: prepared.lines_length(prepared.max_lines, width),
            }
        } else {
            WHOLE
        };
        // SAFETY: a range within the string, no frame attributes, no fit
        // range out-param.
        let size = unsafe {
            prepared.framesetter.suggest_frame_size_with_constraints(
                range,
                None,
                CGSize::new(width, CGFloat::MAX),
                std::ptr::null_mut(),
            )
        };
        Size::new(size.width as f32, size.height as f32)
    }

    fn layout(&self, prepared: &Prepared, size: Size) -> TextLayout {
        let (width, height) = (size.width as CGFloat, size.height as CGFloat);
        let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width, height));
        // SAFETY: a valid rectangle path and whole-string range.
        let frame: CFRetained<CTFrame> = unsafe {
            let path = CGPath::with_rect(rect, std::ptr::null());
            prepared.framesetter.frame(WHOLE, &path, None)
        };
        // SAFETY: the frame's own line array (of CTLines) and origins.
        let mut lines = unsafe {
            let array = frame.lines();
            let count = array.count();
            let mut origins = vec![CGPoint::new(0.0, 0.0); count as usize];
            if let Some(ptr) = NonNull::new(origins.as_mut_ptr()) {
                frame.line_origins(WHOLE, ptr);
            }
            (0..count)
                .map(|i| {
                    let line = array.value_at_index(i).cast::<CTLine>();
                    LaidLine {
                        line: CFRetained::retain(NonNull::new(line.cast_mut()).expect("line")),
                        origin: origins[i as usize],
                    }
                })
                .collect::<Vec<_>>()
        };
        if prepared.max_lines > 0 {
            lines.truncate(prepared.max_lines as usize);
        }
        // Text left over (cut by `numberOfLines` or the box): ellipsize the
        // last line.
        if let Some(last) = lines.last_mut() {
            // SAFETY: a valid line.
            let range = unsafe { last.line.string_range() };
            if range.location + range.length < prepared.length
                && let Some(truncated) = prepared.truncate(range.location, width)
            {
                // SAFETY: a valid line.
                let x = unsafe { truncated.pen_offset_for_flush(prepared.flush_factor(), width) };
                last.origin.x = x;
                last.line = truncated;
            }
        }
        TextLayout(std::sync::Arc::new(CoreTextLayout {
            strikethrough: has_strikethrough(prepared),
            lines,
        }))
    }

    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        // CoreText registers fonts synchronously only from files: write the
        // data to a content-addressed file first.
        let path = font_file(data).map_err(|e| format!("cannot store font: {e}"))?;
        let url = CFURL::from_file_path(&path).ok_or("bad font path")?;
        let mut error = std::ptr::null_mut();
        // SAFETY: a valid file URL and error out-param.
        let ok = unsafe {
            CTFontManagerRegisterFontsForURL(&url, CTFontManagerScope::Process, &mut error)
        };
        // Fonts resolve differently now (a family may have been missing).
        self.fonts.clear();
        if ok {
            return Ok(());
        }
        // SAFETY: on failure the out-param holds a +1 CFError.
        let error = unsafe { CFRetained::from_raw(NonNull::new(error).ok_or("unknown error")?) };
        // kCTFontManagerErrorAlreadyRegistered: e.g. after a hot reload.
        if error.code() == 105 {
            return Ok(());
        }
        Err(error
            .description()
            .map_or_else(|| "font rejected".to_owned(), |d| d.to_string()))
    }
}

/// Writes font data to `<temp>/yf-fonts/<hash>.font` (once).
fn font_file(data: &[u8]) -> std::io::Result<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    data.hash(&mut hasher);
    let dir = std::env::temp_dir().join("yf-fonts");
    let path = dir.join(format!("{:016x}.font", hasher.finish()));
    if !path.exists() {
        std::fs::create_dir_all(&dir)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &path)?;
    }
    Ok(path)
}

fn has_strikethrough(prepared: &Prepared) -> bool {
    let mut index = 0;
    let source: &CFAttributedString = &prepared.string;
    while index < prepared.length {
        let mut range = CFRange {
            location: 0,
            length: 0,
        };
        // SAFETY: `index` lies within the string; `range` is a valid out-param.
        let value = unsafe {
            CFAttributedString::attribute(source, index, Some(strikethrough_key()), &mut range)
        };
        if value.is_some() {
            return true;
        }
        let mut other = CFRange {
            location: 0,
            length: 0,
        };
        // SAFETY: as above.
        let underline = unsafe {
            CFAttributedString::attribute(source, index, Some(underline_key()), &mut other)
        };
        if underline.is_some() {
            return true;
        }
        if other.location + other.length < range.location + range.length {
            range = other;
        }
        index = (range.location + range.length).max(index + 1);
    }
    false
}

/// Warms up font and CoreText caches (process-wide) on a background thread
/// so the first real layout does not pay for them.
pub(crate) fn prewarm() {
    let engine = CoreTextEngine::default();
    for (size, weight) in [(14.0, 400), (17.0, 400), (17.0, 700)] {
        let mut text = AttributedText::default();
        let style = ResolvedTextStyle {
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
