use taffy::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, FlexWrap, LengthPercentage,
    LengthPercentageAuto, Position,
};

/// An sRGB color packed as `0xRRGGBBAA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color(pub u32);

impl Color {
    /// Fully transparent.
    pub const TRANSPARENT: Color = Color(0);
    /// Opaque black.
    pub const BLACK: Color = Color(0x0000_00ff);

    /// Creates a color from 8-bit components.
    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color((r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32)
    }

    /// Returns `(r, g, b, a)` in `0.0..=1.0`.
    #[inline]
    pub fn to_f64(self) -> (f64, f64, f64, f64) {
        let c = self.0;
        (
            (c >> 24) as f64 / 255.0,
            ((c >> 16) & 0xff) as f64 / 255.0,
            ((c >> 8) & 0xff) as f64 / 255.0,
            (c & 0xff) as f64 / 255.0,
        )
    }

    /// Returns true if the alpha component is zero.
    #[inline]
    pub const fn is_transparent(self) -> bool {
        self.0 & 0xff == 0
    }

    /// Parses a CSS-like color: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`,
    /// `rgb(...)`, `rgba(...)` or a small set of named colors.
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            return parse_hex(hex.as_bytes());
        }
        if let Some(args) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")) {
            return parse_rgb_args(args.strip_suffix(')')?);
        }
        Some(match s {
            "transparent" => Color::TRANSPARENT,
            "black" => Color::BLACK,
            "white" => Color(0xffff_ffff),
            "red" => Color(0xff00_00ff),
            "green" => Color(0x0080_00ff),
            "blue" => Color(0x0000_ffff),
            "yellow" => Color(0xffff_00ff),
            "orange" => Color(0xffa5_00ff),
            "purple" => Color(0x8000_80ff),
            "gray" | "grey" => Color(0x8080_80ff),
            _ => return None,
        })
    }
}

#[inline]
fn hex_digit(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u32),
        b'a'..=b'f' => Some((b - b'a' + 10) as u32),
        b'A'..=b'F' => Some((b - b'A' + 10) as u32),
        _ => None,
    }
}

fn parse_hex(hex: &[u8]) -> Option<Color> {
    let mut v = 0u32;
    for &b in hex {
        v = v << 4 | hex_digit(b)?;
    }
    Some(match hex.len() {
        3 | 4 => {
            // Expand each nibble: 0xabc -> 0xaabbcc.
            let (rgb, a) = if hex.len() == 3 {
                (v, 0xf)
            } else {
                (v >> 4, v & 0xf)
            };
            let r = (rgb >> 8) & 0xf;
            let g = (rgb >> 4) & 0xf;
            let b = rgb & 0xf;
            Color((r * 0x11) << 24 | (g * 0x11) << 16 | (b * 0x11) << 8 | (a * 0x11))
        }
        6 => Color(v << 8 | 0xff),
        8 => Color(v),
        _ => return None,
    })
}

fn parse_rgb_args(args: &str) -> Option<Color> {
    let mut it = args.split(',').map(str::trim);
    let channel =
        |s: Option<&str>| -> Option<u8> { Some(s?.parse::<f32>().ok()?.clamp(0.0, 255.0) as u8) };
    let r = channel(it.next())?;
    let g = channel(it.next())?;
    let b = channel(it.next())?;
    let a = match it.next() {
        Some(a) => (a.parse::<f32>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u8,
        None => 255,
    };
    if it.next().is_some() {
        return None;
    }
    Some(Color::rgba(r, g, b, a))
}

/// Horizontal alignment of a text block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Natural alignment for the writing direction.
    #[default]
    Auto,
    /// Left aligned.
    Left,
    /// Centered.
    Center,
    /// Right aligned.
    Right,
    /// Justified.
    Justify,
}

/// A 2D transform, applied around the view's center (React Native
/// semantics: translate, then scale, then rotate).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Horizontal translation in points.
    pub translate_x: f32,
    /// Vertical translation in points.
    pub translate_y: f32,
    /// Horizontal scale.
    pub scale_x: f32,
    /// Vertical scale.
    pub scale_y: f32,
    /// Rotation in degrees, clockwise.
    pub rotate: f32,
}

