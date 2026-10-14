use std::sync::Arc;

use yf_anim::{AnimOp, AnimProp};

use crate::{
    geometry::{Rect, Size},
    ops::{EventMask, ImageSource, NodeId, ResizeMode},
    style::ViewStyle,
    text::{AttributedText, TextLayout},
};

/// Kind of native view to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    /// A plain container view.
    View,
    /// A text label.
    Text,
    /// An image.
    Image,
    /// A scroll view (also used by lists).
    Scroll,
}

/// One instruction for the platform backend.
///
/// Items must be applied in order: [`MountItem::Insert`] refers to views
/// created by the preceding items.
#[derive(Debug, Clone)]
pub enum MountItem {
    /// Create a native view. It starts detached and invisible.
    Create {
        /// Node id, used to address the view in later items.
        id: NodeId,
        /// View kind.
        kind: ViewKind,
    },
    /// Insert (or move) `child` into `parent`, below sibling `before` (or
    /// on top when `None`). Siblings rather than indices, so platform-owned
    /// subviews (refresh controls, scroll indicators) cannot shift positions.
    Insert {
        /// Parent view.
        parent: NodeId,
        /// Child view.
        child: NodeId,
        /// The app sibling to insert below.
        before: Option<NodeId>,
    },
    /// Remove a view from its superview, keeping it alive.
    Remove {
        /// View to detach.
        id: NodeId,
    },
    /// Destroy a view.
    Delete {
        /// View to destroy.
        id: NodeId,
    },
    /// Set the frame of a view, relative to its superview.
    Frame {
        /// View.
        id: NodeId,
        /// New frame.
        frame: Rect,
    },
    /// Set the visual style of a view.
    ViewStyle {
        /// View.
        id: NodeId,
        /// New style.
        style: ViewStyle,
    },
    /// Set the content of a text view.
    Text {
        /// Text view.
        id: NodeId,
        /// Content (for accessibility and debugging); shared, never copied.
        text: Arc<AttributedText>,
        /// The final layout, ready to draw.
        layout: TextLayout,
        /// Where `layout` goes inside the view (inside padding and border).
        content: Rect,
    },
    /// Set which events the view must report.
    Events {
        /// View.
        id: NodeId,
        /// Event set.
        events: EventMask,
    },
    /// Set the source of an image view.
    Image {
        /// Image view.
        id: NodeId,
        /// Source; `None` clears the image.
        source: Option<ImageSource>,
        /// Fit mode.
        resize_mode: ResizeMode,
    },
    /// Configure a scroll view.
    ScrollConfig {
        /// Scroll view.
        id: NodeId,
        /// Scrolls horizontally.
        horizontal: bool,
        /// Shows the scroll indicator.
        shows_indicator: bool,
        /// Native animation value fed with the main-axis offset.
        scroll_value: Option<yf_anim::NodeId>,
    },
    /// Set the scrollable content size.
    ScrollContent {
        /// Scroll view.
        id: NodeId,
        /// Content size.
        size: Size,
    },
    /// Shift the scroll position, keeping visible content in place when
    /// items above the viewport change size.
    ScrollBy {
        /// Scroll view.
        id: NodeId,
        /// Horizontal shift.
        dx: f32,
        /// Vertical shift.
        dy: f32,
    },
    /// Show or hide the pull-to-refresh spinner.
    Refreshing {
        /// Scroll view.
        id: NodeId,
        /// Spinner state.
        refreshing: bool,
    },
    /// Bind visual properties to native animation nodes.
    AnimBindings {
        /// View.
        id: NodeId,
        /// Property bindings (replaces the previous set).
        bindings: Vec<(AnimProp, yf_anim::NodeId)>,
    },
    /// Drive native animation values from a drag on the view.
    PanValues {
        /// View.
        id: NodeId,
        /// Horizontal value.
        x: Option<yf_anim::NodeId>,
        /// Vertical value.
        y: Option<yf_anim::NodeId>,
    },
    /// An animation graph change, in order with the other items.
    Anim(AnimOp),
}

/// All mount items produced by one commit.
#[derive(Debug, Default)]
pub struct MountBatch {
    /// Items, to be applied in order within one UI frame.
    pub items: Vec<MountItem>,
}
