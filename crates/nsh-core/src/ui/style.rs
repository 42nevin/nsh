//! Typed styles of Rust components ([`Styled`]): GPUI-style methods
//! (`.px_4()`, `.rounded_xl()`, `.bg(color)`) recording declarations that
//! lower into the same rules Tailwind classes compile to, without strings
//! to parse. Values follow Tailwind's default scales (4pt spacing steps),
//! independent of the app's Tailwind theme, so a component looks the same
//! in every app.
//!
//! What changes without a render is native, as with classes: theme colors
//! carry a light and a dark value ([`AdaptiveColor`]), `.active(|s| …)`,
//! `.hover(…)` and `.focus(…)` follow the view's interaction states. What a
//! render knows (disabled, a variant) is plain Rust: `.when(cond, …)`.

use std::{
    hash::{Hash, Hasher},
    sync::{Arc, LazyLock},
};

use super::SharedString;
use crate::{
    style::{Color, PatchValue, StyleKey},
    tailwind::{self, ClassList, Decl, Part, Rule, rank, state},
};

/// A length: points, a fraction of the parent, or automatic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Points.
    Points(f32),
    /// A fraction of the parent's size (`0.5` is 50%).
    Relative(f32),
    /// Sized by layout.
    Auto,
}

impl Eq for Length {}

impl Hash for Length {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Length::Points(p) => (0u8, p.to_bits()).hash(state),
            Length::Relative(f) => (1u8, f.to_bits()).hash(state),
            Length::Auto => 2u8.hash(state),
        }
    }
}

/// `points` points.
pub const fn px(points: f32) -> Length {
    Length::Points(points)
}

/// `fraction` of the parent (`relative(0.5)` is 50%).
pub const fn relative(fraction: f32) -> Length {
    Length::Relative(fraction)
}

impl Length {
    fn patch(self) -> PatchValue {
        match self {
            Length::Points(p) => PatchValue::Number(p),
            Length::Relative(f) => PatchValue::Str(format!("{}%", f * 100.0).into()),
            Length::Auto => PatchValue::Str("auto".into()),
        }
    }
}

/// A font weight (100–900).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontWeight(pub u16);

impl FontWeight {
    /// 100.
    pub const THIN: Self = Self(100);
    /// 200.
    pub const EXTRA_LIGHT: Self = Self(200);
    /// 300.
    pub const LIGHT: Self = Self(300);
    /// 400.
    pub const NORMAL: Self = Self(400);
    /// 500.
    pub const MEDIUM: Self = Self(500);
    /// 600.
    pub const SEMIBOLD: Self = Self(600);
    /// 700.
    pub const BOLD: Self = Self(700);
    /// 800.
    pub const EXTRA_BOLD: Self = Self(800);
    /// 900.
    pub const BLACK: Self = Self(900);
}

/// `0xRRGGBB`, opaque.
pub const fn rgb(hex: u32) -> Color {
    Color(hex << 8 | 0xff)
}

/// `0xRRGGBBAA`.
pub const fn rgba(hex: u32) -> Color {
    Color(hex)
}

/// A color for each appearance: the platform switches between them
/// without a render. Theme colors are adaptive; a plain [`Color`] is the
/// same in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AdaptiveColor {
    /// In light appearance.
    pub light: Color,
    /// In dark appearance.
    pub dark: Color,
}

impl AdaptiveColor {
    /// Transparent in both.
    pub const TRANSPARENT: Self = Self {
        light: Color::TRANSPARENT,
        dark: Color::TRANSPARENT,
    };

    /// `light` and `dark`.
    pub const fn new(light: Color, dark: Color) -> Self {
        Self { light, dark }
    }

    /// Both with their alpha multiplied by `opacity`.
    pub fn opacity(self, opacity: f32) -> Self {
        let fade = |c: Color| {
            let a = ((c.0 & 0xff) as f32 * opacity.clamp(0.0, 1.0)).round() as u32;
            Color(c.0 & !0xff | a)
        };
        Self {
            light: fade(self.light),
            dark: fade(self.dark),
        }
    }
}

