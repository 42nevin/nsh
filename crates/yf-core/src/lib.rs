//! Platform-independent core of yf.
//!
//! The core receives [`Op`]s from the JS renderer, maintains the shadow tree,
//! converts styles to [`taffy`] layout, merges inline text into
//! [`AttributedText`], runs layout and finally emits a [`MountBatch`] of
//! [`MountItem`]s that a platform backend applies to native views.
//!
//! Everything here is single-threaded and allocation-conscious: the core runs
//! on the engine thread, between the JS host and the platform backend.

mod core;
mod fenwick;
mod geometry;
mod mount;
mod ops;
mod style;
mod text;

pub use yf_anim;

pub use crate::{
    core::{ApplyError, Core, ListBind},
    geometry::{Rect, Size},
    mount::{MountBatch, MountItem, ViewKind},
    ops::{
        EventKind, EventMask, EventPayload, ImageSource, NodeId, Op, Prop, ROOT_ID, ResizeMode, Tag,
    },
    style::{
        Color, CornerRadii, Shadow, Style, StyleError, StyleValue, TextAlign, TextStyle, Transform,
        ViewStyle,
    },
    text::{ApproxTextEngine, AttributedText, ResolvedTextStyle, TextEngine, TextLayout, TextSpan},
};
