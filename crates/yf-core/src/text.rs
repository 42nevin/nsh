use std::{
    any::Any,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

use rustc_hash::FxHashMap;

use crate::{
    geometry::Size,
    ops::{EventMask, NodeId},
    style::{
        Color, EllipsizeMode, FontFamily, FontVariant, TextAlign, TextDecoration, TextShadow,
        TextStyle, TextTransform,
    },
};

/// Fully resolved character style of a text run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedTextStyle {
    /// Foreground color.
    pub color: Color,
    /// Font size in points.
    pub font_size: f32,
    /// Font weight, `100..=900`.
    pub font_weight: u16,
    /// Line height in points; `0.0` means the font's natural line height.
    pub line_height: f32,
    /// Font family; `None` is the system font.
    pub font_family: Option<FontFamily>,
    /// Italic.
    pub italic: bool,
    /// Extra space between characters in points.
    pub letter_spacing: f32,
    /// Underline / strikethrough.
    pub decoration: TextDecoration,
    /// Color of the decoration lines; `None` uses `color`.
    pub decoration_color: Option<Color>,
    /// Case transform (applied by the core when building the text).
    pub transform: TextTransform,
    /// OpenType features (`fontVariant`).
    pub font_variant: FontVariant,
}

impl Default for ResolvedTextStyle {
    /// React Native defaults.
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            font_size: 14.0,
            font_weight: 400,
            line_height: 0.0,
            font_family: None,
            italic: false,
            letter_spacing: 0.0,
            decoration: TextDecoration {
                underline: false,
                line_through: false,
            },
            decoration_color: None,
            transform: TextTransform::None,
            font_variant: FontVariant(0),
        }
    }
}

impl ResolvedTextStyle {
    /// Applies the properties set on a node on top of the inherited style.
    #[inline]
    pub(crate) fn inherit(self, style: &TextStyle) -> Self {
        Self {
            color: style.color.unwrap_or(self.color),
            font_size: style.font_size.unwrap_or(self.font_size),
            font_weight: style.font_weight.unwrap_or(self.font_weight),
            line_height: style.line_height.unwrap_or(self.line_height),
            font_family: style.font_family.or(self.font_family),
            italic: style.italic.unwrap_or(self.italic),
            letter_spacing: style.letter_spacing.unwrap_or(self.letter_spacing),
            decoration: style.decoration.unwrap_or(self.decoration),
            decoration_color: style.decoration_color.or(self.decoration_color),
            transform: style.transform.unwrap_or(self.transform),
            font_variant: style.font_variant.unwrap_or(self.font_variant),
        }
    }

    /// True if both styles produce the same glyph metrics (colors and
    /// decorations ignored).
    #[inline]
    fn layout_eq(&self, other: &Self) -> bool {
        self.font_size == other.font_size
            && self.font_weight == other.font_weight
            && self.line_height == other.line_height
            && self.font_family == other.font_family
            && self.italic == other.italic
            && self.letter_spacing == other.letter_spacing
            && self.font_variant == other.font_variant
    }
}

/// A run of characters with one style.
///
/// Offsets are in UTF-16 code units, which is what both Apple
/// (`NSAttributedString`) and Android (`Spannable`) text APIs index by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextSpan {
    /// Start offset (UTF-16 units, inclusive).
    pub start: u32,
    /// End offset (UTF-16 units, exclusive).
    pub end: u32,
    /// Style of the run.
    pub style: ResolvedTextStyle,
}

/// The merged content of a `<text>` block: its own text plus all nested
/// spans and raw text nodes, flattened into one string with style runs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AttributedText {
    /// The full string.
    pub text: String,
    /// Non-overlapping, sorted style runs covering `text`.
    pub spans: Vec<TextSpan>,
    /// Paragraph alignment.
    pub align: TextAlign,
    /// Maximum number of lines; 0 means no limit (`numberOfLines`).
    pub max_lines: u32,
    /// How text beyond `max_lines` is cut.
    pub ellipsize: EllipsizeMode,
    /// Shadow behind the whole block (drawing only, not layout).
    pub shadow: Option<TextShadow>,
    /// Nested `<text>` spans that take presses (they may nest).
    pub links: Vec<TextLink>,
    /// `writingDirection`: right to left (`Some(true)`), left to right, or
    /// from the content (`None`).
    pub writing_direction: Option<bool>,
    /// The block is laid out right to left: `auto` alignment is the right
    /// side.
    pub rtl: bool,
}