impl Transform {
    /// The identity transform.
    pub const IDENTITY: Transform = Transform {
        translate_x: 0.0,
        translate_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotate: 0.0,
    };

    /// True for the identity.
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Corner radii in points, clockwise from the top left.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CornerRadii {
    /// Top left.
    pub top_left: f32,
    /// Top right.
    pub top_right: f32,
    /// Bottom right.
    pub bottom_right: f32,
    /// Bottom left.
    pub bottom_left: f32,
}

impl CornerRadii {
    /// The same radius on every corner.
    pub const fn all(r: f32) -> Self {
        Self {
            top_left: r,
            top_right: r,
            bottom_right: r,
            bottom_left: r,
        }
    }

    /// `[top_left, top_right, bottom_right, bottom_left]`.
    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    /// True if no corner is rounded.
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.to_array().iter().all(|&r| r <= 0.0)
    }

    /// The radius shared by every rounded corner, if they all share one.
    /// Platforms render such shapes with a single radius plus a mask of
    /// the rounded corners (no paths).
    pub fn shared(&self) -> Option<f32> {
        let mut shared = 0.0;
        for r in self.to_array() {
            if r > 0.0 {
                if shared > 0.0 && r != shared {
                    return None;
                }
                shared = r;
            }
        }
        Some(shared)
    }

    /// Scales the radii down so adjacent corners do not overlap on a
    /// `width` x `height` box (CSS rule).
    pub fn fit(self, width: f32, height: f32) -> Self {
        let ratio = |side: f32, a: f32, b: f32| if a + b > side { side / (a + b) } else { 1.0 };
        let f = ratio(width, self.top_left, self.top_right)
            .min(ratio(width, self.bottom_left, self.bottom_right))
            .min(ratio(height, self.top_left, self.bottom_left))
            .min(ratio(height, self.top_right, self.bottom_right))
            .max(0.0);
        if f >= 1.0 {
            return self;
        }
        Self {
            top_left: self.top_left * f,
            top_right: self.top_right * f,
            bottom_right: self.bottom_right * f,
            bottom_left: self.bottom_left * f,
        }
    }

    /// The radii of a curve `d` points inside this one (e.g. the inner edge
    /// of a border).
    pub fn inset(self, d: f32) -> Self {
        Self {
            top_left: (self.top_left - d).max(0.0),
            top_right: (self.top_right - d).max(0.0),
            bottom_right: (self.bottom_right - d).max(0.0),
            bottom_left: (self.bottom_left - d).max(0.0),
        }
    }
}

/// A drop shadow (React Native's `shadow*` properties).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// Shadow color; its alpha is multiplied by `opacity`.
    pub color: Color,
    /// Horizontal offset in points.
    pub offset_x: f32,
    /// Vertical offset in points.
    pub offset_y: f32,
    /// Opacity in `0.0..=1.0`; 0 (the default) means no shadow.
    pub opacity: f32,
    /// Blur radius in points.
    pub radius: f32,
}

impl Shadow {
    /// React Native's defaults: black, offset `(0, -3)`, radius 3, opacity 0.
    pub const DEFAULT: Shadow = Shadow {
        color: Color::BLACK,
        offset_x: 0.0,
        offset_y: -3.0,
        opacity: 0.0,
        radius: 3.0,
    };

