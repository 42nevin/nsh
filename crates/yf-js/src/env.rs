//! Facts about the surface that JS reads reactively (`safeArea()`,
//! `keyboard()`, `colorScheme()`...).

/// Edge insets in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    /// Top.
    pub top: f32,
    /// Right.
    pub right: f32,
    /// Bottom.
    pub bottom: f32,
    /// Left.
    pub left: f32,
}

/// The system appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    /// Light mode.
    #[default]
    Light,
    /// Dark mode.
    Dark,
}

/// The kind of device (React Native's `Platform.isPad`, `isTV`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Idiom {
    /// A phone.
    #[default]
    Phone,
    /// A tablet.
    Pad,
    /// A Mac (Mac Catalyst).
    Mac,
    /// A TV.
    Tv,
}

/// What the platform knows about the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Size of the surface in points.
    pub window: (f32, f32),
    /// Insets not covered by system UI (notch, home indicator, title bar).
    pub safe_area: Insets,
    /// Height of the software keyboard over the surface; 0 when hidden.
    pub keyboard_height: f32,
    /// Duration of the keyboard's show / hide animation in seconds.
    pub keyboard_duration: f32,
    /// Light or dark appearance.
    pub color_scheme: ColorScheme,
    /// Preferred text size relative to the default (1.0).
    pub font_scale: f32,
    /// The user's language is written right to left.
    pub rtl: bool,
    /// The user asked for less motion (iOS Reduce Motion, Android's
    /// animator duration scale at 0).
    pub reduce_motion: bool,
    /// Device pixels per point.
    pub pixel_ratio: f32,
    /// A screen reader runs (VoiceOver, TalkBack).
    pub screen_reader: bool,
    /// The OS version: iOS major and minor, Android's API level and 0.
    pub os_version: (u16, u16),
    /// The kind of device.
    pub idiom: Idiom,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            window: (0.0, 0.0),
            safe_area: Insets::default(),
            keyboard_height: 0.0,
            keyboard_duration: 0.25,
            color_scheme: ColorScheme::Light,
            font_scale: 1.0,
            rtl: false,
            reduce_motion: false,
            pixel_ratio: 1.0,
            screen_reader: false,
            os_version: (0, 0),
            idiom: Idiom::Phone,
        }
    }
}
