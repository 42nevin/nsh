use std::sync::Arc;

use yf_anim::AnimOp;

use crate::style::Style;

/// Identifier of a node, assigned by the JS renderer.
///
/// Ids are small, dense and increasing, so the core indexes them directly.
pub type NodeId = u32;

/// Id of the surface root view. It exists before any JS runs.
pub const ROOT_ID: NodeId = 1;

/// Element tag of a host node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    /// A container view (`<view>`).
    View,
    /// A text block, or a span when nested in another text (`<text>`).
    Text,
    /// An image (`<image>`).
    Image,
    /// A scrolling container (`<scroll>`).
    Scroll,
    /// A virtualized list (`<list>`, created by the runtime's `List`).
    List,
    /// A recycled list cell (created by the runtime's `List`).
    Cell,
}

impl Tag {
    /// Parses a JSX intrinsic element name.
    #[inline]
    pub fn from_name(name: &str) -> Option<Tag> {
        Some(match name {
            "view" => Tag::View,
            "text" => Tag::Text,
            "image" => Tag::Image,
            "scroll" => Tag::Scroll,
            "list" => Tag::List,
            "cell" => Tag::Cell,
            _ => return None,
        })
    }
}

macro_rules! events {
    ($($variant:ident = $prop:literal: $doc:literal,)*) => {
        /// An event a node can listen to.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum EventKind {
            $(#[doc = $doc] $variant,)*
        }

        impl EventKind {
            /// Parses an event prop name such as `onPress`.
            #[inline]
            pub fn from_prop(name: &str) -> Option<EventKind> {
                match name {
                    $($prop => Some(EventKind::$variant),)*
                    _ => None,
                }
            }

            /// The JS prop name of this event.
            #[inline]
            pub const fn prop_name(self) -> &'static str {
                match self {
                    $(EventKind::$variant => $prop,)*
                }
            }
        }
    };
}

events! {
    Press = "onPress": "A tap / click.",
    LongPress = "onLongPress": "A press held for about half a second.",
    PanStart = "onPanStart": "A drag started (payload: [`EventPayload::Pan`]).",
    PanEnd = "onPanEnd": "A drag ended (payload: [`EventPayload::Pan`] with velocity).",
    Refresh = "onRefresh": "Pull-to-refresh was triggered on a scroll or list.",
    EndReached = "onEndReached": "A list was scrolled close to its end.",
    ScrollEnd = "onScrollEnd": "Scrolling came to rest (payload: [`EventPayload::Scroll`]).",
    Load = "onLoad": "An image loaded (payload: [`EventPayload::Image`]).",
    Error = "onError": "An image failed to load (payload: [`EventPayload::Error`]).",
}

/// A set of [`EventKind`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventMask(u16);

impl EventMask {
    /// Returns true if `event` is in the set.
    #[inline]
    pub const fn contains(self, event: EventKind) -> bool {
        self.0 & (1 << event as u8) != 0
    }

    /// Adds or removes `event`.
    #[inline]
    pub fn set(&mut self, event: EventKind, enabled: bool) {
        if enabled {
            self.0 |= 1 << event as u8;
        } else {
            self.0 &= !(1 << event as u8);
        }
    }

    /// Returns true if no event is set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Data delivered with an event.
#[derive(Debug, Clone, PartialEq)]
pub enum EventPayload {
    /// No data.
    None,
    /// A drag: translation since the start and velocity (points / second).
    Pan {
        /// Horizontal translation.
        translation_x: f32,
        /// Vertical translation.
        translation_y: f32,
        /// Horizontal velocity.
        velocity_x: f32,
        /// Vertical velocity.
        velocity_y: f32,
    },
    /// A scroll position.
    Scroll {
        /// Horizontal content offset.
        x: f32,
        /// Vertical content offset.
        y: f32,
    },
    /// A loaded image's natural size in points.
    Image {
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
    /// An error message.
    Error {
        /// The message.
        message: String,
    },
}

/// How an image fits its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeMode {
    /// Fill the box, cropping (default).
    #[default]
    Cover,
    /// Fit inside the box, letterboxing.
    Contain,
    /// Stretch to the box.
    Stretch,
    /// Center at natural size.
    Center,
}

/// An image source.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSource {
    /// `https://…`, `asset:/path` (bundled file) or a file path.
    pub uri: Arc<str>,
    /// Natural size in points, when known up front (bundled assets): used
    /// as the intrinsic layout size.
    pub size: Option<(f32, f32)>,
}

/// A non-style property of a host node.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    /// `<image src>`.
    Source(Option<ImageSource>),
    /// `<image resizeMode>`.
    ResizeMode(ResizeMode),
    /// `<scroll horizontal>` / `<list horizontal>`.
    Horizontal(bool),
    /// Whether the scroll indicator is shown.
    ShowsIndicator(bool),
    /// Pull-to-refresh spinner state.
    Refreshing(bool),
    /// Number of items of a list.
    Count(u32),
    /// Size used for list items not measured yet.
    EstimatedItemSize(f32),
    /// The recycling slot of a list cell (set once by the runtime).
    Slot(u32),
    /// The item index a list cell now renders (set by the runtime after
    /// each bind, in the same batch as the cell's new content).
    CellIndex(u32),
    /// Native animation value driven by the scroll offset (main axis).
    ScrollValue(Option<yf_anim::NodeId>),
    /// Native animation values driven by a drag (x, y).
    PanValues(Option<yf_anim::NodeId>, Option<yf_anim::NodeId>),
}

/// A mutation produced by the JS renderer.
#[derive(Debug)]
pub enum Op {
    /// Creates a detached element node.
    Create {
        /// New node id.
        id: NodeId,
        /// Element tag.
        tag: Tag,
    },
    /// Creates a detached raw text node.
    CreateText {
        /// New node id.
        id: NodeId,
        /// Text content.
        text: String,
    },
    /// Replaces the content of a raw text node.
    SetText {
        /// Text node id.
        id: NodeId,
        /// New content.
        text: String,
    },
    /// Replaces the whole style of a node.
    SetStyle {
        /// Node id.
        id: NodeId,
        /// The new style; boxed because layout styles are large.
        style: Box<Style>,
    },
    /// Sets a non-style property.
    SetProp {
        /// Node id.
        id: NodeId,
        /// The property.
        prop: Prop,
    },
    /// Enables or disables an event listener on a node.
    Listen {
        /// Node id.
        id: NodeId,
        /// Event kind.
        event: EventKind,
        /// Whether a handler is attached.
        enabled: bool,
    },
    /// Inserts (or moves) `child` into `parent` before `before`, or appends.
    Insert {
        /// Parent node id.
        parent: NodeId,
        /// Child node id.
        child: NodeId,
        /// Sibling to insert before; `None` appends.
        before: Option<NodeId>,
    },
    /// Detaches `child` from `parent`. The child stays alive until released.
    Remove {
        /// Parent node id.
        parent: NodeId,
        /// Child node id.
        child: NodeId,
    },
    /// The JS node was garbage collected: free the node and its native view.
    Release {
        /// Node id.
        id: NodeId,
    },
    /// An animation graph change, forwarded to the platform in order.
    Anim(AnimOp),
}