/// A nested `<text>` span with press listeners: presses on its characters
/// go to it rather than to the block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLink {
    /// Start offset (UTF-16 units, inclusive).
    pub start: u32,
    /// End offset (UTF-16 units, exclusive).
    pub end: u32,
    /// The span's node.
    pub node: NodeId,
    /// Its listeners.
    pub events: EventMask,
}

impl AttributedText {
    /// Length of the text so far, in UTF-16 units.
    pub fn len_utf16(&self) -> u32 {
        self.spans.last().map_or(0, |s| s.end)
    }

    /// The innermost link containing the UTF-16 offset `index`.
    pub fn link_at(&self, index: u32) -> Option<&TextLink> {
        self.links
            .iter()
            .filter(|l| index >= l.start && index < l.end)
            .min_by_key(|l| l.end - l.start)
    }

    /// Appends `s` with `style`, merging with the previous run when the
    /// style is identical.
    pub fn push(&mut self, s: &str, style: ResolvedTextStyle) {
        if s.is_empty() {
            return;
        }
        let s = &*style.transform.apply(s);
        let len = s.encode_utf16().count() as u32;
        self.text.push_str(s);
        match self.spans.last_mut() {
            Some(last) if last.style == style => last.end += len,
            last => {
                let start = last.map_or(0, |l| l.end);
                self.spans.push(TextSpan {
                    start,
                    end: start + len,
                    style,
                });
            }
        }
    }

    /// True if both texts lay out identically: same characters, runs,
    /// metrics and alignment. Decorative attributes (colors) are ignored.
    pub fn layout_eq(&self, other: &Self) -> bool {
        self.align == other.align
            && self.writing_direction == other.writing_direction
            && self.rtl == other.rtl
            && self.max_lines == other.max_lines
            && self.ellipsize == other.ellipsize
            && self.text == other.text
            && self.spans.len() == other.spans.len()
            && self
                .spans
                .iter()
                .zip(&other.spans)
                .all(|(a, b)| a.start == b.start && a.end == b.end && a.style.layout_eq(&b.style))
    }

    /// A 64-bit hash of everything [`AttributedText::layout_eq`] compares.
    /// SipHash (not Fx): it keys a cache without a full equality check.
    pub fn layout_hash(&self) -> u64 {
        let mut h = std::hash::DefaultHasher::new();
        self.text.hash(&mut h);
        (self.align as u8, self.writing_direction, self.rtl).hash(&mut h);
        (self.max_lines, self.ellipsize).hash(&mut h);
        for s in &self.spans {
            (s.start, s.end, s.style.font_weight).hash(&mut h);
            (s.style.font_family, s.style.italic, s.style.font_variant).hash(&mut h);
            s.style.font_size.to_bits().hash(&mut h);
            s.style.line_height.to_bits().hash(&mut h);
            s.style.letter_spacing.to_bits().hash(&mut h);
        }
        h.finish()
    }
}

/// An immutable, fully laid out text produced by a [`TextEngine`] on the
/// engine thread and drawn as-is by the platform on the UI thread, so text
/// is typeset exactly once.
#[derive(Clone)]
pub struct TextLayout(pub Arc<dyn Any + Send + Sync>);

impl TextLayout {
    /// Returns the platform layout object.
    pub fn downcast<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

impl fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextLayout")
    }
}

/// The platform text engine (CoreText, Android `StaticLayout`, ...).
///
/// All methods run on the engine thread, so implementations must not touch
/// UI-thread-only objects. The split mirrors how native text stacks work:
/// typesetting state is prepared once per content, measured at any number
/// of widths during layout, and finally laid out once at the final size.
pub trait TextEngine {
    /// Typesetting state for one content, reused across measurements
    /// (e.g. a `CTFramesetter`).
    type Prepared;

    /// Prepares `text` for measuring and layout.
    fn prepare(&self, text: &AttributedText) -> Self::Prepared;

    /// Size of the prepared text with an optional maximum width.
    fn measure(&self, prepared: &Self::Prepared, max_width: Option<f32>) -> Size;

    /// The final layout in a box of exactly `size`, for drawing.
    fn layout(&self, prepared: &Self::Prepared, size: Size) -> TextLayout;

    /// Makes the font in `data` (TrueType / OpenType) available by its
    /// family name. Fonts are registered before the text using them is
    /// measured.
    fn register_font(&self, data: &[u8]) -> Result<(), String> {
        let _ = data;
        Err("this text engine does not support custom fonts".to_owned())
    }
}

/// A deterministic, platform-free [`TextEngine`] for headless runs and
/// tests: every character is `0.55 × font size` wide and lines are
/// `1.2 × font size` tall.
#[derive(Debug, Default, Clone, Copy)]
pub struct ApproxTextEngine;