    /// True if the shadow draws anything.
    #[inline]
    pub fn is_visible(&self) -> bool {
        self.opacity > 0.0 && !self.color.is_transparent()
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Visual (non-layout) properties of a native view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewStyle {
    /// Background fill.
    pub background_color: Color,
    /// `borderRadius`: the radius of corners without their own.
    pub border_radius: f32,
    /// `borderTopLeftRadius` etc., clockwise from the top left; they
    /// override `border_radius`.
    pub corner_radius: [Option<f32>; 4],
    /// Border width in points (also affects layout).
    pub border_width: f32,
    /// Border color.
    pub border_color: Color,
    /// Opacity in `0.0..=1.0`.
    pub opacity: f32,
    /// Clip children to the (rounded) bounds.
    pub overflow_hidden: bool,
    /// Drop shadow.
    pub shadow: Shadow,
    /// Static transform.
    pub transform: Transform,
}

impl ViewStyle {
    /// The radius of each corner (before fitting them to a size with
    /// [`CornerRadii::fit`]).
    pub fn radii(&self) -> CornerRadii {
        let [tl, tr, br, bl] = self
            .corner_radius
            .map(|r| r.unwrap_or(self.border_radius).max(0.0));
        CornerRadii {
            top_left: tl,
            top_right: tr,
            bottom_right: br,
            bottom_left: bl,
        }
    }
}

impl Default for ViewStyle {
    fn default() -> Self {
        Self {
            background_color: Color::TRANSPARENT,
            border_radius: 0.0,
            corner_radius: [None; 4],
            border_width: 0.0,
            border_color: Color::BLACK,
            opacity: 1.0,
            overflow_hidden: false,
            shadow: Shadow::DEFAULT,
            transform: Transform::IDENTITY,
        }
    }
}

/// Text properties as written on a node; unset values are inherited from
/// the enclosing text.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextStyle {
    /// Foreground color.
    pub color: Option<Color>,
    /// Font size in points.
    pub font_size: Option<f32>,
    /// Font weight, `100..=900`.
    pub font_weight: Option<u16>,
    /// Line height in points.
    pub line_height: Option<f32>,
    /// Block alignment (only meaningful on a text block).
    pub text_align: Option<TextAlign>,
}

/// A complete node style, split by consumer.
#[derive(Debug, Clone)]
pub struct Style {
    /// Layout properties, consumed by taffy.
    pub layout: taffy::Style,
    /// Visual properties of the native view.
    pub view: ViewStyle,
    /// Text properties.
    pub text: TextStyle,
    /// Visual properties driven by native animation nodes; they override
    /// the static values while bound.
    pub animated: Vec<(yf_anim::AnimProp, yf_anim::NodeId)>,
}

impl Default for Style {
    /// React Native defaults, which differ from CSS (and taffy) defaults.
    fn default() -> Self {
        let layout = taffy::Style {
            display: Display::Flex,
            position: Position::Relative,
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            align_content: AlignContent::FLEX_START,
            ..taffy::Style::DEFAULT
        };
        Self {
            layout,
            view: ViewStyle::default(),
            text: TextStyle::default(),
            animated: Vec::new(),
        }
    }
}

/// A style property value as read from JS.
#[derive(Debug, Clone, Copy)]
pub enum StyleValue<'a> {
    /// A number.
    Number(f32),
    /// A string.
    Str(&'a str),
    /// `null` / `undefined`: resets the property to its default.
    Null,
}

/// Why a style property was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleError {
    /// The property name is not supported.
    UnknownProperty,
    /// The value has the wrong type or format.
    InvalidValue,
}

enum Len {
    Px(f32),
    Pct(f32),
    Auto,
}

impl Len {
    fn parse(v: StyleValue<'_>) -> Result<Len, StyleError> {
        match v {
            StyleValue::Number(n) => Ok(Len::Px(n)),
            StyleValue::Str("auto") => Ok(Len::Auto),
            StyleValue::Str(s) => s
                .strip_suffix('%')
                .and_then(|p| p.trim().parse::<f32>().ok())
                .map(|p| Len::Pct(p / 100.0))
                .ok_or(StyleError::InvalidValue),
            StyleValue::Null => Ok(Len::Auto),
        }
    }

    fn dim(self) -> Dimension {
        match self {
            Len::Px(v) => Dimension::length(v),
            Len::Pct(v) => Dimension::percent(v),
            Len::Auto => Dimension::auto(),
        }
    }

    fn lpa(self) -> LengthPercentageAuto {
        match self {
            Len::Px(v) => LengthPercentageAuto::length(v),
            Len::Pct(v) => LengthPercentageAuto::percent(v),
            Len::Auto => LengthPercentageAuto::auto(),
        }
    }

    fn lp(self) -> Result<LengthPercentage, StyleError> {
        match self {
            Len::Px(v) => Ok(LengthPercentage::length(v)),
            Len::Pct(v) => Ok(LengthPercentage::percent(v)),
            Len::Auto => Err(StyleError::InvalidValue),
        }
    }
}