impl From<Color> for AdaptiveColor {
    fn from(c: Color) -> Self {
        Self { light: c, dark: c }
    }
}

/// An `f32` compared and hashed by its bits (styles are cache keys).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bits(f32);

impl PartialEq for Bits {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Bits {}

impl Hash for Bits {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state)
    }
}

/// When a declaration applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Variant {
    Base,
    Hover,
    Focus,
    Active,
}

/// One typed declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Declaration {
    Number(StyleKey, Bits),
    Length(StyleKey, Length),
    Keyword(StyleKey, &'static str),
    Str(StyleKey, SharedString),
    Color(StyleKey, AdaptiveColor),
    /// Tailwind's `text-*` size and line height.
    TextPreset(&'static str),
    /// Tailwind's `shadow-*` (`"none"` removes it).
    Shadow(&'static str),
    Scale(Bits),
    TranslateX(Bits),
    TranslateY(Bits),
    Rotate(Bits),
}

/// Tailwind's default theme: the presets typed styles name.
static PRESETS: LazyLock<tailwind::Theme> = LazyLock::new(tailwind::Theme::default);

fn color_patch(c: Color) -> PatchValue {
    PatchValue::Str(format!("#{:08x}", c.0).into())
}

impl Declaration {
    fn lower(&self, out: &mut Vec<Decl>) {
        match self {
            Declaration::Number(key, n) => out.push(Decl::Set(*key, PatchValue::Number(n.0))),
            Declaration::Length(key, l) => out.push(Decl::Set(*key, l.patch())),
            Declaration::Keyword(key, k) => out.push(Decl::Set(*key, PatchValue::Str((*k).into()))),
            Declaration::Str(key, s) => {
                out.push(Decl::Set(*key, PatchValue::Str(s.as_str().into())))
            }
            Declaration::Color(key, c) => out.push(if c.light == c.dark {
                Decl::Set(*key, color_patch(c.light))
            } else {
                Decl::Adaptive(*key, [color_patch(c.light), color_patch(c.dark)])
            }),
            Declaration::TextPreset(name) => {
                if let Some(&(size, line)) = PRESETS.text.get(*name) {
                    out.push(Decl::Set(StyleKey::FontSize, PatchValue::Number(size)));
                    if let Some(m) = line {
                        out.push(Decl::Set(
                            StyleKey::LineHeight,
                            PatchValue::Str(m.to_string().into()),
                        ));
                    }
                }
            }
            Declaration::Shadow(name) => {
                let layers = PRESETS.shadows.get(*name).cloned().unwrap_or_default();
                out.push(Decl::Part(Part::Shadow(Arc::from(layers))));
            }
            Declaration::Scale(s) => {
                out.push(Decl::Part(Part::ScaleX(s.0)));
                out.push(Decl::Part(Part::ScaleY(s.0)));
            }
            Declaration::TranslateX(p) => {
                out.push(Decl::Part(Part::TranslateX(tailwind::Len::Points(p.0))))
            }
            Declaration::TranslateY(p) => {
                out.push(Decl::Part(Part::TranslateY(tailwind::Len::Points(p.0))))
            }
            Declaration::Rotate(d) => out.push(Decl::Part(Part::Rotate(d.0))),
        }
    }
}

type Entries = Vec<(Variant, Declaration)>;

/// An element's styles, in order (later declarations win). Built by the
/// [`Styled`] methods; `.active(|s| …)` and the like hand a fresh one to
/// their closure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct StyleRefinement {
    entries: Entries,
}

impl StyleRefinement {
    pub(crate) fn push(&mut self, decl: Declaration) {
        // Elements take several declarations: no regrowth for the first.
        if self.entries.capacity() == 0 {
            self.entries.reserve(8);
        }
        self.entries.push((Variant::Base, decl));
    }

    pub(crate) fn lengths(&mut self, keys: &[StyleKey], length: Length) {
        for key in keys {
            self.push(Declaration::Length(*key, length));
        }
    }

    pub(crate) fn number(&mut self, key: StyleKey, n: f32) {
        self.push(Declaration::Number(key, Bits(n)));
    }

    pub(crate) fn keyword(&mut self, key: StyleKey, k: &'static str) {
        self.push(Declaration::Keyword(key, k));
    }

    pub(crate) fn color(&mut self, keys: &[StyleKey], color: AdaptiveColor) {
        for key in keys {
            self.push(Declaration::Color(*key, color));
        }
    }

    /// `other`'s declarations, applying while in `variant`.
    pub(crate) fn extend_as(&mut self, variant: Variant, other: StyleRefinement) {
        self.entries
            .extend(other.entries.into_iter().map(|(_, d)| (variant, d)));
    }

    /// No declarations.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The rules, as Tailwind classes compile to.
    pub(crate) fn lower(&self) -> ClassList {
        let mut rules = Vec::new();
        for (variant, states, rank) in [
            (Variant::Base, 0, 0),
            (Variant::Hover, state::HOVERED, rank::HOVER),
            (Variant::Focus, state::FOCUSED, rank::FOCUS),
            (Variant::Active, state::PRESSED, rank::ACTIVE),
        ] {
            let mut decls = Vec::new();
            for (v, d) in &self.entries {
                if *v == variant {
                    d.lower(&mut decls);
                }
            }
            if !decls.is_empty() {
                rules.push(Rule::new(states, rank, decls));
            }
        }
        ClassList::from_rules(rules)
    }
}

/// Styling methods, named as in GPUI and Tailwind. Implemented by the
/// elements and by [`StyleRefinement`] (in `.active(|s| …)`).
pub trait Styled: Sized {
    /// The styles being built.
    fn style(&mut self) -> &mut StyleRefinement;

    nsh_macros::style_helpers! {
        /// Padding on all sides
        p => [Padding],
        /// Horizontal padding
        px => [PaddingHorizontal],
        /// Vertical padding
        py => [PaddingVertical],
        /// Top padding
        pt => [PaddingTop],
        /// Right padding
        pr => [PaddingRight],
        /// Bottom padding
        pb => [PaddingBottom],
        /// Left padding
        pl => [PaddingLeft],
        /// Leading padding (left in left-to-right layouts)
        ps => [PaddingStart],
        /// Trailing padding
        pe => [PaddingEnd],
        /// Margin on all sides
        m => [Margin],
        /// Horizontal margin
        mx => [MarginHorizontal],
        /// Vertical margin
        my => [MarginVertical],
        /// Top margin
        mt => [MarginTop],
        /// Right margin
        mr => [MarginRight],
        /// Bottom margin
        mb => [MarginBottom],
        /// Left margin
        ml => [MarginLeft],
        /// Leading margin
        ms => [MarginStart],
        /// Trailing margin
        me => [MarginEnd],
        /// Space between children
        gap => [Gap],
        /// Space between columns
        gap_x => [ColumnGap],
        /// Space between rows
        gap_y => [RowGap],
        /// Width
        w => [Width],
        /// Height
        h => [Height],
        /// Width and height
        size => [Width, Height],
        /// Minimum width
        min_w => [MinWidth],
        /// Minimum height
        min_h => [MinHeight],
        /// Maximum width
        max_w => [MaxWidth],
        /// Maximum height
        max_h => [MaxHeight],
        /// Distance from the parent's top edge (absolute layout)
        top => [Top],
        /// Distance from the parent's right edge
        right => [Right],
        /// Distance from the parent's bottom edge
        bottom => [Bottom],
        /// Distance from the parent's left edge
        left => [Left],
        /// Distance from every edge of the parent
        inset => [Top, Right, Bottom, Left],
        /// The flex basis
        basis => [FlexBasis],
    }

    // Layout.

    /// Flex layout (the default).
    fn flex(mut self) -> Self {
        self.style().keyword(StyleKey::Display, "flex");
        self
    }

    /// Grid layout.
    fn grid(mut self) -> Self {
        self.style().keyword(StyleKey::Display, "grid");
        self
    }

    /// Not laid out nor shown.
    fn hidden(mut self) -> Self {
        self.style().keyword(StyleKey::Display, "none");
        self
    }

    /// Children in a row.
    fn flex_row(mut self) -> Self {
        self.style().keyword(StyleKey::FlexDirection, "row");
        self
    }

    /// Children in a row, last first.
    fn flex_row_reverse(mut self) -> Self {
        self.style().keyword(StyleKey::FlexDirection, "row-reverse");
        self
    }

    /// Children in a column (the default).
    fn flex_col(mut self) -> Self {
        self.style().keyword(StyleKey::FlexDirection, "column");
        self
    }

    /// Children in a column, last first.
    fn flex_col_reverse(mut self) -> Self {
        self.style()
            .keyword(StyleKey::FlexDirection, "column-reverse");
        self
    }

    /// Children wrap onto new lines.
    fn flex_wrap(mut self) -> Self {
        self.style().keyword(StyleKey::FlexWrap, "wrap");
        self
    }

    /// Grows and shrinks, ignoring its own size (`flex: 1 1 0`).
    fn flex_1(mut self) -> Self {
        let s = self.style();
        s.number(StyleKey::FlexGrow, 1.0);
        s.number(StyleKey::FlexShrink, 1.0);
        s.lengths(&[StyleKey::FlexBasis], px(0.0));
        self
    }

    /// Grows and shrinks from its own size (`flex: 1 1 auto`).
    fn flex_auto(mut self) -> Self {
        let s = self.style();
        s.number(StyleKey::FlexGrow, 1.0);
        s.number(StyleKey::FlexShrink, 1.0);
        s.lengths(&[StyleKey::FlexBasis], Length::Auto);
        self
    }

    /// Neither grows nor shrinks.
    fn flex_none(mut self) -> Self {
        let s = self.style();
        s.number(StyleKey::FlexGrow, 0.0);
        s.number(StyleKey::FlexShrink, 0.0);
        self
    }

    /// Takes free space.
    fn flex_grow(mut self) -> Self {
        self.style().number(StyleKey::FlexGrow, 1.0);
        self
    }

    /// Never shrinks.
    fn flex_shrink_0(mut self) -> Self {
        self.style().number(StyleKey::FlexShrink, 0.0);
        self
    }

    /// Children at the cross axis' start.
    fn items_start(mut self) -> Self {
        self.style().keyword(StyleKey::AlignItems, "flex-start");
        self
    }

    /// Children centered on the cross axis.
    fn items_center(mut self) -> Self {
        self.style().keyword(StyleKey::AlignItems, "center");
        self
    }

    /// Children at the cross axis' end.
    fn items_end(mut self) -> Self {
        self.style().keyword(StyleKey::AlignItems, "flex-end");
        self
    }

    /// Children stretched across (the default).
    fn items_stretch(mut self) -> Self {
        self.style().keyword(StyleKey::AlignItems, "stretch");
        self
    }

    /// Children aligned on their text baseline.
    fn items_baseline(mut self) -> Self {
        self.style().keyword(StyleKey::AlignItems, "baseline");
        self
    }

    /// Children at the main axis' start (the default).
    fn justify_start(mut self) -> Self {
        self.style().keyword(StyleKey::JustifyContent, "flex-start");
        self
    }

    /// Children centered on the main axis.
    fn justify_center(mut self) -> Self {
        self.style().keyword(StyleKey::JustifyContent, "center");
        self
    }

    /// Children at the main axis' end.
    fn justify_end(mut self) -> Self {
        self.style().keyword(StyleKey::JustifyContent, "flex-end");
        self
    }

    /// Free space between children.
    fn justify_between(mut self) -> Self {
        self.style()
            .keyword(StyleKey::JustifyContent, "space-between");
        self
    }

    /// Free space around children.
    fn justify_around(mut self) -> Self {
        self.style()
            .keyword(StyleKey::JustifyContent, "space-around");
        self
    }

    /// Free space evenly between and around children.
    fn justify_evenly(mut self) -> Self {
        self.style()
            .keyword(StyleKey::JustifyContent, "space-evenly");
        self
    }

    /// Itself at the cross axis' start.
    fn self_start(mut self) -> Self {
        self.style().keyword(StyleKey::AlignSelf, "flex-start");
        self
    }

    /// Itself centered on the cross axis.
    fn self_center(mut self) -> Self {
        self.style().keyword(StyleKey::AlignSelf, "center");
        self
    }

    /// Itself at the cross axis' end.
    fn self_end(mut self) -> Self {
        self.style().keyword(StyleKey::AlignSelf, "flex-end");
        self
    }

    /// Itself stretched across.
    fn self_stretch(mut self) -> Self {
        self.style().keyword(StyleKey::AlignSelf, "stretch");
        self
    }

    /// The parent's full width.
    fn w_full(self) -> Self {
        self.w(relative(1.0))
    }

    /// The parent's full height.
    fn h_full(self) -> Self {
        self.h(relative(1.0))
    }

    /// The parent's full size.
    fn size_full(self) -> Self {
        self.size(relative(1.0))
    }

    /// Width over height.
    fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.style().number(StyleKey::AspectRatio, ratio);
        self
    }

    /// As wide as high.
    fn aspect_square(self) -> Self {
        self.aspect_ratio(1.0)
    }

    /// Laid out in the flow (the default).
    fn relative(mut self) -> Self {
        self.style().keyword(StyleKey::Position, "relative");
        self
    }

    /// Out of the flow, placed by `top` / `left` / ... in the parent.
    fn absolute(mut self) -> Self {
        self.style().keyword(StyleKey::Position, "absolute");
        self
    }

    /// Clips its children.
    fn overflow_hidden(mut self) -> Self {
        self.style().keyword(StyleKey::Overflow, "hidden");
        self
    }

    /// Stacking order among siblings.
    fn z_index(mut self, z: i32) -> Self {
        self.style().number(StyleKey::ZIndex, z as f32);
        self
    }

    // Visuals.

    /// The background color.
    fn bg(mut self, color: impl Into<AdaptiveColor>) -> Self {
        self.style()
            .color(&[StyleKey::BackgroundColor], color.into());
        self
    }

    /// Opacity, 0 (invisible) to 1.
    fn opacity(mut self, opacity: f32) -> Self {
        self.style()
            .number(StyleKey::Opacity, opacity.clamp(0.0, 1.0));
        self
    }

    /// The corner radius.
    fn rounded(mut self, radius: impl Into<Length>) -> Self {
        self.style()
            .lengths(&[StyleKey::BorderRadius], radius.into());
        self
    }

    /// No rounded corners.
    fn rounded_none(self) -> Self {
        self.rounded(px(0.0))
    }

    /// 2pt corners.
    fn rounded_xs(self) -> Self {
        self.rounded(px(2.0))
    }

    /// 4pt corners.
    fn rounded_sm(self) -> Self {
        self.rounded(px(4.0))
    }

    /// 6pt corners.
    fn rounded_md(self) -> Self {
        self.rounded(px(6.0))
    }

    /// 8pt corners.
    fn rounded_lg(self) -> Self {
        self.rounded(px(8.0))
    }

    /// 12pt corners.
    fn rounded_xl(self) -> Self {
        self.rounded(px(12.0))
    }

    /// 16pt corners.
    fn rounded_2xl(self) -> Self {
        self.rounded(px(16.0))
    }

    /// 24pt corners.
    fn rounded_3xl(self) -> Self {
        self.rounded(px(24.0))
    }

    /// Fully round ends (pills, circles).
    fn rounded_full(self) -> Self {
        self.rounded(px(9999.0))
    }

    /// The top corners' radius.
    fn rounded_t(mut self, radius: impl Into<Length>) -> Self {
        self.style().lengths(
            &[
                StyleKey::BorderTopLeftRadius,
                StyleKey::BorderTopRightRadius,
            ],
            radius.into(),
        );
        self
    }

    /// The bottom corners' radius.
    fn rounded_b(mut self, radius: impl Into<Length>) -> Self {
        self.style().lengths(
            &[
                StyleKey::BorderBottomLeftRadius,
                StyleKey::BorderBottomRightRadius,
            ],
            radius.into(),
        );
        self
    }

    /// The border width on all sides.
    fn border(mut self, width: f32) -> Self {
        self.style().number(StyleKey::BorderWidth, width);
        self
    }

    /// A 1pt border.
    fn border_1(self) -> Self {
        self.border(1.0)
    }

    /// A 2pt border.
    fn border_2(self) -> Self {
        self.border(2.0)
    }

    /// A 1pt top border.
    fn border_t_1(mut self) -> Self {
        self.style().number(StyleKey::BorderTopWidth, 1.0);
        self
    }

    /// A 1pt bottom border.
    fn border_b_1(mut self) -> Self {
        self.style().number(StyleKey::BorderBottomWidth, 1.0);
        self
    }

    /// A 1pt left border.
    fn border_l_1(mut self) -> Self {
        self.style().number(StyleKey::BorderLeftWidth, 1.0);
        self
    }

    /// A 1pt right border.
    fn border_r_1(mut self) -> Self {
        self.style().number(StyleKey::BorderRightWidth, 1.0);
        self
    }

    /// The border color.
    fn border_color(mut self, color: impl Into<AdaptiveColor>) -> Self {
        self.style().color(&[StyleKey::BorderColor], color.into());
        self
    }

    /// No shadow.
    fn shadow_none(mut self) -> Self {
        self.style().push(Declaration::Shadow("none"));
        self
    }

    /// Tailwind's `shadow-2xs`.
    fn shadow_2xs(mut self) -> Self {
        self.style().push(Declaration::Shadow("2xs"));
        self
    }

    /// Tailwind's `shadow-xs`.
    fn shadow_xs(mut self) -> Self {
        self.style().push(Declaration::Shadow("xs"));
        self
    }

    /// Tailwind's `shadow-sm`.
    fn shadow_sm(mut self) -> Self {
        self.style().push(Declaration::Shadow("sm"));
        self
    }

    /// Tailwind's `shadow-md`.
    fn shadow_md(mut self) -> Self {
        self.style().push(Declaration::Shadow("md"));
        self
    }

    /// Tailwind's `shadow-lg`.
    fn shadow_lg(mut self) -> Self {
        self.style().push(Declaration::Shadow("lg"));
        self
    }

    /// Tailwind's `shadow-xl`.
    fn shadow_xl(mut self) -> Self {
        self.style().push(Declaration::Shadow("xl"));
        self
    }

    /// Tailwind's `shadow-2xl`.
    fn shadow_2xl(mut self) -> Self {
        self.style().push(Declaration::Shadow("2xl"));
        self
    }

    /// Scales from the center (1 is the natural size).
    fn scale(mut self, scale: f32) -> Self {
        self.style().push(Declaration::Scale(Bits(scale)));
        self
    }

    /// Moves horizontally by `points` (after layout).
    fn translate_x(mut self, points: f32) -> Self {
        self.style().push(Declaration::TranslateX(Bits(points)));
        self
    }

    /// Moves vertically by `points` (after layout).
    fn translate_y(mut self, points: f32) -> Self {
        self.style().push(Declaration::TranslateY(Bits(points)));
        self
    }

    /// Rotates by `degrees`, clockwise.
    fn rotate(mut self, degrees: f32) -> Self {
        self.style().push(Declaration::Rotate(Bits(degrees)));
        self
    }

    // Text (inherited by the texts inside).

    /// The text color.
    fn text_color(mut self, color: impl Into<AdaptiveColor>) -> Self {
        self.style().color(&[StyleKey::Color], color.into());
        self
    }

    /// The font size, in points.
    fn text_size(mut self, size: impl Into<Length>) -> Self {
        self.style().lengths(&[StyleKey::FontSize], size.into());
        self
    }

    /// 12pt text, 16pt lines.
    fn text_xs(mut self) -> Self {
        self.style().push(Declaration::TextPreset("xs"));
        self
    }

    /// 14pt text, 20pt lines.
    fn text_sm(mut self) -> Self {
        self.style().push(Declaration::TextPreset("sm"));
        self
    }

    /// 16pt text, 24pt lines.
    fn text_base(mut self) -> Self {
        self.style().push(Declaration::TextPreset("base"));
        self
    }

    /// 18pt text, 28pt lines.
    fn text_lg(mut self) -> Self {
        self.style().push(Declaration::TextPreset("lg"));
        self
    }

    /// 20pt text, 28pt lines.
    fn text_xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("xl"));
        self
    }

    /// 24pt text, 32pt lines.
    fn text_2xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("2xl"));
        self
    }

    /// 30pt text, 36pt lines.
    fn text_3xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("3xl"));
        self
    }

    /// 36pt text, 40pt lines.
    fn text_4xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("4xl"));
        self
    }

    /// 48pt text, 48pt lines.
    fn text_5xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("5xl"));
        self
    }

    /// 60pt text, 60pt lines.
    fn text_6xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("6xl"));
        self
    }

    /// 72pt text, 72pt lines.
    fn text_7xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("7xl"));
        self
    }

    /// 96pt text, 96pt lines.
    fn text_8xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("8xl"));
        self
    }

    /// 128pt text, 128pt lines.
    fn text_9xl(mut self) -> Self {
        self.style().push(Declaration::TextPreset("9xl"));
        self
    }

    /// The line height, in points.
    fn line_height(mut self, height: impl Into<Length>) -> Self {
        self.style().lengths(&[StyleKey::LineHeight], height.into());
        self
    }

    /// The font weight.
    fn font_weight(mut self, weight: FontWeight) -> Self {
        self.style().number(StyleKey::FontWeight, weight.0 as f32);
        self
    }

    /// The font family (`None` style: the system font is the default).
    fn font_family(mut self, family: impl Into<SharedString>) -> Self {
        self.style()
            .push(Declaration::Str(StyleKey::FontFamily, family.into()));
        self
    }

    /// Italic text.
    fn italic(mut self) -> Self {
        self.style().keyword(StyleKey::FontStyle, "italic");
        self
    }

    /// Underlined text.
    fn underline(mut self) -> Self {
        self.style()
            .keyword(StyleKey::TextDecorationLine, "underline");
        self
    }

    /// Struck-through text.
    fn line_through(mut self) -> Self {
        self.style()
            .keyword(StyleKey::TextDecorationLine, "line-through");
        self
    }

    /// Text aligned left.
    fn text_left(mut self) -> Self {
        self.style().keyword(StyleKey::TextAlign, "left");
        self
    }

    /// Text centered.
    fn text_center(mut self) -> Self {
        self.style().keyword(StyleKey::TextAlign, "center");
        self
    }

    /// Text aligned right.
    fn text_right(mut self) -> Self {
        self.style().keyword(StyleKey::TextAlign, "right");
        self
    }

    /// Extra space between letters, in points.
    fn letter_spacing(mut self, points: f32) -> Self {
        self.style().number(StyleKey::LetterSpacing, points);
        self
    }
}

impl Styled for StyleRefinement {
    fn style(&mut self) -> &mut StyleRefinement {
        self
    }
}
