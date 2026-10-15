//! Native animation graph for yf.
//!
//! JS declares values, derived nodes (interpolations, math) and animations
//! once; the platform evaluates them every display frame on the UI thread,
//! so animations, gestures and scroll-linked effects never wait for JS.
//!
//! The crate is platform-free: the platform feeds it [`AnimOp`]s, frame
//! timestamps, gesture and scroll values, and reads back evaluated values.

mod driver;
mod graph;
mod ops;

pub use crate::{
    graph::{AnimValue, Finished, Graph},
    ops::{
        AnimId, AnimOp, AnimProp, Composite, Curve, Driver, EaseMode, Easing, Extrapolate, MathOp,
        NodeId, Operand, Output,
    },
};
