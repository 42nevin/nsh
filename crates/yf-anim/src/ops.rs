/// Identifier of an animation node, assigned by JS.
pub type NodeId = u32;

/// Identifier of one started animation, assigned by JS; reported back when
/// the animation finishes.
pub type AnimId = u32;

/// A view property that an animation node can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimProp {
    /// `opacity`, 0..1.
    Opacity,
    /// `transform: [{ translateX }]`, points.
    TranslateX,
    /// `transform: [{ translateY }]`, points.
    TranslateY,
    /// `transform: [{ scale }]`.
    Scale,
    /// `transform: [{ scaleX }]`.
    ScaleX,
    /// `transform: [{ scaleY }]`.
    ScaleY,
    /// `transform: [{ rotate }]`, degrees.
    Rotate,
    /// `backgroundColor`, from a color interpolation.
    BackgroundColor,
}

impl AnimProp {
    /// Parses a style key (or transform key).
    pub fn from_name(name: &str) -> Option<AnimProp> {
        Some(match name {
            "opacity" => AnimProp::Opacity,
            "translateX" => AnimProp::TranslateX,
            "translateY" => AnimProp::TranslateY,
            "scale" => AnimProp::Scale,
            "scaleX" => AnimProp::ScaleX,
            "scaleY" => AnimProp::ScaleY,
            "rotate" => AnimProp::Rotate,
            "backgroundColor" => AnimProp::BackgroundColor,
            _ => return None,
        })
    }
}

/// Easing curve of a timing animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// CSS `ease-in`.
    EaseIn,
    /// CSS `ease-out`.
    EaseOut,
    /// CSS `ease-in-out`.
    EaseInOut,
    /// A cubic Bézier `(x1, y1, x2, y2)`, like CSS `cubic-bezier()`.
    Bezier(f32, f32, f32, f32),
}

/// How an animation moves a value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Driver {
    /// Fixed duration along an easing curve.
    Timing {
        /// Target value.
        to: f32,
        /// Duration in milliseconds.
        duration_ms: f32,
        /// Curve.
        easing: Easing,
        /// Delay before starting, in milliseconds.
        delay_ms: f32,
    },
    /// Damped spring.
    Spring {
        /// Target value.
        to: f32,
        /// Spring stiffness `k`.
        stiffness: f32,
        /// Damping `c`.
        damping: f32,
        /// Mass `m`.
        mass: f32,
        /// Initial velocity (units per second); `None` keeps the current one.
        velocity: Option<f32>,
    },
    /// Exponential slow-down from an initial velocity (flings).
    Decay {
        /// Initial velocity (units per second); `None` keeps the current one.
        velocity: Option<f32>,
        /// Fraction of velocity kept per millisecond, e.g. `0.998`.
        deceleration: f32,
    },
}

/// Output range of an interpolation.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// Numbers.
    Numbers(Vec<f32>),
    /// Colors packed as `0xRRGGBBAA` (same as `yf_core::Color`).
    Colors(Vec<u32>),
}

/// What an interpolation does outside its input range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extrapolate {
    /// Continue the first / last segment.
    Extend,
    /// Stick to the first / last output.
    Clamp,
    /// Return the input unchanged.
    Identity,
}

/// Binary operation of a math node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathOp {
    /// `a + b`.
    Add,
    /// `a - b`.
    Sub,
    /// `a * b`.
    Mul,
    /// `a / b` (0 when `b` is 0).
    Div,
}

/// An operand of a math node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operand {
    /// Another node.
    Node(NodeId),
    /// A constant.
    Const(f32),
}

/// A change to the animation graph, sent by JS.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimOp {
    /// A settable, animatable value.
    CreateValue {
        /// New node.
        node: NodeId,
        /// Initial value.
        value: f32,
    },
    /// Maps `input` through piecewise-linear ranges.
    CreateInterpolate {
        /// New node.
        node: NodeId,
        /// Source node.
        input: NodeId,
        /// Increasing input breakpoints.
        input_range: Vec<f32>,
        /// Outputs, same length as `input_range`.
        output: Output,
        /// Behaviour outside the input range.
        extrapolate: Extrapolate,
    },
    /// Combines two operands.
    CreateMath {
        /// New node.
        node: NodeId,
        /// Operation.
        op: MathOp,
        /// Left operand.
        a: Operand,
        /// Right operand.
        b: Operand,
    },
    /// Sets a value immediately, stopping its animation.
    Set {
        /// Value node.
        node: NodeId,
        /// New value.
        value: f32,
    },
    /// Starts an animation on a value, replacing any running one.
    Start {
        /// Value node.
        node: NodeId,
        /// Reported back on completion.
        animation: AnimId,
        /// How to move.
        driver: Driver,
    },
    /// Stops the animation of a value where it is.
    Stop {
        /// Value node.
        node: NodeId,
    },
    /// Frees a node.
    Release {
        /// Node.
        node: NodeId,
    },
}