impl TextEngine for ApproxTextEngine {
    /// `(character count, font size, max lines)`.
    type Prepared = (usize, f32, u32);

    fn prepare(&self, text: &AttributedText) -> (usize, f32, u32) {
        let size = text.spans.first().map_or(14.0, |s| s.style.font_size);
        (text.text.chars().count(), size, text.max_lines)
    }

    fn measure(
        &self,
        &(chars, size, max_lines): &(usize, f32, u32),
        max_width: Option<f32>,
    ) -> Size {
        let width = chars as f32 * size * 0.55;
        let line = size * 1.2;
        let (width, mut lines) = match max_width {
            Some(max) if width > max && max > 0.0 => (max, (width / max).ceil()),
            _ => (width, 1.0),
        };
        if max_lines > 0 {
            lines = lines.min(max_lines as f32);
        }
        Size::new(width, lines * line)
    }

    fn layout(&self, _: &(usize, f32, u32), size: Size) -> TextLayout {
        TextLayout(Arc::new(size))
    }

    fn register_font(&self, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

/// Content-addressed text measurements shared by all nodes of a surface,
/// so identical texts (list rows, recycled cells) are measured once.
///
/// Approximate LRU with two generations: lookups hit the current one or
/// promote from the previous one; when the current generation is full it
/// becomes the previous one. O(1), and never more than `2 * GENERATION`
/// entries.
#[derive(Default)]
pub(crate) struct MeasureCache {
    current: FxHashMap<(u64, u32), Size>,
    previous: FxHashMap<(u64, u32), Size>,
}

impl MeasureCache {
    const GENERATION: usize = 512;

    #[inline]
    fn key(layout_hash: u64, max_width: Option<f32>) -> (u64, u32) {
        // NaN never comes from taffy, so it can stand for "unconstrained".
        (layout_hash, max_width.unwrap_or(f32::NAN).to_bits())
    }

    pub(crate) fn get(&mut self, layout_hash: u64, max_width: Option<f32>) -> Option<Size> {
        let key = Self::key(layout_hash, max_width);
        if let Some(&size) = self.current.get(&key) {
            return Some(size);
        }
        let size = self.previous.remove(&key)?;
        self.insert_key(key, size);
        Some(size)
    }

    pub(crate) fn insert(&mut self, layout_hash: u64, max_width: Option<f32>, size: Size) {
        self.insert_key(Self::key(layout_hash, max_width), size);
    }

    fn insert_key(&mut self, key: (u64, u32), size: Size) {
        if self.current.len() >= Self::GENERATION {
            self.previous = std::mem::take(&mut self.current);
            self.current.reserve(Self::GENERATION);
        }
        self.current.insert(key, size);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.current.len() + self.previous.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_merges_and_counts_utf16() {
        let mut t = AttributedText::default();
        let a = ResolvedTextStyle::default();
        let b = ResolvedTextStyle {
            font_weight: 700,
            ..a
        };
        t.push("ab", a);
        t.push("", b);
        t.push("😀", a);
        t.push("c", b);
        assert_eq!(t.text, "ab😀c");
        assert_eq!(t.spans.len(), 2);
        assert_eq!((t.spans[0].start, t.spans[0].end), (0, 4));
        assert_eq!((t.spans[1].start, t.spans[1].end), (4, 5));
    }

    #[test]
    fn color_does_not_affect_layout_identity() {
        let mut a = AttributedText::default();
        a.push("hello", ResolvedTextStyle::default());
        let mut b = AttributedText::default();
        b.push(
            "hello",
            ResolvedTextStyle {
                color: Color(0xff00_00ff),
                ..Default::default()
            },
        );
        assert_ne!(a, b);
        assert!(a.layout_eq(&b));
        assert_eq!(a.layout_hash(), b.layout_hash());
        let mut c = b.clone();
        c.spans[0].style.font_size = 15.0;
        assert!(!a.layout_eq(&c));
        assert_ne!(a.layout_hash(), c.layout_hash());
    }

    #[test]
    fn measure_cache_is_bounded_and_promotes() {
        let mut cache = MeasureCache::default();
        cache.insert(1, Some(100.0), Size::new(1.0, 1.0));
        for i in 0..10_000u64 {
            cache.insert(1000 + i, None, Size::default());
            // Keep entry 1 hot: every lookup promotes it.
            assert!(cache.get(1, Some(100.0)).is_some());
            assert!(cache.len() <= 2 * MeasureCache::GENERATION);
        }
        assert!(cache.get(1, Some(99.0)).is_none());
        assert!(cache.get(1000, None).is_none(), "cold entries are evicted");
    }
}