/// A zero-default length: `null` resets to 0 rather than `auto`.
fn lp_or_zero(v: StyleValue<'_>) -> Result<LengthPercentage, StyleError> {
    match v {
        StyleValue::Null => Ok(LengthPercentage::length(0.0)),
        v => Len::parse(v)?.lp(),
    }
}

fn lpa_or_zero(v: StyleValue<'_>) -> Result<LengthPercentageAuto, StyleError> {
    match v {
        StyleValue::Null => Ok(LengthPercentageAuto::length(0.0)),
        v => Ok(Len::parse(v)?.lpa()),
    }
}

fn number(v: StyleValue<'_>, default: f32) -> Result<f32, StyleError> {
    match v {
        StyleValue::Number(n) => Ok(n),
        StyleValue::Null => Ok(default),
        StyleValue::Str(_) => Err(StyleError::InvalidValue),
    }
}

fn opt_number(v: StyleValue<'_>) -> Result<Option<f32>, StyleError> {
    match v {
        StyleValue::Number(n) => Ok(Some(n)),
        StyleValue::Null => Ok(None),
        StyleValue::Str(_) => Err(StyleError::InvalidValue),
    }
}

fn color(v: StyleValue<'_>) -> Result<Option<Color>, StyleError> {
    match v {
        StyleValue::Str(s) => Color::parse(s).map(Some).ok_or(StyleError::InvalidValue),
        StyleValue::Null => Ok(None),
        StyleValue::Number(_) => Err(StyleError::InvalidValue),
    }
}

fn keyword<'a>(v: StyleValue<'a>) -> Result<Option<&'a str>, StyleError> {
    match v {
        StyleValue::Str(s) => Ok(Some(s)),
        StyleValue::Null => Ok(None),
        StyleValue::Number(_) => Err(StyleError::InvalidValue),
    }
}

fn align_items(v: StyleValue<'_>) -> Result<Option<AlignItems>, StyleError> {
    Ok(match keyword(v)? {
        None => None,
        Some("flex-start") => Some(AlignItems::FLEX_START),
        Some("flex-end") => Some(AlignItems::FLEX_END),
        Some("center") => Some(AlignItems::CENTER),
        Some("stretch") => Some(AlignItems::STRETCH),
        Some("baseline") => Some(AlignItems::BASELINE),
        Some(_) => return Err(StyleError::InvalidValue),
    })
}

fn align_content(v: StyleValue<'_>, default: AlignContent) -> Result<AlignContent, StyleError> {
    Ok(match keyword(v)? {
        None => default,
        Some("flex-start") => AlignContent::FLEX_START,
        Some("flex-end") => AlignContent::FLEX_END,
        Some("center") => AlignContent::CENTER,
        Some("stretch") => AlignContent::STRETCH,
        Some("space-between") => AlignContent::SPACE_BETWEEN,
        Some("space-around") => AlignContent::SPACE_AROUND,
        Some("space-evenly") => AlignContent::SPACE_EVENLY,
        Some(_) => return Err(StyleError::InvalidValue),
    })
}

fn font_weight(v: StyleValue<'_>) -> Result<Option<u16>, StyleError> {
    Ok(match v {
        StyleValue::Null => None,
        StyleValue::Number(n) => Some((n as u16).clamp(100, 900)),
        StyleValue::Str("normal") => Some(400),
        StyleValue::Str("bold") => Some(700),
        StyleValue::Str(s) => Some(
            s.parse::<u16>()
                .map_err(|_| StyleError::InvalidValue)?
                .clamp(100, 900),
        ),
    })
}

impl Style {
    /// Sets one entry of the `transform` array (`translateX`, `scale`, ...).
    /// Rotations are degrees; `"45deg"` strings are parsed by the caller.
    pub fn set_transform(&mut self, key: &str, value: f32) -> Result<(), StyleError> {
        let t = &mut self.view.transform;
        match key {
            "translateX" => t.translate_x = value,
            "translateY" => t.translate_y = value,
            "scale" => (t.scale_x, t.scale_y) = (value, value),
            "scaleX" => t.scale_x = value,
            "scaleY" => t.scale_y = value,
            "rotate" | "rotateZ" => t.rotate = value,
            _ => return Err(StyleError::UnknownProperty),
        }
        Ok(())
    }

