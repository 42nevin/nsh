/// A size in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    /// Width in points.
    pub width: f32,
    /// Height in points.
    pub height: f32,
}

impl Size {
    /// Creates a size.
    #[inline]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A rectangle in points, relative to the parent view.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// A rectangle that never compares equal to a real frame; used to force
    /// the first frame of a newly created view to be emitted.
    pub(crate) const UNSET: Rect = Rect {
        x: f32::NAN,
        y: f32::NAN,
        width: f32::NAN,
        height: f32::NAN,
    };
}
