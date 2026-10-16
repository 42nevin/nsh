//! What components read from the environment: the surface, system UI, the
//! user's settings ([`super::RenderCx::window`] and the like).

use crate::{EdgeInsets, Size};

/// The environment of a surface, as the platform reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// The surface's size in points.
    pub window: Size,
    /// The edges system UI covers.
    pub safe_area: EdgeInsets,
    /// The software keyboard's height, 0 when hidden.
    pub keyboard_height: f32,
    /// How long the keyboard's show / hide animation takes, in seconds.
    pub keyboard_duration: f32,
    /// Dark appearance.
    pub dark: bool,
    /// The user's text size relative to the default.
    pub font_scale: f32,
    /// Less motion asked for.
    pub reduce_motion: bool,
    /// A screen reader runs.
    pub screen_reader: bool,
    /// Right-to-left layout.
    pub rtl: bool,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            window: Size::new(0.0, 0.0),
            safe_area: EdgeInsets::default(),
            keyboard_height: 0.0,
            keyboard_duration: 0.0,
            dark: false,
            font_scale: 1.0,
            reduce_motion: false,
            screen_reader: false,
            rtl: false,
        }
    }
}

// What a render read, to render again when it changes.
pub(crate) const WINDOW: u16 = 1;
pub(crate) const SAFE_AREA: u16 = 2;
pub(crate) const KEYBOARD: u16 = 4;
pub(crate) const APPEARANCE: u16 = 8;
pub(crate) const FONT_SCALE: u16 = 16;
pub(crate) const MOTION: u16 = 32;
pub(crate) const SCREEN_READER: u16 = 64;
pub(crate) const RTL: u16 = 128;
/// The insets of the screen or modal a component renders in.
pub(crate) const INSETS: u16 = 256;

impl Environment {
    /// The parts that differ from `old`.
    pub(crate) fn changes(&self, old: &Environment) -> u16 {
        let mut changed = 0;
        let mut flag = |bit, differs: bool| {
            if differs {
                changed |= bit;
            }
        };
        flag(WINDOW, self.window != old.window);
        flag(SAFE_AREA, self.safe_area != old.safe_area);
        flag(KEYBOARD, self.keyboard_height != old.keyboard_height);
        flag(APPEARANCE, self.dark != old.dark);
        flag(FONT_SCALE, self.font_scale != old.font_scale);
        flag(MOTION, self.reduce_motion != old.reduce_motion);
        flag(SCREEN_READER, self.screen_reader != old.screen_reader);
        flag(RTL, self.rtl != old.rtl);
        changed
    }
}