    /// Sets `shadowOffset: { width, height }`; `None` restores the default.
    pub fn set_shadow_offset(&mut self, offset: Option<(f32, f32)>) {
        let (x, y) = offset.unwrap_or((Shadow::DEFAULT.offset_x, Shadow::DEFAULT.offset_y));
        self.view.shadow.offset_x = x;
        self.view.shadow.offset_y = y;
    }

    /// Sets one property using its React Native name.
    pub fn set(&mut self, key: &str, value: StyleValue<'_>) -> Result<(), StyleError> {
        let l = &mut self.layout;
        match key {
            // Layout
            "display" => {
                l.display = match keyword(value)? {
                    None | Some("flex") => Display::Flex,
                    Some("none") => Display::None,
                    Some(_) => return Err(StyleError::InvalidValue),
                }
            }
            "position" => {
                l.position = match keyword(value)? {
                    None | Some("relative") => Position::Relative,
                    Some("absolute") => Position::Absolute,
                    Some(_) => return Err(StyleError::InvalidValue),
                }
            }
            "flexDirection" => {
                l.flex_direction = match keyword(value)? {
                    None | Some("column") => FlexDirection::Column,
                    Some("row") => FlexDirection::Row,
                    Some("column-reverse") => FlexDirection::ColumnReverse,
                    Some("row-reverse") => FlexDirection::RowReverse,
                    Some(_) => return Err(StyleError::InvalidValue),
                }
            }
            "flexWrap" => {
                l.flex_wrap = match keyword(value)? {
                    None | Some("nowrap") => FlexWrap::NoWrap,
                    Some("wrap") => FlexWrap::Wrap,
                    Some("wrap-reverse") => FlexWrap::WrapReverse,
                    Some(_) => return Err(StyleError::InvalidValue),
                }
            }
            "flex" => {
                // React Native semantics, not the CSS shorthand.
                let f = number(value, 0.0)?;
                if f > 0.0 {
                    l.flex_grow = f;
                    l.flex_shrink = 1.0;
                    l.flex_basis = Dimension::length(0.0);
                } else if f < 0.0 {
                    l.flex_grow = 0.0;
                    l.flex_shrink = -f;
                    l.flex_basis = Dimension::auto();
                } else {
                    l.flex_grow = 0.0;
                    l.flex_shrink = 0.0;
                    l.flex_basis = Dimension::auto();
                }
            }
            "flexGrow" => l.flex_grow = number(value, 0.0)?,
            "flexShrink" => l.flex_shrink = number(value, 0.0)?,
            "flexBasis" => l.flex_basis = Len::parse(value)?.dim(),
            "justifyContent" => l.justify_content = align_content(value, AlignContent::FLEX_START)?,
            "alignItems" => l.align_items = align_items(value)?.unwrap_or(AlignItems::STRETCH),
            "alignSelf" => {
                l.align_self = match value {
                    StyleValue::Str("auto") => None,
                    v => align_items(v)?,
                }
            }
            "alignContent" => l.align_content = align_content(value, AlignContent::FLEX_START)?,
            "width" => l.size.width = Len::parse(value)?.dim(),
            "height" => l.size.height = Len::parse(value)?.dim(),
            "minWidth" => l.min_size.width = Len::parse(value)?.lpa(),
            "minHeight" => l.min_size.height = Len::parse(value)?.lpa(),
            "maxWidth" => l.max_size.width = Len::parse(value)?.lpa(),
            "maxHeight" => l.max_size.height = Len::parse(value)?.lpa(),
            "aspectRatio" => l.aspect_ratio = opt_number(value)?,
            "top" => l.inset.top = Len::parse(value)?.lpa(),
            "right" => l.inset.right = Len::parse(value)?.lpa(),
            "bottom" => l.inset.bottom = Len::parse(value)?.lpa(),
            "left" => l.inset.left = Len::parse(value)?.lpa(),
            "margin" => {
                let v = lpa_or_zero(value)?;
                l.margin = taffy::Rect {
                    left: v,
                    right: v,
                    top: v,
                    bottom: v,
                };
            }
            "marginHorizontal" => {
                let v = lpa_or_zero(value)?;
                (l.margin.left, l.margin.right) = (v, v);
            }
            "marginVertical" => {
                let v = lpa_or_zero(value)?;
                (l.margin.top, l.margin.bottom) = (v, v);
            }
            "marginTop" => l.margin.top = lpa_or_zero(value)?,
            "marginRight" => l.margin.right = lpa_or_zero(value)?,
            "marginBottom" => l.margin.bottom = lpa_or_zero(value)?,
            "marginLeft" => l.margin.left = lpa_or_zero(value)?,
            "padding" => {
                let v = lp_or_zero(value)?;
                l.padding = taffy::Rect {
                    left: v,
                    right: v,
                    top: v,
                    bottom: v,
                };
            }
            "paddingHorizontal" => {
                let v = lp_or_zero(value)?;
                (l.padding.left, l.padding.right) = (v, v);
            }
            "paddingVertical" => {
                let v = lp_or_zero(value)?;
                (l.padding.top, l.padding.bottom) = (v, v);
            }
            "paddingTop" => l.padding.top = lp_or_zero(value)?,
            "paddingRight" => l.padding.right = lp_or_zero(value)?,
            "paddingBottom" => l.padding.bottom = lp_or_zero(value)?,
            "paddingLeft" => l.padding.left = lp_or_zero(value)?,
            "gap" => {
                let v = lp_or_zero(value)?;
                l.gap = taffy::Size {
                    width: v,
                    height: v,
                };
            }
            "rowGap" => l.gap.height = lp_or_zero(value)?,
            "columnGap" => l.gap.width = lp_or_zero(value)?,
            "borderWidth" => {
                let w = number(value, 0.0)?;
                let v = LengthPercentage::length(w);
                l.border = taffy::Rect {
                    left: v,
                    right: v,
                    top: v,
                    bottom: v,
                };
                self.view.border_width = w;
            }

            // View
            "backgroundColor" => {
                self.view.background_color = color(value)?.unwrap_or(Color::TRANSPARENT)
            }
            "borderRadius" => self.view.border_radius = number(value, 0.0)?,
            "borderTopLeftRadius" => self.view.corner_radius[0] = opt_number(value)?,
            "borderTopRightRadius" => self.view.corner_radius[1] = opt_number(value)?,
            "borderBottomRightRadius" => self.view.corner_radius[2] = opt_number(value)?,
            "borderBottomLeftRadius" => self.view.corner_radius[3] = opt_number(value)?,
            "shadowColor" => {
                self.view.shadow.color = color(value)?.unwrap_or(Shadow::DEFAULT.color)
            }
            "shadowOpacity" => {
                self.view.shadow.opacity = number(value, Shadow::DEFAULT.opacity)?.clamp(0.0, 1.0)
            }
            "shadowRadius" => {
                self.view.shadow.radius = number(value, Shadow::DEFAULT.radius)?.max(0.0)
            }
            "borderColor" => self.view.border_color = color(value)?.unwrap_or(Color::BLACK),
            "opacity" => self.view.opacity = number(value, 1.0)?.clamp(0.0, 1.0),
            "overflow" => {
                self.view.overflow_hidden = match keyword(value)? {
                    None | Some("visible") => false,
                    Some("hidden") => true,
                    Some(_) => return Err(StyleError::InvalidValue),
                }
            }

            // Text
            "color" => self.text.color = color(value)?,
            "fontSize" => self.text.font_size = opt_number(value)?,
            "fontWeight" => self.text.font_weight = font_weight(value)?,
            "lineHeight" => self.text.line_height = opt_number(value)?,
            "textAlign" => {
                self.text.text_align = match keyword(value)? {
                    None => None,
                    Some("auto") => Some(TextAlign::Auto),
                    Some("left") => Some(TextAlign::Left),
                    Some("center") => Some(TextAlign::Center),
                    Some("right") => Some(TextAlign::Right),
                    Some("justify") => Some(TextAlign::Justify),
                    Some(_) => return Err(StyleError::InvalidValue),
                }
            }
            _ => return Err(StyleError::UnknownProperty),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_colors() {
        assert_eq!(Color::parse("#fff"), Some(Color(0xffff_ffff)));
        assert_eq!(Color::parse("#f008"), Some(Color(0xff00_0088)));
        assert_eq!(Color::parse("#123456"), Some(Color(0x1234_56ff)));
        assert_eq!(Color::parse("#12345678"), Some(Color(0x1234_5678)));
        assert_eq!(Color::parse("rgb(255, 0, 10)"), Some(Color(0xff00_0aff)));
        assert_eq!(Color::parse("rgba(0,0,0,0.5)"), Some(Color(0x0000_0080)));
        assert_eq!(Color::parse("white"), Some(Color(0xffff_ffff)));
        assert_eq!(Color::parse("#12"), None);
        assert_eq!(Color::parse("nope"), None);
    }

    #[test]
    fn react_native_defaults() {
        let s = Style::default();
        assert_eq!(s.layout.flex_direction, FlexDirection::Column);
        assert_eq!(s.layout.flex_shrink, 0.0);
        assert_eq!(s.layout.position, Position::Relative);
        assert_eq!(s.layout.align_content, AlignContent::FLEX_START);
    }

    #[test]
    fn flex_shorthand() {
        let mut s = Style::default();
        s.set("flex", StyleValue::Number(2.0)).unwrap();
        assert_eq!(s.layout.flex_grow, 2.0);
        assert_eq!(s.layout.flex_shrink, 1.0);
        assert_eq!(s.layout.flex_basis, Dimension::length(0.0));
    }

    #[test]
    fn corner_radii_override_the_shorthand_in_any_order() {
        let mut s = Style::default();
        s.set("borderTopLeftRadius", StyleValue::Number(4.0))
            .unwrap();
        s.set("borderRadius", StyleValue::Number(10.0)).unwrap();
        let r = s.view.radii();
        assert_eq!(r.to_array(), [4.0, 10.0, 10.0, 10.0]);
        assert_eq!(r.shared(), None);
        s.set("borderTopLeftRadius", StyleValue::Null).unwrap();
        assert_eq!(s.view.radii().shared(), Some(10.0));
    }

    #[test]
    fn shared_radius_ignores_square_corners() {
        let r = CornerRadii {
            top_left: 12.0,
            top_right: 12.0,
            ..CornerRadii::default()
        };
        assert_eq!(r.shared(), Some(12.0));
        assert_eq!(CornerRadii::default().shared(), Some(0.0));
        assert!(CornerRadii::default().is_zero());
    }

    #[test]
    fn radii_fit_the_box() {
        // 100 + 300 on a 200 wide box: everything scales by 0.5.
        let r = CornerRadii {
            top_left: 100.0,
            top_right: 300.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
        .fit(200.0, 1000.0);
        assert_eq!(r.to_array(), [50.0, 150.0, 0.0, 0.0]);
        let fits = CornerRadii::all(10.0);
        assert_eq!(fits.fit(100.0, 100.0), fits);
        assert_eq!(fits.inset(4.0), CornerRadii::all(6.0));
        assert_eq!(fits.inset(40.0), CornerRadii::all(0.0));
    }

    #[test]
    fn shadow_defaults_follow_react_native() {
        let mut s = Style::default();
        assert!(!s.view.shadow.is_visible());
        s.set("shadowOpacity", StyleValue::Number(0.3)).unwrap();
        s.set("shadowRadius", StyleValue::Number(8.0)).unwrap();
        s.set_shadow_offset(Some((0.0, 4.0)));
        assert!(s.view.shadow.is_visible());
        assert_eq!(s.view.shadow.offset_y, 4.0);
        s.set("shadowColor", StyleValue::Str("transparent"))
            .unwrap();
        assert!(!s.view.shadow.is_visible());
        s.set_shadow_offset(None);
        assert_eq!(s.view.shadow.offset_y, -3.0);
    }

    #[test]
    fn rejects_bad_values() {
        let mut s = Style::default();
        assert_eq!(
            s.set("width", StyleValue::Str("wide")),
            Err(StyleError::InvalidValue)
        );
        assert_eq!(
            s.set("nope", StyleValue::Number(1.0)),
            Err(StyleError::UnknownProperty)
        );
        s.set("width", StyleValue::Str("50%")).unwrap();
        assert_eq!(s.layout.size.width, Dimension::percent(0.5));
    }
}
