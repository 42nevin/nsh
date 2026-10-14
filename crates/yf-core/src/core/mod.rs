mod list;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use slab::Slab;
use taffy::{AvailableSpace, NodeId as LayoutId, Overflow, Position, TaffyTree};
use yf_anim::AnimProp;

use self::list::{ExtraKind, ListState};
use crate::{
    geometry::{Rect, Size},
    mount::{
        ActivityProps, ControlSizes, InputConfig, ModalConfig, MountBatch, MountItem, NativeProps,
        ScreenConfig, ScrollBehavior, SliderProps, StickyCell, SwitchProps, VideoProps, ViewKind,
    },
    ops::{
        CellSlot, Command, EventKind, EventMask, EventPayload, HeaderSlot, ImageSource,
        KeyboardDismissMode, ModalProp, NativeProp, NodeId, Op, PayloadValue, Prop, ROOT_ID,
        ResizeMode, ScreenProp, ScrollProp, Tag,
    },
    props::{AccessibilityProp, EdgeInsets, GestureValues, LayoutAnimation, ViewProps},
    style::{EllipsizeMode, Style, TextStyle, ViewStyle},
    text::{AttributedText, MeasureCache, ResolvedTextStyle, TextEngine, TextLayout},
};

/// Slab index of a node. Internal links use keys, never ids, to avoid the
/// id -> key indirection on hot paths.
type Key = u32;

const NO_KEY: Key = Key::MAX;

/// Dirty flags, processed once per commit.
mod dirty {
    /// Emit [`crate::MountItem::ViewStyle`].
    pub(super) const VIEW: u32 = 1 << 0;
    /// Emit [`crate::MountItem::Events`].
    pub(super) const EVENTS: u32 = 1 << 1;
    /// Rebuild the attributed text of a text block.
    pub(super) const TEXT_BUILD: u32 = 1 << 2;
    /// Lay out the text block again and emit [`crate::MountItem::Text`].
    pub(super) const TEXT_MOUNT: u32 = 1 << 3;
    /// Emit [`crate::MountItem::Image`].
    pub(super) const IMAGE: u32 = 1 << 4;
    /// Emit [`crate::MountItem::ScrollConfig`].
    pub(super) const SCROLL: u32 = 1 << 5;
    /// Emit [`crate::MountItem::Refreshing`].
    pub(super) const REFRESHING: u32 = 1 << 6;
    /// Emit [`crate::MountItem::AnimBindings`].
    pub(super) const ANIM: u32 = 1 << 7;
    /// Emit [`crate::MountItem::PanValues`].
    pub(super) const PAN: u32 = 1 << 8;
    /// Emit [`crate::MountItem::Input`].
    pub(super) const INPUT: u32 = 1 << 9;
    /// Emit [`crate::MountItem::InputValue`].
    pub(super) const INPUT_VALUE: u32 = 1 << 10;
    /// Emit [`crate::MountItem::ViewProps`].
    pub(super) const VIEW_PROPS: u32 = 1 << 11;
    /// Emit [`crate::MountItem::ScreenConfig`].
    pub(super) const SCREEN: u32 = 1 << 12;
    /// Emit [`crate::MountItem::ModalConfig`].
    pub(super) const MODAL: u32 = 1 << 13;
    /// Emit [`crate::MountItem::NativeProps`].
    pub(super) const NATIVE: u32 = 1 << 14;
    /// Emit [`crate::MountItem::ScrollBehavior`].
    pub(super) const SCROLL_BEHAVIOR: u32 = 1 << 15;
}

/// A native control: its properties, and for switches and sliders the
/// value JS set (the platform may have moved away from it).
#[derive(Debug)]
struct ControlState {
    props: NativeProps,
    /// The value JS set last (switches: 0 or 1).
    js_value: f32,
    /// JS set the value since the user last changed it.
    js_set: bool,
}

/// A layout root other than the surface's: screens, modals and header
/// items are laid out on their own, at a size their container gives them.
#[derive(Debug, Default)]
struct RootState {
    /// Size the platform reported for the root (modals).
    reported: Option<Size>,
    /// Safe-area insets the platform reported for the root (modals).
    insets: EdgeInsets,
    /// The size and header height the root was last laid out with.
    laid_out: Option<(Size, f32)>,
    /// Screen options (screens).
    screen: ScreenConfig,
    /// Modal options (modals).
    modal: ModalConfig,
    /// Where a header item goes (header items).
    slot: HeaderSlot,
}

/// Paragraph settings of a text block (or input).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Paragraph {
    /// `numberOfLines`; 0 means no limit.
    lines: u32,
    /// `ellipsizeMode`.
    ellipsize: EllipsizeMode,
    /// `allowFontScaling`.
    allow_font_scaling: bool,
    /// `maxFontSizeMultiplier`; 0 means no limit.
    max_font_multiplier: f32,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            lines: 0,
            ellipsize: EllipsizeMode::Tail,
            allow_font_scaling: true,
            max_font_multiplier: 0.0,
        }
    }
}

impl Paragraph {
    /// The factor applied to font sizes for the user's text size setting.
    fn font_multiplier(&self, font_scale: f32) -> f32 {
        if !self.allow_font_scaling {
            return 1.0;
        }
        if self.max_font_multiplier >= 1.0 {
            font_scale.min(self.max_font_multiplier)
        } else {
            font_scale
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    View,
    Text,
    RawText,
    Image,
    Scroll,
    List,
    Cell,
    Input,
    Stack,
    Screen,
    Modal,
    HeaderItem,
    Switch,
    Slider,
    Activity,
    WebView,
    Video,
}

impl Kind {
    /// Kinds whose native view hosts child views.
    fn is_container(self) -> bool {
        matches!(
            self,
            Kind::View
                | Kind::Scroll
                | Kind::List
                | Kind::Cell
                | Kind::Stack
                | Kind::Screen
                | Kind::Modal
                | Kind::HeaderItem
        )
    }

    /// Kinds laid out as their own root, not as part of their parent.
    fn is_root(self) -> bool {
        matches!(self, Kind::Screen | Kind::Modal | Kind::HeaderItem)
    }

    fn view_kind(self) -> ViewKind {
        match self {
            Kind::Text => ViewKind::Text,
            Kind::Image => ViewKind::Image,
            Kind::Scroll | Kind::List => ViewKind::Scroll,
            Kind::Input => ViewKind::Input,
            Kind::Stack => ViewKind::Stack,
            Kind::Screen => ViewKind::Screen,
            Kind::Modal => ViewKind::Modal,
            Kind::HeaderItem => ViewKind::HeaderItem,
            Kind::Switch => ViewKind::Switch,
            Kind::Slider => ViewKind::Slider,
            Kind::Activity => ViewKind::ActivityIndicator,
            Kind::WebView => ViewKind::WebView,
            Kind::Video => ViewKind::Video,
            Kind::View | Kind::Cell | Kind::RawText => ViewKind::View,
        }
    }

    /// Kinds whose taffy style is derived from the JS style (and must keep
    /// the JS style around to re-derive it).
    fn derives_layout(self) -> bool {
        matches!(
            self,
            Kind::Text | Kind::Scroll | Kind::List | Kind::Cell | Kind::Screen | Kind::Modal
        )
    }
}

#[derive(Debug)]
struct ScrollState {
    horizontal: bool,
    shows_indicator: bool,
    refreshing: bool,
    scroll_value: Option<yf_anim::NodeId>,
    keyboard_dismiss: KeyboardDismissMode,
    /// Last content size sent to the platform.
    content: Option<Size>,
    behavior: ScrollBehavior,
    /// Minimum milliseconds between `onScroll` events.
    event_throttle_ms: f32,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            horizontal: false,
            // React Native shows scroll indicators by default.
            shows_indicator: true,
            refreshing: false,
            scroll_value: None,
            keyboard_dismiss: KeyboardDismissMode::None,
            content: None,
            behavior: ScrollBehavior::default(),
            event_throttle_ms: 0.0,
        }
    }
}

#[derive(Debug, Default)]
struct InputState {
    value: String,
    /// JS set `value` (or `defaultValue`) at least once.
    value_set: bool,
    /// Edits reported by the platform.
    count: u32,
    /// Everything but the text style, which comes from the node's style.
    config: InputConfig,
    /// Last content box sent to the platform.
    content: Option<Rect>,
}

#[derive(Debug, Default)]
struct ImageState {
    source: Option<ImageSource>,
    resize_mode: ResizeMode,
}

struct Node {
    id: NodeId,
    kind: Kind,
    /// Inside a text block: a nested `<text>` span or a raw text run.
    inline: bool,
    parent: Key,
    children: Vec<Key>,
    /// Layout node. Present exactly for nodes that render a native view.
    /// Raw text and spans have none.
    layout: Option<LayoutId>,
    /// The style JS set, for kinds whose taffy style is derived from it.
    base_layout: Option<Box<taffy::Style>>,
    view_style: ViewStyle,
    text_style: TextStyle,
    animated: Vec<(AnimProp, yf_anim::NodeId)>,
    events: EventMask,
    /// `zIndex`: native views are ordered by it (stable) among siblings.
    z_index: i32,
    /// Touch, accessibility and test properties; most views have none.
    props: Option<Box<ViewProps>>,
    /// Raw text content.
    text: String,
    /// Paragraph settings of a text block or input.
    paragraph: Paragraph,
    image: Option<Box<ImageState>>,
    scroll: Option<Box<ScrollState>>,
    list: Option<Box<ListState>>,
    input: Option<Box<InputState>>,
    /// Screens, modals and header items.
    root: Option<Box<RootState>>,
    /// Switches, sliders, activity indicators, web views and videos.
    control: Option<Box<ControlState>>,
    /// Recycling slot (or role) of a list cell.
    slot: Option<CellSlot>,
    /// Native values driven by gestures on the view.
    gestures: GestureValues,
    /// The native view exists.
    created: bool,
    /// Last frame sent to the platform.
    frame: Rect,
    dirty: u32,
    queued: bool,
    /// In `Core::pending_text`.
    text_queued: bool,
    /// A list cell whose subtree was laid out again this commit.
    relaid: bool,
}

impl Node {
    fn new(id: NodeId, kind: Kind) -> Self {
        Self {
            id,
            kind,
            inline: false,
            parent: NO_KEY,
            children: Vec::new(),
            layout: None,
            base_layout: None,
            view_style: ViewStyle::default(),
            text_style: TextStyle::default(),
            animated: Vec::new(),
            events: EventMask::default(),
            z_index: 0,
            props: None,
            text: String::new(),
            paragraph: Paragraph::default(),
            image: matches!(kind, Kind::Image).then(Box::default),
            scroll: matches!(kind, Kind::Scroll | Kind::List).then(Box::default),
            list: matches!(kind, Kind::List).then(|| Box::new(ListState::new())),
            input: matches!(kind, Kind::Input).then(Box::default),
            root: kind.is_root().then(Box::default),
            control: match kind {
                Kind::Switch => Some(NativeProps::Switch(SwitchProps::default())),
                Kind::Slider => Some(NativeProps::Slider(SliderProps::default())),
                Kind::Activity => Some(NativeProps::Activity(ActivityProps::default())),
                Kind::WebView => Some(NativeProps::WebView(Box::default())),
                Kind::Video => Some(NativeProps::Video(Box::<VideoProps>::default())),
                _ => None,
            }
            .map(|props| {
                Box::new(ControlState {
                    props,
                    js_value: 0.0,
                    js_set: false,
                })
            }),
            slot: None,
            gestures: GestureValues::default(),
            created: false,
            frame: Rect::UNSET,
            dirty: 0,
            queued: false,
            text_queued: false,
            relaid: false,
        }
    }
}

/// Taffy context of a text block.
struct TextContext<P> {
    text: Arc<AttributedText>,
    /// `text.layout_hash()`, computed once per content.
    layout_hash: u64,
    /// Platform typesetting state, created on first use.
    prepared: Option<P>,
    /// Last measurement: `(max_width, size)`.
    cache: Option<(Option<f32>, Size)>,
    /// Last final layout sent to the platform and the box it was made for.
    laid_out: Option<(Size, TextLayout)>,
}

impl<P> TextContext<P> {
    fn new(text: Arc<AttributedText>) -> Self {
        Self {
            layout_hash: text.layout_hash(),
            text,
            prepared: None,
            cache: None,
            laid_out: None,
        }
    }
}

/// Measured leaves: text blocks, images with a known natural size and
/// controls of a fixed size.
enum LeafContext<P> {
    Text(TextContext<P>),
    Image(Option<Size>),
    /// A control's natural size; a zero width takes the layout's.
    Fixed(Size),
}

/// A request for the JS runtime to render list items into cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListBind {
    /// The list.
    pub list: NodeId,
    /// `(slot, item index)` pairs.
    pub binds: Vec<(u32, u32)>,
}

/// An op referenced a node in an impossible way. This is a renderer bug;
/// the op is skipped and the tree stays consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyError {
    /// The id is unknown (never created or already released).
    UnknownNode(NodeId),
    /// The id is already in use.
    DuplicateNode(NodeId),
    /// The child is not a child of the given parent.
    NotAChild(NodeId),
    /// The `before` sibling is not a child of the given parent.
    BadAnchor(NodeId),
    /// The insert would create a cycle.
    Cycle(NodeId),
}

/// The shadow tree of one surface.
///
/// Feed it [`Op`]s with [`Core::apply`], then call [`Core::commit`] once per
/// JS turn to get the [`MountBatch`] for the platform. Lists may then ask
/// the runtime to render more items ([`Core::take_list_binds`]); the engine
/// runs those and commits again within the same frame.
pub struct Core<M: TextEngine> {
    nodes: Slab<Node>,
    /// Node id -> key + 1 (0 = none). Ids are dense, so this is a flat vec.
    slots: Vec<u32>,
    layout: TaffyTree<LeafContext<M::Prepared>>,
    text_engine: M,
    measure_cache: MeasureCache,
    /// Text blocks whose final layout must be (re)made after layout.
    pending_text: Vec<Key>,
    /// Live list nodes.
    lists: Vec<Key>,
    /// Live screens, modals and header items.
    roots: Vec<Key>,
    /// Height of the native header bar of screens: normal and large title.
    header_heights: (f32, f32),
    /// Natural sizes of the platform's controls.
    control_sizes: ControlSizes,
    /// Switches the user toggled: JS gets a turn to accept the change.
    toggled: Vec<Key>,
    /// The surface's safe-area insets (stacks at the top extend their
    /// headers under the status bar).
    safe_area: EdgeInsets,
    list_binds: Vec<ListBind>,
    events: Vec<(NodeId, EventKind, EventPayload)>,
    /// Reused buffer of `(item index, cell)` for list updates.
    cells_scratch: Vec<(u32, Key)>,
    root: Key,
    viewport: Size,
    mount: Vec<MountItem>,
    /// Imperative calls, run after everything else in the batch.
    commands: Vec<(NodeId, Command)>,
    /// Animates the layout changes of the next commit.
    layout_animation: Option<LayoutAnimation>,
    /// The user's preferred text size (1.0 is the default).
    font_scale: f32,
    queue: Vec<Key>,
    needs_layout: bool,
}

impl<M: TextEngine> Core<M> {
    /// Creates a core whose root view (id [`ROOT_ID`]) fills `viewport`.
    pub fn new(text_engine: M, viewport: Size) -> Self {
        let mut layout = TaffyTree::with_capacity(256);
        let root_layout = layout.new_leaf(root_style(viewport)).expect("taffy root");
        let mut nodes = Slab::with_capacity(256);
        let mut root = Node::new(ROOT_ID, Kind::View);
        root.layout = Some(root_layout);
        // The platform owns the root view.
        root.created = true;
        let root = nodes.insert(root) as Key;
        let mut slots = vec![0; 256];
        slots[ROOT_ID as usize] = root + 1;
        Self {
            nodes,
            slots,
            layout,
            text_engine,
            measure_cache: MeasureCache::default(),
            pending_text: Vec::new(),
            lists: Vec::new(),
            roots: Vec::new(),
            header_heights: (44.0, 96.0),
            control_sizes: ControlSizes::default(),
            toggled: Vec::new(),
            safe_area: EdgeInsets::default(),
            list_binds: Vec::new(),
            events: Vec::new(),
            cells_scratch: Vec::new(),
            root,
            viewport,
            mount: Vec::new(),
            commands: Vec::new(),
            layout_animation: None,
            font_scale: 1.0,
            queue: Vec::new(),
            needs_layout: true,
        }
    }

    /// Sets the height of the platform's screen header bar, normal and with
    /// a large title (status bar excluded).
    pub fn set_header_heights(&mut self, normal: f32, large: f32) {
        if self.header_heights != (normal, large) {
            self.header_heights = (normal, large);
            self.needs_layout = true;
        }
    }

    /// Sets the natural sizes of the platform's controls (before any
    /// control is created).
    pub fn set_control_sizes(&mut self, sizes: ControlSizes) {
        self.control_sizes = sizes;
    }

    /// The user moved switch or slider `id` to `value` (a switch: 0 or 1),
    /// before JS hears about it. A switch whose `value` JS does not change
    /// in response goes back to it (controlled, like React Native).
    pub fn control_changed(&mut self, id: NodeId, value: f32) {
        let Ok(key) = self.key(id) else { return };
        let Some(control) = self.nodes[key as usize].control.as_mut() else {
            return;
        };
        control.js_set = false;
        match &mut control.props {
            NativeProps::Switch(p) => {
                p.value = value != 0.0;
                self.toggled.push(key);
            }
            NativeProps::Slider(p) => p.value = value,
            _ => {}
        }
    }

    /// Sets the surface's safe-area insets.
    pub fn set_safe_area(&mut self, insets: EdgeInsets) {
        if self.safe_area != insets {
            self.safe_area = insets;
            self.needs_layout = true;
        }
    }

    /// The platform presents root `id` (a modal) at `size`, with `insets`
    /// of it covered by system UI.
    pub fn set_root_size(&mut self, id: NodeId, size: Size, insets: EdgeInsets) {
        let Ok(key) = self.key(id) else { return };
        if let Some(root) = self.nodes[key as usize].root.as_mut()
            && (root.reported != Some(size) || root.insets != insets)
        {
            root.reported = Some(size);
            root.insets = insets;
            self.needs_layout = true;
        }
    }

    /// Sets the user's preferred text size (1.0 is the default): text that
    /// allows font scaling is laid out again.
    pub fn set_font_scale(&mut self, scale: f32) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        if scale == self.font_scale {
            return;
        }
        self.font_scale = scale;
        let blocks: Vec<(Key, Kind)> = self
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, Kind::Text | Kind::Input) && !n.inline)
            .map(|(k, n)| (k as Key, n.kind))
            .collect();
        for (key, kind) in blocks {
            if kind == Kind::Text {
                self.mark(key, dirty::TEXT_BUILD);
            } else {
                self.measure_input(key);
                self.mark(key, dirty::INPUT);
            }
        }
    }

    /// Number of live nodes, including the root.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Changes the size of the root view.
    pub fn set_viewport(&mut self, viewport: Size) {
        if viewport != self.viewport {
            self.viewport = viewport;
            let root = self.nodes[self.root as usize].layout.expect("root layout");
            self.layout
                .set_style(root, root_style(viewport))
                .expect("taffy root");
            self.needs_layout = true;
        }
    }

    /// The platform scrolled a scroll view or list to `(x, y)`, having
    /// applied `applied` of the scrolls the core issued to it
    /// (`MountItem::ScrollBy` and `Command::ScrollTo` of lists).
    pub fn set_scroll_offset(&mut self, id: NodeId, x: f32, y: f32, applied: u32) {
        let Ok(key) = self.key(id) else { return };
        let node = &mut self.nodes[key as usize];
        let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
        if let Some(list) = node.list.as_mut() {
            list.reported(if horizontal { x } else { y }, applied);
        }
    }

    /// What `onScroll` reports for scroll view `id`, if it listens to it:
    /// the minimum milliseconds between events, the content size and the
    /// viewport size.
    pub fn scroll_listener(&self, id: NodeId) -> Option<(f32, Size, Size)> {
        let node = &self.nodes[self.key(id).ok()? as usize];
        if !node.events.contains(EventKind::Scroll) {
            return None;
        }
        let scroll = node.scroll.as_ref()?;
        let frame = Size::new(node.frame.width, node.frame.height);
        Some((
            scroll.event_throttle_ms,
            scroll.content.unwrap_or(frame),
            frame,
        ))
    }

    /// Moves list render requests produced by the last commit into `out`.
    pub fn take_list_binds(&mut self, out: &mut Vec<ListBind>) {
        out.append(&mut self.list_binds);
    }

    /// Moves events the core raised (e.g. `onEndReached`, `onLayout`) into
    /// `out`.
    pub fn take_events(&mut self, out: &mut Vec<(NodeId, EventKind, EventPayload)>) {
        out.append(&mut self.events);
    }

    /// Registers a custom font (TrueType / OpenType data) with the text
    /// engine. Text measured before is measured again.
    pub fn register_font(&mut self, data: &[u8]) -> Result<(), String> {
        self.text_engine.register_font(data)?;
        self.measure_cache = MeasureCache::default();
        let blocks: Vec<Key> = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                matches!(n.kind, Kind::Text | Kind::Input) && !n.inline && n.layout.is_some()
            })
            .map(|(k, _)| k as Key)
            .collect();
        for key in blocks {
            let layout = self.nodes[key as usize].layout.expect("text layout");
            let ctx = self.text_ctx(layout);
            ctx.prepared = None;
            ctx.cache = None;
            ctx.laid_out = None;
            self.layout.mark_dirty(layout).expect("taffy dirty");
            self.needs_layout = true;
            if self.nodes[key as usize].kind == Kind::Text {
                self.queue_text(key);
            } else {
                // The platform resolves the input's font again.
                self.mark(key, dirty::INPUT);
            }
        }
        Ok(())
    }

    /// The user edited input `id` (before JS sees the edit): keeps the text
    /// for layout and for stale-value detection.
    pub fn input_changed(&mut self, id: NodeId, text: &str, count: u32) {
        let Ok(key) = self.key(id) else { return };
        let Some(input) = self.nodes[key as usize].input.as_mut() else {
            return;
        };
        input.count = count;
        if input.value != text {
            input.value.clear();
            input.value.push_str(text);
            self.measure_input(key);
        }
    }

    /// Applies one op.
    pub fn apply(&mut self, op: Op) -> Result<(), ApplyError> {
        match op {
            Op::Create { id, tag } => self.create(id, tag),
            Op::CreateText { id, text } => {
                let key = self.alloc(id, Kind::RawText)?;
                self.nodes[key as usize].text = text;
                Ok(())
            }
            Op::SetText { id, text } => {
                let key = self.key(id)?;
                let node = &mut self.nodes[key as usize];
                if node.text != text {
                    node.text = text;
                    if node.inline {
                        self.mark_text_block(key);
                    }
                }
                Ok(())
            }
            Op::SetStyle { id, style } => self.set_style(id, *style),
            Op::SetProp { id, prop } => self.set_prop(id, prop),
            Op::Listen { id, event, enabled } => {
                let key = self.key(id)?;
                let node = &mut self.nodes[key as usize];
                let before = node.events;
                node.events.set(event, enabled);
                // A listener added after layout still learns the frame.
                if event == EventKind::Layout && enabled && !before.contains(event) {
                    let f = node.frame;
                    if !f.x.is_nan() {
                        self.events.push((
                            id,
                            event,
                            EventPayload::Layout {
                                x: f.x,
                                y: f.y,
                                width: f.width,
                                height: f.height,
                            },
                        ));
                    }
                }
                if node.events != before {
                    self.mark(key, dirty::EVENTS);
                }
                Ok(())
            }
            Op::Insert {
                parent,
                child,
                before,
            } => self.insert(parent, child, before),
            Op::Remove { parent, child } => {
                let parent = self.key(parent)?;
                let child = self.key(child)?;
                if self.nodes[child as usize].parent != parent {
                    return Err(ApplyError::NotAChild(self.nodes[child as usize].id));
                }
                self.detach(child);
                Ok(())
            }
            Op::Release { id } => self.release(id),
            Op::Anim(op) => {
                self.mount.push(MountItem::Anim(op));
                Ok(())
            }
            Op::Command {
                command: Command::LayoutAnimation(animation),
                ..
            } => {
                self.layout_animation = Some(animation);
                Ok(())
            }
            Op::Command { id, command } => {
                self.key(id)?;
                self.commands.push((id, command));
                Ok(())
            }
        }
    }

    /// Finishes a turn: rebuilds text, runs layout, updates lists and
    /// returns the mount items, or `None` when nothing changed.
    pub fn commit(&mut self) -> Option<MountBatch> {
        // Switches the user toggled whose value JS kept: back to it.
        for key in std::mem::take(&mut self.toggled) {
            let Some(control) = self
                .nodes
                .get_mut(key as usize)
                .and_then(|n| n.control.as_mut())
            else {
                continue;
            };
            if let NativeProps::Switch(p) = &mut control.props
                && !control.js_set
                && p.value != (control.js_value != 0.0)
            {
                p.value = control.js_value != 0.0;
                self.mark(key, dirty::NATIVE);
            }
        }
        // Platforms animate the frames, insertions and removals that follow
        // in the batch; ops may already have queued some.
        if let Some(animation) = self.layout_animation.take() {
            self.mount.insert(0, MountItem::LayoutAnimation(animation));
        }
        self.flush_dirty();
        let relayout = self.needs_layout;
        if relayout {
            self.needs_layout = false;
            // Only walk the main tree when it was laid out again (a change
            // inside a list cell leaves it untouched).
            if self.layout_main() {
                let root = self.root;
                let children = std::mem::take(&mut self.nodes[root as usize].children);
                for &child in &children {
                    if !self.nodes[child as usize].kind.is_root() {
                        self.emit_frames(child);
                    }
                }
                self.nodes[root as usize].children = children;
            }
            // Screens and modals, then list cells (lists may live in them).
            self.layout_roots();
            self.layout_cells();
        }
        self.update_lists(relayout);
        self.flush_text();
        // On laid out views; dropped for views that were never shown.
        for (id, mut command) in std::mem::take(&mut self.commands) {
            let Ok(key) = self.key(id) else { continue };
            let node = &self.nodes[key as usize];
            if !node.created {
                continue;
            }
            // Lists resolved theirs in `update_lists`; a scroll view can
            // still scroll to its end.
            match command {
                Command::ScrollToEnd { animated } => {
                    let Some(content) = node.scroll.as_ref().and_then(|s| s.content) else {
                        continue;
                    };
                    let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
                    let frame = node.frame;
                    let (x, y) = if horizontal {
                        ((content.width - frame.width).max(0.0), 0.0)
                    } else {
                        (0.0, (content.height - frame.height).max(0.0))
                    };
                    command = Command::ScrollTo { x, y, animated };
                }
                Command::ScrollToIndex { .. } => continue,
                _ => {}
            }
            self.mount.push(MountItem::Command { id, command });
        }
        if self.mount.is_empty() || matches!(self.mount[..], [MountItem::LayoutAnimation(_)]) {
            self.mount.clear();
            None
        } else {
            Some(MountBatch {
                items: std::mem::take(&mut self.mount),
            })
        }
    }

    #[inline]
    fn key(&self, id: NodeId) -> Result<Key, ApplyError> {
        match self.slots.get(id as usize) {
            Some(&k) if k != 0 => Ok(k - 1),
            _ => Err(ApplyError::UnknownNode(id)),
        }
    }

    fn alloc(&mut self, id: NodeId, kind: Kind) -> Result<Key, ApplyError> {
        let idx = id as usize;
        if idx >= self.slots.len() {
            self.slots.resize((idx + 1).next_power_of_two(), 0);
        } else if self.slots[idx] != 0 {
            return Err(ApplyError::DuplicateNode(id));
        }
        let key = self.nodes.insert(Node::new(id, kind)) as Key;
        self.slots[idx] = key + 1;
        Ok(key)
    }

    fn create(&mut self, id: NodeId, tag: Tag) -> Result<(), ApplyError> {
        let kind = match tag {
            Tag::View => Kind::View,
            Tag::Text => Kind::Text,
            Tag::Image => Kind::Image,
            Tag::Scroll => Kind::Scroll,
            Tag::List => Kind::List,
            Tag::Cell => Kind::Cell,
            Tag::Input => Kind::Input,
            Tag::Stack => Kind::Stack,
            Tag::Screen => Kind::Screen,
            Tag::Modal => Kind::Modal,
            Tag::HeaderItem => Kind::HeaderItem,
            Tag::Switch => Kind::Switch,
            Tag::Slider => Kind::Slider,
            Tag::ActivityIndicator => Kind::Activity,
            Tag::WebView => Kind::WebView,
            Tag::Video => Kind::Video,
        };
        let key = self.alloc(id, kind)?;
        let style = Style::default().layout;
        let layout = match kind {
            Kind::Text => {
                self.nodes[key as usize].dirty |= dirty::TEXT_BUILD;
                self.layout.new_leaf_with_context(
                    style.clone(),
                    LeafContext::Text(TextContext::new(Arc::default())),
                )
            }
            Kind::Image => self
                .layout
                .new_leaf_with_context(style.clone(), LeafContext::Image(None)),
            // Inputs measure like a text block holding their text (or
            // placeholder).
            Kind::Input => self.layout.new_leaf_with_context(
                style.clone(),
                LeafContext::Text(TextContext::new(Arc::default())),
            ),
            Kind::Switch => self.layout.new_leaf_with_context(
                style.clone(),
                LeafContext::Fixed(self.control_sizes.switch),
            ),
            Kind::Activity => self.layout.new_leaf_with_context(
                style.clone(),
                LeafContext::Fixed(self.control_sizes.activity_small),
            ),
            Kind::Slider => self.layout.new_leaf_with_context(
                style.clone(),
                LeafContext::Fixed(Size::new(0.0, self.control_sizes.slider_height)),
            ),
            _ => self.layout.new_leaf(style.clone()),
        }
        .expect("taffy leaf");
        let node = &mut self.nodes[key as usize];
        node.layout = Some(layout);
        if kind.derives_layout() {
            node.base_layout = Some(Box::new(style));
        }
        if kind == Kind::List {
            self.lists.push(key);
        }
        if kind.is_root() {
            self.roots.push(key);
        }
        if kind == Kind::Input {
            self.measure_input(key);
        }
        self.sync_layout_style(key);
        Ok(())
    }

    /// Rebuilds the text an input is measured with: its value, else its
    /// placeholder, else an empty line.
    fn measure_input(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (Some(input), Some(layout)) = (&node.input, node.layout) else {
            return;
        };
        let multiline = input.config.multiline;
        let mut text = AttributedText {
            max_lines: if multiline { 0 } else { 1 },
            ..AttributedText::default()
        };
        let style = scaled(
            ResolvedTextStyle::default().inherit(&node.text_style),
            node.paragraph.font_multiplier(self.font_scale),
        );
        let content = if !input.value.is_empty() {
            &input.value
        } else {
            &input.config.placeholder
        };
        text.push(content, style);
        // An empty last line still takes space (a trailing line break, or no
        // text at all).
        if content.is_empty() || (multiline && content.ends_with('\n')) {
            text.push("\u{200b}", style);
        }
        let ctx = self.text_ctx(layout);
        if ctx.text.layout_eq(&text) {
            return;
        }
        *ctx = TextContext::new(Arc::new(text));
        self.layout.mark_dirty(layout).expect("taffy dirty");
        self.needs_layout = true;
    }

    /// Pushes the effective taffy style of `key`: the JS style, adjusted
    /// for scroll containers and list cells.
    fn sync_layout_style(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (Some(layout), Some(base)) = (node.layout, node.base_layout.as_deref()) else {
            return;
        };
        let mut style = base.clone();
        let kind = node.kind;
        if let Some(root) = self.nodes[key as usize].root.as_mut() {
            // Laid out again with its size by `layout_roots`.
            root.laid_out = None;
        }
        let node = &self.nodes[key as usize];
        match kind {
            Kind::Scroll => {
                // Children may overflow along the scroll axis; a horizontal
                // scroll lays its content out in a row.
                let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
                if horizontal {
                    style.flex_direction = taffy::FlexDirection::Row;
                    style.overflow.x = Overflow::Scroll;
                } else {
                    style.overflow.y = Overflow::Scroll;
                }
            }
            Kind::List => {
                style.overflow = taffy::Point {
                    x: Overflow::Hidden,
                    y: Overflow::Hidden,
                }
            }
            Kind::Cell => {
                // Cells are their own layout roots (laid out at the list's
                // cross size, positioned by the list): a change in one row
                // never relayouts the rest of the tree.
                let horizontal = self
                    .nodes
                    .get(node.parent as usize)
                    .and_then(|p| p.scroll.as_ref())
                    .is_some_and(|s| s.horizontal);
                style.position = Position::Relative;
                if horizontal {
                    style.size.height = taffy::Dimension::percent(1.0);
                } else {
                    style.size.width = taffy::Dimension::percent(1.0);
                }
            }
            _ => {}
        }
        self.layout.set_style(layout, style).expect("taffy style");
        self.needs_layout = true;
    }

    fn set_style(&mut self, id: NodeId, style: Style) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        let node = &mut self.nodes[key as usize];
        let Style {
            layout,
            view,
            text,
            animated,
            z_index,
        } = style;
        let view_changed = node.view_style != view;
        let text_changed = node.text_style != text;
        let anim_changed = node.animated != animated;
        let z_changed = node.z_index != z_index;
        node.view_style = view;
        node.text_style = text;
        node.animated = animated;
        node.z_index = z_index;
        if z_changed {
            self.restack(key);
        }
        let node = &mut self.nodes[key as usize];
        if node.kind.derives_layout() {
            node.base_layout = Some(Box::new(layout));
            self.sync_layout_style(key);
        } else if let Some(l) = node.layout {
            self.layout.set_style(l, layout).expect("taffy style");
            self.needs_layout = true;
        }
        let mut flags = 0;
        if view_changed {
            flags |= dirty::VIEW;
        }
        if anim_changed {
            flags |= dirty::ANIM;
        }
        if flags != 0 {
            self.mark(key, flags);
        }
        if text_changed {
            match self.nodes[key as usize].kind {
                Kind::Text => self.mark_text_block(key),
                Kind::Input => {
                    self.measure_input(key);
                    self.mark(key, dirty::INPUT);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn set_prop(&mut self, id: NodeId, prop: Prop) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        let node = &mut self.nodes[key as usize];
        let mut flags = 0;
        match prop {
            Prop::Source(source) => {
                if let Some(image) = node.image.as_mut() {
                    let size = source
                        .as_ref()
                        .and_then(|s| s.size)
                        .map(|(w, h)| Size::new(w, h));
                    image.source = source;
                    flags |= dirty::IMAGE;
                    let layout = node.layout.expect("image layout");
                    if let Some(LeafContext::Image(natural)) =
                        self.layout.get_node_context_mut(layout)
                        && *natural != size
                    {
                        *natural = size;
                        self.layout.mark_dirty(layout).expect("taffy dirty");
                        self.needs_layout = true;
                    }
                }
            }
            Prop::ResizeMode(mode) => {
                if let Some(image) = node.image.as_mut() {
                    image.resize_mode = mode;
                    flags |= dirty::IMAGE;
                } else if let Some(control) = node.control.as_mut() {
                    apply_native(
                        control,
                        NativeProp::VideoResizeMode(mode),
                        self.control_sizes,
                    );
                    flags |= dirty::NATIVE;
                }
            }
            Prop::Horizontal(horizontal) => {
                if let Some(scroll) = node.scroll.as_mut()
                    && scroll.horizontal != horizontal
                {
                    scroll.horizontal = horizontal;
                    flags |= dirty::SCROLL;
                    if let Some(list) = node.list.as_mut() {
                        list.set_horizontal(horizontal);
                    }
                    let children = node.children.clone();
                    self.sync_layout_style(key);
                    for child in children {
                        self.sync_layout_style(child);
                    }
                }
            }
            Prop::ShowsIndicator(shows) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.shows_indicator = shows;
                    flags |= dirty::SCROLL;
                }
            }
            Prop::Scroll(prop) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    let b = &mut scroll.behavior;
                    match prop {
                        ScrollProp::Paging(on) => b.paging = on,
                        ScrollProp::SnapInterval(v) => b.snap_interval = v.max(0.0),
                        ScrollProp::SnapOffsets(v) => b.snap_offsets = v,
                        ScrollProp::SnapAlignment(a) => b.snap_alignment = a,
                        ScrollProp::Deceleration(r) => b.deceleration = r,
                        ScrollProp::Bounces(on) => b.bounces = on,
                        ScrollProp::ContentInset(i) => b.content_inset = i,
                        ScrollProp::KeyboardTaps(t) => b.keyboard_taps = t,
                        ScrollProp::DisableIntervalMomentum(on) => b.one_step = on,
                        ScrollProp::EventThrottle(ms) => scroll.event_throttle_ms = ms.max(0.0),
                        ScrollProp::ContentOffset(x, y) => {
                            // Where it starts: scrolled there after layout.
                            let id = node.id;
                            self.commands.push((
                                id,
                                Command::ScrollTo {
                                    x,
                                    y,
                                    animated: false,
                                },
                            ));
                        }
                    }
                    flags |= dirty::SCROLL_BEHAVIOR;
                }
            }
            Prop::Refreshing(refreshing) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.refreshing = refreshing;
                    flags |= dirty::REFRESHING;
                }
            }
            Prop::ScrollValue(value) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.scroll_value = value;
                    flags |= dirty::SCROLL;
                }
            }
            Prop::Count(count) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_count(count);
                }
            }
            Prop::EstimatedItemSize(size) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_estimated(size);
                }
            }
            Prop::Slot(slot) => {
                node.slot = Some(slot);
                let parent = node.parent;
                if let Some(list) = self
                    .nodes
                    .get_mut(parent as usize)
                    .and_then(|p| p.list.as_mut())
                {
                    list.add_slot_cell(slot, key);
                }
            }
            Prop::Splice(at, removed, inserted) => {
                if let Some(list) = node.list.as_mut() {
                    list.splice(at, removed, inserted);
                }
            }
            Prop::Columns(columns) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_columns(columns);
                }
            }
            Prop::Inverted(inverted) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_inverted(inverted);
                }
            }
            Prop::StickyIndices(indices) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_sticky(indices);
                }
            }
            Prop::ListContent(content) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_content(content);
                }
            }
            Prop::Viewability(config) => {
                if let Some(list) = node.list.as_mut()
                    && list.viewability != config
                {
                    list.viewability = config;
                    list.dirty = true;
                }
            }
            Prop::EndReachedThreshold(threshold) => {
                if let Some(list) = node.list.as_mut() {
                    list.end_threshold = threshold.max(0.0);
                }
            }
            Prop::CellIndex(index) => {
                let parent = node.parent;
                if let Some(list) = self
                    .nodes
                    .get_mut(parent as usize)
                    .and_then(|p| p.list.as_mut())
                {
                    list.rendered(key, index);
                }
            }
            Prop::PanValues(x, y) => {
                (node.gestures.pan_x, node.gestures.pan_y) = (x, y);
                flags |= dirty::PAN;
            }
            Prop::PinchValue(value) => {
                node.gestures.pinch = value;
                flags |= dirty::PAN;
            }
            Prop::RotationValue(value) => {
                node.gestures.rotation = value;
                flags |= dirty::PAN;
            }
            Prop::NumberOfLines(lines) => {
                if node.paragraph.lines != lines {
                    node.paragraph.lines = lines;
                    self.mark_text_block(key);
                }
            }
            Prop::EllipsizeMode(mode) => {
                if node.paragraph.ellipsize != mode {
                    node.paragraph.ellipsize = mode;
                    self.mark_text_block(key);
                }
            }
            Prop::AllowFontScaling(allow) => {
                if node.paragraph.allow_font_scaling != allow {
                    node.paragraph.allow_font_scaling = allow;
                    self.font_settings_changed(key);
                }
            }
            Prop::MaxFontSizeMultiplier(max) => {
                if node.paragraph.max_font_multiplier != max {
                    node.paragraph.max_font_multiplier = max;
                    self.font_settings_changed(key);
                }
            }
            Prop::PointerEvents(value) => {
                let props = node.props.get_or_insert_default();
                if props.pointer_events != value {
                    props.pointer_events = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::HitSlop(value) => {
                let props = node.props.get_or_insert_default();
                if props.hit_slop != value {
                    props.hit_slop = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::ActiveOpacity(value) => {
                let props = node.props.get_or_insert_default();
                if props.active_opacity != value {
                    props.active_opacity = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::TestId(value) => {
                let props = node.props.get_or_insert_default();
                if props.test_id != value {
                    props.test_id = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::Accessibility(prop) => {
                let props = node.props.get_or_insert_default();
                let a = &mut props.accessibility;
                let before = a.clone();
                match prop {
                    AccessibilityProp::Accessible(v) => a.accessible = v,
                    AccessibilityProp::Label(v) => a.label = v,
                    AccessibilityProp::Hint(v) => a.hint = v,
                    AccessibilityProp::Role(v) => a.role = v,
                    AccessibilityProp::State(v) => a.state = v,
                    AccessibilityProp::Value(v) => a.value = v,
                    AccessibilityProp::Hidden(v) => a.hidden = v,
                    AccessibilityProp::Important(accessible, hidden) => {
                        a.accessible = accessible;
                        a.hidden = hidden;
                    }
                    AccessibilityProp::LiveRegion(v) => a.live_region = v,
                    AccessibilityProp::Actions(v) => a.actions = v,
                }
                if *a != before {
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::KeyboardDismissMode(mode) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.keyboard_dismiss = mode;
                    flags |= dirty::SCROLL;
                }
            }
            Prop::Screen(prop) => {
                if let Some(root) = node.root.as_mut() {
                    let c = &mut root.screen;
                    match prop {
                        ScreenProp::Title(v) => c.title = v,
                        ScreenProp::HeaderShown(v) => c.header_shown = v,
                        ScreenProp::HeaderLargeTitle(v) => c.large_title = v,
                        ScreenProp::HeaderTransparent(v) => c.header_transparent = v,
                        ScreenProp::HeaderTintColor(v) => c.tint_color = v,
                        ScreenProp::HeaderBackgroundColor(v) => c.background_color = v,
                        ScreenProp::HeaderTitleColor(v) => c.title_color = v,
                        ScreenProp::HeaderBackTitle(v) => c.back_title = v,
                        ScreenProp::HeaderBackVisible(v) => c.back_visible = v,
                        ScreenProp::GestureEnabled(v) => c.gesture_enabled = v,
                        ScreenProp::Animation(v) => c.animation = v,
                    }
                    // Header changes move the content.
                    root.laid_out = None;
                    self.needs_layout = true;
                    flags |= dirty::SCREEN;
                }
            }
            Prop::Modal(prop) => {
                if let Some(root) = node.root.as_mut() {
                    let c = &mut root.modal;
                    match prop {
                        ModalProp::Animation(v) => c.animation = v,
                        ModalProp::Presentation(v) => c.presentation = v,
                        ModalProp::Transparent(v) => c.transparent = v,
                    }
                    flags |= dirty::MODAL;
                }
            }
            Prop::HeaderSlot(slot) => {
                if let Some(root) = node.root.as_mut()
                    && root.slot != slot
                {
                    root.slot = slot;
                    let parent = node.parent;
                    if parent != NO_KEY {
                        self.mark(parent, dirty::SCREEN);
                    }
                }
            }
            // `scrollEnabled` of scroll views and lists.
            Prop::Native(NativeProp::ScrollEnabled(on)) if node.scroll.is_some() => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.behavior.enabled = on;
                    flags |= dirty::SCROLL_BEHAVIOR;
                }
            }
            Prop::Native(prop) => {
                if let Some(control) = node.control.as_mut() {
                    let resize = apply_native(control, prop, self.control_sizes);
                    flags |= dirty::NATIVE;
                    if let Some(size) = resize {
                        let layout = node.layout.expect("control layout");
                        if let Some(LeafContext::Fixed(s)) =
                            self.layout.get_node_context_mut(layout)
                        {
                            *s = size;
                        }
                        self.layout.mark_dirty(layout).expect("taffy dirty");
                        self.needs_layout = true;
                    }
                }
            }
            Prop::Value(value) => {
                if let Some(input) = node.input.as_mut() {
                    input.value_set = true;
                    if input.value != value {
                        input.value = value;
                        flags |= dirty::INPUT_VALUE;
                        self.measure_input(key);
                    }
                }
            }
            Prop::DefaultValue(value) => {
                if let Some(input) = node.input.as_mut()
                    && !input.value_set
                {
                    input.value_set = true;
                    input.value = value;
                    flags |= dirty::INPUT_VALUE;
                    self.measure_input(key);
                }
            }
            prop => {
                let Some(input) = node.input.as_mut() else {
                    return Ok(());
                };
                let c = &mut input.config;
                let remeasure = match prop {
                    Prop::Placeholder(text) => {
                        c.placeholder = text;
                        true
                    }
                    Prop::Multiline(multiline) => {
                        c.multiline = multiline;
                        true
                    }
                    Prop::PlaceholderColor(color) => {
                        c.placeholder_color = color;
                        false
                    }
                    Prop::Secure(secure) => {
                        c.secure = secure;
                        false
                    }
                    Prop::KeyboardType(keyboard) => {
                        c.keyboard = keyboard;
                        false
                    }
                    Prop::ReturnKeyType(key) => {
                        c.return_key = key;
                        false
                    }
                    Prop::AutoCapitalize(mode) => {
                        c.auto_capitalize = mode;
                        false
                    }
                    Prop::AutoCorrect(on) => {
                        c.auto_correct = on;
                        false
                    }
                    Prop::MaxLength(len) => {
                        c.max_length = len;
                        false
                    }
                    Prop::Editable(on) => {
                        c.editable = on;
                        false
                    }
                    Prop::BlurOnSubmit(on) => {
                        c.blur_on_submit = on;
                        false
                    }
                    Prop::AutoFocus(on) => {
                        c.auto_focus = on;
                        false
                    }
                    Prop::SelectionColor(color) => {
                        c.selection_color = color;
                        false
                    }
                    _ => return Ok(()),
                };
                flags |= dirty::INPUT;
                if remeasure {
                    self.measure_input(key);
                }
            }
        }
        if flags != 0 {
            self.mark(key, flags);
        }
        Ok(())
    }

    fn insert(
        &mut self,
        parent_id: NodeId,
        child_id: NodeId,
        before: Option<NodeId>,
    ) -> Result<(), ApplyError> {
        let parent = self.key(parent_id)?;
        let child = self.key(child_id)?;
        let before = before.map(|b| self.key(b)).transpose()?;
        if child == self.root || self.is_ancestor(child, parent) {
            return Err(ApplyError::Cycle(child_id));
        }
        if let Some(b) = before
            && (b == child || self.nodes[b as usize].parent != parent)
        {
            return Err(ApplyError::BadAnchor(self.nodes[b as usize].id));
        }

        // Detach from the current position. A native view that stays visible
        // is moved by the insert below, so only emit a remove when the new
        // parent has no native view.
        let parent_created = self.nodes[parent as usize].created;
        if self.nodes[child as usize].parent != NO_KEY {
            self.unlink(child, !parent_created);
        }

        let p = &self.nodes[parent as usize];
        let inline = p.kind == Kind::Text || p.inline;
        let pos = match before {
            Some(b) => p
                .children
                .iter()
                .position(|&c| c == b)
                .expect("anchor is a child"),
            None => p.children.len(),
        };
        self.set_inline(child, inline);
        self.nodes[child as usize].parent = parent;
        self.nodes[parent as usize].children.insert(pos, child);

        if inline {
            self.mark_text_block(parent);
            return Ok(());
        }
        // A cell joining a list: register its slot and derive its style
        // from the list orientation.
        if self.nodes[parent as usize].list.is_some() {
            if let Some(slot) = self.nodes[child as usize].slot {
                self.nodes[parent as usize]
                    .list
                    .as_mut()
                    .expect("list")
                    .add_slot_cell(slot, child);
            }
            self.sync_layout_style(child);
        }
        let child_kind = self.nodes[child as usize].kind;
        if child_kind == Kind::HeaderItem {
            self.mark(parent, dirty::SCREEN);
        }
        if let Some(child_layout) = self.nodes[child as usize].layout {
            // List cells, screens, modals and header items stay independent
            // layout roots.
            if self.nodes[parent as usize].list.is_none() && !child_kind.is_root() {
                let index = self.render_index(parent, pos);
                let parent_layout = self.nodes[parent as usize]
                    .layout
                    .expect("block parent has layout");
                self.layout
                    .insert_child_at_index(parent_layout, index as usize, child_layout)
                    .expect("taffy insert");
            }
            self.needs_layout = true;
            if parent_created {
                self.ensure_created(child);
                let before = self.native_before(parent, child);
                self.mount.push(MountItem::Insert {
                    parent: parent_id,
                    child: child_id,
                    before,
                });
            }
        }
        Ok(())
    }

    /// Detaches `key` from its parent (Remove op).
    fn detach(&mut self, key: Key) {
        self.unlink(key, true);
    }

    /// Unlinks `key` from its parent's children, layout and (optionally)
    /// native hierarchy.
    fn unlink(&mut self, key: Key, emit_remove: bool) {
        let parent = self.nodes[key as usize].parent;
        let siblings = &mut self.nodes[parent as usize].children;
        let pos = siblings
            .iter()
            .position(|&c| c == key)
            .expect("child of parent");
        siblings.remove(pos);
        if let Some(list) = self.nodes[parent as usize].list.as_mut() {
            list.remove_cell(key);
        }
        let node = &mut self.nodes[key as usize];
        node.parent = NO_KEY;
        let (inline, layout, created, id) = (node.inline, node.layout, node.created, node.id);
        if inline {
            self.mark_text_block(parent);
            return;
        }
        let kind = self.nodes[key as usize].kind;
        if kind == Kind::HeaderItem {
            self.mark(parent, dirty::SCREEN);
        }
        if let Some(layout) = layout {
            if self.nodes[parent as usize].list.is_none() && !kind.is_root() {
                let parent_layout = self.nodes[parent as usize]
                    .layout
                    .expect("block parent has layout");
                self.layout
                    .remove_child(parent_layout, layout)
                    .expect("taffy remove");
            }
            self.needs_layout = true;
            if emit_remove && created && self.nodes[parent as usize].created {
                self.mount.push(MountItem::Remove { id });
            }
        }
    }

    fn release(&mut self, id: NodeId) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        if key == self.root {
            return Ok(());
        }
        if self.nodes[key as usize].parent != NO_KEY {
            // Deleting the view also detaches it natively.
            self.unlink(key, false);
        }
        let node = self.nodes.remove(key as usize);
        self.slots[id as usize] = 0;
        if node.kind == Kind::List {
            self.lists.retain(|&k| k != key);
        }
        if node.kind.is_root() {
            self.roots.retain(|&k| k != key);
        }
        // Children are released by their own ops (the whole subtree became
        // unreachable in JS); orphan them so they never point at a dead key.
        for &c in &node.children {
            if let Some(child) = self.nodes.get_mut(c as usize) {
                child.parent = NO_KEY;
            }
        }
        if let Some(layout) = node.layout {
            self.layout.remove(layout).expect("taffy remove");
        }
        if node.created {
            self.mount.push(MountItem::Delete { id });
        }
        Ok(())
    }

    fn is_ancestor(&self, ancestor: Key, mut key: Key) -> bool {
        while key != NO_KEY {
            if key == ancestor {
                return true;
            }
            key = self.nodes[key as usize].parent;
        }
        false
    }

    /// The native sibling `child` goes below: native views are stacked by
    /// `zIndex`, then in tree order.
    fn native_before(&self, parent: Key, child: Key) -> Option<NodeId> {
        let children = &self.nodes[parent as usize].children;
        let z = self.nodes[child as usize].z_index;
        let pos = children.iter().position(|&c| c == child)?;
        let mut best: Option<(i32, usize, NodeId)> = None;
        for (i, &c) in children.iter().enumerate() {
            let n = &self.nodes[c as usize];
            if c == child || !n.created || n.layout.is_none() {
                continue;
            }
            if (n.z_index, i) > (z, pos) && best.is_none_or(|b| (n.z_index, i) < (b.0, b.1)) {
                best = Some((n.z_index, i, n.id));
            }
        }
        best.map(|b| b.2)
    }

    /// `key`'s `zIndex` changed: moves its native view among its siblings.
    fn restack(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (parent, id) = (node.parent, node.id);
        if parent == NO_KEY || !node.created || node.layout.is_none() || node.inline {
            return;
        }
        let p = &self.nodes[parent as usize];
        if !p.created || p.list.is_some() {
            return;
        }
        let before = self.native_before(parent, key);
        self.mount.push(MountItem::Insert {
            parent: p.id,
            child: id,
            before,
        });
    }

    /// `allowFontScaling` / `maxFontSizeMultiplier` changed on `key`.
    fn font_settings_changed(&mut self, key: Key) {
        match self.nodes[key as usize].kind {
            Kind::Text => self.mark_text_block(key),
            Kind::Input => {
                self.measure_input(key);
                self.mark(key, dirty::INPUT);
            }
            _ => {}
        }
    }

    /// Index among rendering (layout-backed) children of `parent` for a
    /// child at `pos` in the full children list.
    fn render_index(&self, parent: Key, pos: usize) -> u32 {
        self.nodes[parent as usize].children[..pos]
            .iter()
            .map(|&c| &self.nodes[c as usize])
            .filter(|c| c.layout.is_some() && !c.kind.is_root())
            .count() as u32
    }

    /// Moves `key` (and its subtree) into or out of an inline context.
    fn set_inline(&mut self, key: Key, inline: bool) {
        let node = &mut self.nodes[key as usize];
        if node.inline == inline {
            return;
        }
        node.inline = inline;
        if node.kind != Kind::Text {
            // Other elements inside text are not supported; they are ignored.
            return;
        }
        if inline {
            // Block -> span: spans have no layout node or native view.
            if let Some(l) = node.layout.take() {
                self.layout.remove(l).expect("taffy remove");
            }
            if node.created {
                node.created = false;
                self.mount.push(MountItem::Delete { id: node.id });
            }
        } else {
            let style = node.base_layout.as_deref().cloned().unwrap_or_default();
            let ctx = LeafContext::Text(TextContext::new(Arc::default()));
            node.layout = Some(
                self.layout
                    .new_leaf_with_context(style, ctx)
                    .expect("taffy leaf"),
            );
            self.mark(key, dirty::TEXT_BUILD);
        }
    }

    /// Creates the native views of `key`'s subtree if needed, wiring up the
    /// children in order.
    fn ensure_created(&mut self, key: Key) {
        let node = &mut self.nodes[key as usize];
        if node.created || node.layout.is_none() {
            return;
        }
        node.created = true;
        node.frame = Rect::UNSET;
        if let Some(scroll) = node.scroll.as_mut() {
            scroll.content = None;
        }
        let id = node.id;
        let kind = node.kind;
        // A new native view already has the default style and no listeners.
        let mut flags = 0;
        if node.view_style != ViewStyle::default() {
            flags |= dirty::VIEW;
        }
        if !node.events.is_empty() {
            flags |= dirty::EVENTS;
        }
        if kind == Kind::Text {
            flags |= dirty::TEXT_MOUNT;
        }
        if node.image.as_ref().is_some_and(|i| i.source.is_some()) {
            flags |= dirty::IMAGE;
        }
        if let Some(scroll) = &node.scroll {
            flags |= dirty::SCROLL;
            if scroll.refreshing {
                flags |= dirty::REFRESHING;
            }
        }
        if !node.animated.is_empty() {
            flags |= dirty::ANIM;
        }
        if !node.gestures.is_empty() {
            flags |= dirty::PAN;
        }
        if node.props.is_some() {
            flags |= dirty::VIEW_PROPS;
        }
        match kind {
            Kind::Screen => flags |= dirty::SCREEN,
            Kind::Modal => flags |= dirty::MODAL,
            _ => {}
        }
        if node.control.is_some() {
            flags |= dirty::NATIVE;
        }
        if let Some(list) = node.list.as_mut() {
            list.dirty = true;
        }
        if let Some(input) = node.input.as_mut() {
            input.content = None;
            flags |= dirty::INPUT;
            if !input.value.is_empty() {
                flags |= dirty::INPUT_VALUE;
            }
        }
        self.mount.push(MountItem::Create {
            id,
            kind: kind.view_kind(),
        });
        self.mark(key, flags);
        if !kind.is_container() {
            return;
        }
        // Bottom to top: by `zIndex`, then in order.
        let mut children = std::mem::take(&mut self.nodes[key as usize].children);
        let original = children.clone();
        if children
            .iter()
            .any(|&c| self.nodes[c as usize].z_index != 0)
        {
            children.sort_by_key(|&c| self.nodes[c as usize].z_index);
        }
        for &child in &children {
            if self.nodes[child as usize].layout.is_some() {
                self.ensure_created(child);
                let child_id = self.nodes[child as usize].id;
                self.mount.push(MountItem::Insert {
                    parent: id,
                    child: child_id,
                    before: None,
                });
            }
        }
        self.nodes[key as usize].children = original;
    }

    #[inline]
    fn mark(&mut self, key: Key, flags: u32) {
        let node = &mut self.nodes[key as usize];
        node.dirty |= flags;
        if !node.queued {
            node.queued = true;
            self.queue.push(key);
        }
    }

    /// Marks the text block containing `key` (or `key` itself) for rebuild.
    fn mark_text_block(&mut self, mut key: Key) {
        loop {
            let node = &self.nodes[key as usize];
            if !node.inline {
                if node.kind == Kind::Text {
                    self.mark(key, dirty::TEXT_BUILD);
                }
                return;
            }
            if node.parent == NO_KEY {
                // A detached span: rebuilt when it is inserted into a block.
                return;
            }
            key = node.parent;
        }
    }

    fn flush_dirty(&mut self) {
        let mut queue = std::mem::take(&mut self.queue);
        for &key in &queue {
            let Some(node) = self.nodes.get_mut(key as usize) else {
                continue;
            };
            let mut flags = std::mem::take(&mut node.dirty);
            node.queued = false;
            if flags & dirty::TEXT_BUILD != 0 && node.layout.is_some() && self.rebuild_text(key) {
                flags |= dirty::TEXT_MOUNT;
            }
            let node = &self.nodes[key as usize];
            if !node.created {
                continue;
            }
            let id = node.id;
            if flags & dirty::VIEW != 0 {
                self.mount.push(MountItem::ViewStyle {
                    id,
                    style: Box::new(node.view_style),
                });
            }
            if flags & dirty::EVENTS != 0 {
                self.mount.push(MountItem::Events {
                    id,
                    events: node.events,
                });
            }
            if flags & dirty::IMAGE != 0
                && let Some(image) = &node.image
            {
                self.mount.push(MountItem::Image {
                    id,
                    source: image.source.clone(),
                    resize_mode: image.resize_mode,
                });
            }
            if let Some(scroll) = &node.scroll {
                if flags & dirty::SCROLL != 0 {
                    self.mount.push(MountItem::ScrollConfig {
                        id,
                        horizontal: scroll.horizontal,
                        shows_indicator: scroll.shows_indicator,
                        scroll_value: scroll.scroll_value,
                        keyboard_dismiss: scroll.keyboard_dismiss,
                    });
                }
                if flags & dirty::SCROLL_BEHAVIOR != 0 {
                    self.mount.push(MountItem::ScrollBehavior {
                        id,
                        behavior: Box::new(scroll.behavior.clone()),
                    });
                }
                if flags & dirty::REFRESHING != 0 {
                    self.mount.push(MountItem::Refreshing {
                        id,
                        refreshing: scroll.refreshing,
                    });
                }
            }
            if flags & dirty::ANIM != 0 {
                self.mount.push(MountItem::AnimBindings {
                    id,
                    bindings: node.animated.clone(),
                });
            }
            if flags & dirty::PAN != 0 {
                self.mount.push(MountItem::Gestures {
                    id,
                    values: node.gestures,
                });
            }
            if flags & dirty::VIEW_PROPS != 0 {
                self.mount.push(MountItem::ViewProps {
                    id,
                    props: node.props.clone().unwrap_or_default(),
                });
            }
            if flags & dirty::SCREEN != 0
                && let Some(config) = self.screen_config(key)
            {
                self.mount.push(MountItem::ScreenConfig {
                    id,
                    config: Box::new(config),
                });
            }
            let node = &self.nodes[key as usize];
            if flags & dirty::NATIVE != 0
                && let Some(control) = &node.control
            {
                self.mount.push(MountItem::NativeProps {
                    id,
                    props: control.props.clone(),
                });
            }
            if flags & dirty::MODAL != 0
                && let Some(root) = &node.root
            {
                self.mount.push(MountItem::ModalConfig {
                    id,
                    config: root.modal,
                });
            }
            if let Some(input) = &node.input {
                if flags & dirty::INPUT != 0 {
                    let mut config = input.config.clone();
                    config.style = scaled(
                        ResolvedTextStyle::default().inherit(&node.text_style),
                        node.paragraph.font_multiplier(self.font_scale),
                    );
                    config.align = node.text_style.text_align.unwrap_or_default();
                    self.mount.push(MountItem::Input {
                        id,
                        config: Box::new(config),
                    });
                }
                if flags & dirty::INPUT_VALUE != 0 {
                    self.mount.push(MountItem::InputValue {
                        id,
                        text: input.value.clone(),
                        count: input.count,
                    });
                }
            }
            if flags & dirty::TEXT_MOUNT != 0 {
                self.queue_text(key);
            }
        }
        queue.clear();
        // Keep the allocation; nothing is queued while flushing.
        debug_assert!(self.queue.is_empty());
        self.queue = queue;
    }

    fn text_ctx(&mut self, layout: LayoutId) -> &mut TextContext<M::Prepared> {
        match self.layout.get_node_context_mut(layout) {
            Some(LeafContext::Text(ctx)) => ctx,
            _ => unreachable!("text block without text context"),
        }
    }

    /// Rebuilds the attributed text of a block. Returns true if it changed.
    ///
    /// Only layout-relevant changes invalidate measurement and layout; a
    /// color change keeps both and just needs a new final layout.
    fn rebuild_text(&mut self, key: Key) -> bool {
        let node = &self.nodes[key as usize];
        let mut text = AttributedText {
            align: node.text_style.text_align.unwrap_or_default(),
            max_lines: node.paragraph.lines,
            ellipsize: node.paragraph.ellipsize,
            shadow: node.text_style.shadow(),
            ..AttributedText::default()
        };
        let base = ResolvedTextStyle::default().inherit(&node.text_style);
        self.collect_text(key, base, &mut text);
        let multiplier = node.paragraph.font_multiplier(self.font_scale);
        if multiplier != 1.0 {
            for span in &mut text.spans {
                span.style = scaled(span.style, multiplier);
            }
        }
        let layout = node.layout.expect("text block has layout");
        let ctx = self.text_ctx(layout);
        if *ctx.text == text {
            return false;
        }
        let same_layout = ctx.text.layout_eq(&text);
        ctx.text = Arc::new(text);
        // Platform typesetting state embeds colors too: always rebuild it.
        ctx.prepared = None;
        ctx.laid_out = None;
        if !same_layout {
            ctx.layout_hash = ctx.text.layout_hash();
            ctx.cache = None;
            self.layout.mark_dirty(layout).expect("taffy dirty");
            self.needs_layout = true;
        }
        true
    }

    fn collect_text(&self, key: Key, style: ResolvedTextStyle, out: &mut AttributedText) {
        for &c in &self.nodes[key as usize].children {
            let child = &self.nodes[c as usize];
            match child.kind {
                Kind::RawText => out.push(&child.text, style),
                Kind::Text => self.collect_text(c, style.inherit(&child.text_style), out),
                _ => {}
            }
        }
    }

    /// Lays out the main tree if it changed. Returns true if it did.
    fn layout_main(&mut self) -> bool {
        let root = self.nodes[self.root as usize].layout.expect("root layout");
        let available = taffy::Size {
            width: AvailableSpace::Definite(self.viewport.width),
            height: AvailableSpace::Definite(self.viewport.height),
        };
        // Even a clean root costs a full rounding walk in taffy: skip it.
        let main = self.layout.dirty(root).unwrap_or(true);
        if main {
            self.layout_root(root, available);
        }
        main
    }

    /// Lays out screens, modals and header items whose size or content
    /// changed, outer ones first (a stack may live in a modal), and emits
    /// their frames.
    fn layout_roots(&mut self) {
        if self.roots.is_empty() {
            return;
        }
        let mut order: Vec<(u32, Key)> = self
            .roots
            .iter()
            .filter(|&&k| {
                let n = &self.nodes[k as usize];
                n.created && n.parent != NO_KEY
            })
            .map(|&k| (self.root_depth(k), k))
            .collect();
        order.sort_unstable();
        for (_, key) in order {
            self.layout_detached(key);
        }
    }

    /// Number of screens / modals / header items above `key`.
    fn root_depth(&self, mut key: Key) -> u32 {
        let mut depth = 0;
        key = self.nodes[key as usize].parent;
        while key != NO_KEY {
            let n = &self.nodes[key as usize];
            if n.kind.is_root() {
                depth += 1;
            }
            key = n.parent;
        }
        depth
    }

    /// How far the system UI at the top (status bar) reaches into `key`:
    /// its root's top inset minus its offset in that root.
    fn top_overlap(&self, key: Key) -> f32 {
        let mut y = 0.0;
        let mut k = key;
        loop {
            let n = &self.nodes[k as usize];
            if k == self.root {
                return (self.safe_area.top - y).max(0.0);
            }
            if n.kind.is_root() {
                let inset = match n.kind {
                    Kind::Modal => n.root.as_ref().map_or(0.0, |r| r.insets.top),
                    _ => 0.0,
                };
                return (inset - y).max(0.0);
            }
            if !n.frame.y.is_nan() {
                y += n.frame.y;
            }
            if n.parent == NO_KEY {
                return 0.0;
            }
            k = n.parent;
        }
    }

    fn layout_detached(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (Some(layout), kind, parent) = (node.layout, node.kind, node.parent) else {
            return;
        };
        let root = node.root.as_ref().expect("root state");
        let (size, header, available) = match kind {
            Kind::Screen => {
                let p = &self.nodes[parent as usize];
                let Some(stack_layout) = p.layout.filter(|_| p.kind == Kind::Stack) else {
                    return;
                };
                let s = self.layout.layout(stack_layout).expect("taffy layout").size;
                let c = &root.screen;
                let header = if c.header_shown {
                    let bar = if c.large_title {
                        self.header_heights.1
                    } else {
                        self.header_heights.0
                    };
                    bar + self.top_overlap(parent)
                } else {
                    0.0
                };
                let size = Size::new(s.width, s.height);
                (size, header, definite(size))
            }
            Kind::Modal => {
                let size = root.reported.unwrap_or(self.viewport);
                (size, 0.0, definite(size))
            }
            _ => (
                Size::default(),
                0.0,
                taffy::Size {
                    width: AvailableSpace::MaxContent,
                    height: AvailableSpace::MaxContent,
                },
            ),
        };
        let changed = root.laid_out != Some((size, header));
        if !changed && !self.layout.dirty(layout).unwrap_or(true) {
            return;
        }
        if kind != Kind::HeaderItem {
            let config_padding = match kind {
                // Large titles and transparent headers lay the content out
                // under the header (scroll views inset themselves natively).
                Kind::Screen if root.screen.header_transparent || root.screen.large_title => 0.0,
                _ => header,
            };
            let mut style = self.nodes[key as usize]
                .base_layout
                .as_deref()
                .cloned()
                .unwrap_or_default();
            style.size = taffy::Size {
                width: taffy::Dimension::length(size.width),
                height: taffy::Dimension::length(size.height),
            };
            if config_padding > 0.0 {
                let base = match style.padding.top.into_raw() {
                    raw if raw.tag() == taffy::CompactLength::LENGTH_TAG => raw.value(),
                    _ => 0.0,
                };
                style.padding.top = taffy::LengthPercentage::length(base + config_padding);
            }
            self.layout.set_style(layout, style).expect("taffy style");
        }
        let header_changed = root.laid_out.is_none_or(|(_, h)| h != header);
        self.nodes[key as usize]
            .root
            .as_mut()
            .expect("root state")
            .laid_out = Some((size, header));
        self.layout_root(layout, available);
        self.emit_frames(key);
        if kind == Kind::Screen && header_changed {
            let id = self.nodes[key as usize].id;
            if let Some(config) = self.screen_config(key) {
                self.mount.push(MountItem::ScreenConfig {
                    id,
                    config: Box::new(config),
                });
            }
        }
    }

    /// The options of screen `key`, with its header items.
    fn screen_config(&self, key: Key) -> Option<ScreenConfig> {
        let node = &self.nodes[key as usize];
        let root = node.root.as_ref().filter(|_| node.kind == Kind::Screen)?;
        let mut config = root.screen.clone();
        config.header_height = root.laid_out.map_or(0.0, |(_, h)| h);
        for &c in &node.children {
            let child = &self.nodes[c as usize];
            if child.kind != Kind::HeaderItem || !child.created {
                continue;
            }
            let slot = child.root.as_ref().map_or(HeaderSlot::Left, |r| r.slot);
            let target = match slot {
                HeaderSlot::Left => &mut config.left,
                HeaderSlot::Right => &mut config.right,
                HeaderSlot::Title => &mut config.title_view,
            };
            *target = Some(child.id);
        }
        Some(config)
    }

    /// Lays out the list cells that changed: items at their column's
    /// width, the header, footer and empty placeholder at the content's.
    fn layout_cells(&mut self) {
        for i in 0..self.lists.len() {
            let key = self.lists[i];
            let list = &self.nodes[key as usize];
            let Some(list_layout) = list.layout else {
                continue;
            };
            let state = list.list.as_ref().expect("list");
            let horizontal = state.horizontal;
            let size = self.layout.layout(list_layout).expect("taffy layout").size;
            let cross = if horizontal { size.height } else { size.width };
            let item_cross = state.column(cross, 0).1;
            let extra_cross = state.full_width(cross).1;
            let available = |c: f32| {
                if horizontal {
                    taffy::Size {
                        width: AvailableSpace::MaxContent,
                        height: AvailableSpace::Definite(c),
                    }
                } else {
                    taffy::Size {
                        width: AvailableSpace::Definite(c),
                        height: AvailableSpace::MaxContent,
                    }
                }
            };
            // Laying out even a clean root costs a tree walk (taffy rounds the
            // whole subtree), so only dirty cells are laid out again — all
            // of them when the list's cross size or columns changed.
            let list_state = self.nodes[key as usize].list.as_mut().expect("list");
            let resized = list_state.cells_cross != cross;
            list_state.cells_cross = cross;
            let cells = std::mem::take(&mut self.nodes[key as usize].children);
            for &cell in &cells {
                let node = &self.nodes[cell as usize];
                let Some(cell_layout) = node.layout else {
                    continue;
                };
                let c = match node.slot {
                    Some(CellSlot::Item(_)) | None => item_cross,
                    Some(_) => extra_cross,
                };
                if resized || self.layout.dirty(cell_layout).unwrap_or(true) {
                    self.layout_root(cell_layout, available(c));
                    self.nodes[cell as usize].relaid = true;
                }
            }
            self.nodes[key as usize].children = cells;
        }
    }

    /// Lays out the tree under `root` within `available`.
    fn layout_root(&mut self, root: LayoutId, available: taffy::Size<AvailableSpace>) {
        let engine = &self.text_engine;
        let cache = &mut self.measure_cache;
        self.layout
            .compute_layout_with_measure(root, available, |input, _, ctx, style| {
                // Taffy resolves style sizes, padding, border, min/max and
                // aspect ratio; only content is measured here.
                taffy::compute_leaf_layout(
                    input,
                    style,
                    |_, _| 0.0,
                    |known, available| match ctx {
                        Some(LeafContext::Text(ctx)) => {
                            let max_width = known.width.or(match available.width {
                                AvailableSpace::Definite(w) => Some(w),
                                // Text has no cheap min-content; use the natural width.
                                AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
                            });
                            let measured = measure_text(engine, cache, ctx, max_width);
                            taffy::Size {
                                width: known.width.unwrap_or(measured.width),
                                height: known.height.unwrap_or(measured.height),
                            }
                        }
                        Some(LeafContext::Image(Some(natural))) => {
                            image_size(*natural, known.width, known.height)
                        }
                        Some(LeafContext::Fixed(size)) => taffy::Size {
                            width: known.width.unwrap_or(size.width),
                            height: known.height.unwrap_or(size.height),
                        },
                        _ => taffy::Size::ZERO,
                    },
                )
            })
            .expect("taffy layout");
    }

    #[inline]
    fn queue_text(&mut self, key: Key) {
        let node = &mut self.nodes[key as usize];
        if !node.text_queued {
            node.text_queued = true;
            self.pending_text.push(key);
        }
    }

    /// Makes the final layout of every queued text block whose content or
    /// box changed, and emits it.
    fn flush_text(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_text);
        for &key in &pending {
            let Some(node) = self.nodes.get_mut(key as usize) else {
                continue;
            };
            node.text_queued = false;
            let (id, created, Some(layout)) = (node.id, node.created, node.layout) else {
                continue;
            };
            if !created {
                continue;
            }
            let content = content_box(self.layout.layout(layout).expect("taffy layout"));
            let size = Size::new(content.width, content.height);
            let engine = &self.text_engine;
            let Some(LeafContext::Text(ctx)) = self.layout.get_node_context_mut(layout) else {
                continue;
            };
            if matches!(&ctx.laid_out, Some((s, _)) if *s == size) {
                continue;
            }
            let prepared = ctx
                .prepared
                .get_or_insert_with(|| engine.prepare(&ctx.text));
            let text_layout = engine.layout(prepared, size);
            ctx.laid_out = Some((size, text_layout.clone()));
            self.mount.push(MountItem::Text {
                id,
                text: ctx.text.clone(),
                layout: text_layout,
                content,
            });
        }
        pending.clear();
        self.pending_text = pending;
    }

    /// Emits the frame of `key` and its subtree (if changed).
    fn emit_frames(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let Some(layout) = node.layout else { return };
        if !node.created {
            return;
        }
        let l = self.layout.layout(layout).expect("taffy layout");
        let frame = Rect {
            x: l.location.x,
            y: l.location.y,
            width: l.size.width,
            height: l.size.height,
        };
        self.set_frame(key, frame);
        let kind = self.nodes[key as usize].kind;
        if kind == Kind::Input {
            let content = content_box(self.layout.layout(layout).expect("taffy layout"));
            let node = &mut self.nodes[key as usize];
            let input = node.input.as_mut().expect("input state");
            if input.content != Some(content) {
                input.content = Some(content);
                self.mount.push(MountItem::InputContent {
                    id: node.id,
                    content,
                });
            }
        }
        if kind == Kind::Scroll {
            // Scrollable extent: the overflow rectangle is measured from the
            // padding box origin and stops at the children's border boxes, so
            // add the leading border and trailing padding.
            let l = self.layout.layout(layout).expect("taffy layout");
            let o = l.scrollable_overflow_rect;
            let size = Size::new(
                l.size.width.max(l.border.left + o.right + l.padding.right),
                l.size
                    .height
                    .max(l.border.top + o.bottom + l.padding.bottom),
            );
            self.set_scroll_content(key, size);
        }
        // List cells are positioned by `update_lists`, roots by
        // `layout_roots`.
        if kind.is_container() && kind != Kind::List {
            let children = std::mem::take(&mut self.nodes[key as usize].children);
            for &child in &children {
                if !self.nodes[child as usize].kind.is_root() {
                    self.emit_frames(child);
                }
            }
            self.nodes[key as usize].children = children;
        }
    }

    fn set_frame(&mut self, key: Key, frame: Rect) {
        let node = &mut self.nodes[key as usize];
        if node.frame != frame {
            node.frame = frame;
            let (id, is_text) = (node.id, node.kind == Kind::Text);
            if node.events.contains(EventKind::Layout) {
                self.events.push((
                    id,
                    EventKind::Layout,
                    EventPayload::Layout {
                        x: frame.x,
                        y: frame.y,
                        width: frame.width,
                        height: frame.height,
                    },
                ));
            }
            self.mount.push(MountItem::Frame { id, frame });
            if is_text {
                self.queue_text(key);
            }
        }
    }

    fn set_scroll_content(&mut self, key: Key, size: Size) {
        let node = &mut self.nodes[key as usize];
        let id = node.id;
        if let Some(scroll) = node.scroll.as_mut()
            && scroll.content != Some(size)
        {
            scroll.content = Some(size);
            self.mount.push(MountItem::ScrollContent { id, size });
        }
    }

    /// Measures rendered cells, keeps the visible content anchored, picks
    /// the items to render and positions the cells of every list.
    fn update_lists(&mut self, relayout: bool) {
        for i in 0..self.lists.len() {
            let key = self.lists[i];
            let node = &self.nodes[key as usize];
            if !node.created {
                continue;
            }
            let list = node.list.as_ref().expect("list");
            if !(relayout || list.dirty || self.commands.iter().any(|c| c.0 == node.id)) {
                continue;
            }
            let (id, events) = (node.id, node.events);
            let horizontal = list.horizontal;
            let l = *self
                .layout
                .layout(node.layout.expect("list layout"))
                .expect("taffy layout");
            let (main, cross) = if horizontal {
                (l.size.width, l.size.height)
            } else {
                (l.size.height, l.size.width)
            };
            let main_of = |s: taffy::Size<f32>| if horizontal { s.width } else { s.height };

            // Sizes of cells showing their item, and of the extras.
            let mut ready = std::mem::take(&mut self.cells_scratch);
            ready.clear();
            ready.extend(
                list.slots
                    .iter()
                    .filter_map(|s| s.ready().map(|i| (i, s.cell))),
            );
            let extras = list.extras;
            let mut anchor = 0.0;
            for &(index, cell) in &ready {
                if let Some(cell_layout) = self.nodes.get(cell as usize).and_then(|c| c.layout) {
                    let size = main_of(self.layout.layout(cell_layout).expect("taffy layout").size);
                    let list = self.nodes[key as usize].list.as_mut().expect("list");
                    anchor += list.measured(index, size);
                }
            }
            for (kind, extra) in [ExtraKind::Header, ExtraKind::Footer, ExtraKind::Empty]
                .into_iter()
                .zip(extras)
            {
                if let Some(cell_layout) =
                    self.nodes.get(extra.cell as usize).and_then(|c| c.layout)
                {
                    let size = main_of(self.layout.layout(cell_layout).expect("taffy layout").size);
                    let list = self.nodes[key as usize].list.as_mut().expect("list");
                    anchor += list.measured_extra(kind, size);
                }
            }

            let list = self.nodes[key as usize].list.as_mut().expect("list");
            // Not on the first layout: there is nothing to clamp yet.
            let grew = list.viewport > 0.0 && main > list.viewport;
            list.viewport = main;
            list.offset += anchor;
            // A splice above the viewport already moved the offset.
            anchor += list.take_splice_anchor();
            // Inverted lists keep their distance from the end instead.
            anchor += list.anchor_end();

            // Scroll commands for the list, now that sizes are known: the
            // window follows a jump in this same commit.
            for (target, command) in &mut self.commands {
                if *target != id {
                    continue;
                }
                let list = self.nodes[key as usize].list.as_mut().expect("list");
                let (offset, animated) = match *command {
                    Command::ScrollToIndex {
                        index,
                        animated,
                        view_position,
                        view_offset,
                    } => (
                        list.offset_for_index(index, view_position, view_offset),
                        animated,
                    ),
                    Command::ScrollToEnd { animated } => (list.end_offset(), animated),
                    Command::ScrollTo { x, y, animated } => {
                        (if horizontal { x } else { y }, animated)
                    }
                    _ => continue,
                };
                if !animated {
                    list.jump(offset);
                }
                list.issue_scroll_to(offset);
                let (x, y) = if horizontal {
                    (offset, 0.0)
                } else {
                    (0.0, offset)
                };
                *command = Command::ScrollTo { x, y, animated };
            }

            let list = self.nodes[key as usize].list.as_mut().expect("list");
            let mut binds = Vec::new();
            if list.update_window(&mut binds) {
                self.events
                    .push((id, EventKind::EndReached, EventPayload::None));
            }
            if !binds.is_empty() {
                self.list_binds.push(ListBind { list: id, binds });
            }
            if events.contains(EventKind::ViewableItemsChanged) {
                let viewable = list.viewable_items();
                if viewable != list.viewable {
                    list.viewable = viewable;
                    let (first, last) =
                        viewable.map_or((-1.0, -1.0), |(f, l)| (f as f64, l as f64));
                    self.events.push((
                        id,
                        EventKind::ViewableItemsChanged,
                        EventPayload::Map(vec![
                            ("first", PayloadValue::Number(first)),
                            ("last", PayloadValue::Number(last)),
                        ]),
                    ));
                }
            }

            // Position cells that show their item (the window may have
            // changed which ones). Descendant frames only change when the
            // cell was laid out again.
            let list = self.nodes[key as usize].list.as_ref().expect("list");
            let content_len = list.content_len();
            let blocks = list.blocks();
            let sticky = list.sticky_enabled();
            ready.clear();
            ready.extend(
                list.slots
                    .iter()
                    .filter_map(|s| s.ready().map(|i| (i, s.cell))),
            );
            // Cells left without an item still show their last one. Those
            // in the render window (the data shrank, the list jumped) would
            // show through: move them out of sight. Cells the window left
            // behind while scrolling are out of sight already.
            let (window_start, window_end) = list.render_window();
            let hidden: Vec<Key> = list
                .slots
                .iter()
                .filter(|s| s.index.is_none() && s.cell != u32::MAX)
                .map(|s| s.cell)
                .collect();
            for cell in hidden {
                let frame = self.nodes[cell as usize].frame;
                if frame.width.is_nan() {
                    // Never placed: nothing to see yet.
                    continue;
                }
                let (start, size) = if horizontal {
                    (frame.x, frame.width)
                } else {
                    (frame.y, frame.height)
                };
                if start + size <= window_start || start >= window_end {
                    continue;
                }
                let away = if horizontal {
                    Rect {
                        x: -HIDDEN,
                        ..frame
                    }
                } else {
                    Rect {
                        y: -HIDDEN,
                        ..frame
                    }
                };
                if frame != away {
                    self.set_frame(cell, away);
                }
            }
            let mut sticky_cells = Vec::new();
            let place = |this: &mut Self, cell: Key, at: f32, row: f32, (x, width): (f32, f32)| {
                let cell_layout = this.nodes.get(cell as usize).and_then(|c| c.layout)?;
                let size = this.layout.layout(cell_layout).expect("taffy layout").size;
                let list = this.nodes[key as usize].list.as_ref().expect("list");
                let main_at = list.to_content(at, row.max(main_of(size)));
                let frame = if horizontal {
                    Rect {
                        x: main_at,
                        y: x,
                        width: size.width,
                        height: width,
                    }
                } else {
                    Rect {
                        x,
                        y: main_at,
                        width,
                        height: size.height,
                    }
                };
                this.set_frame(cell, frame);
                if std::mem::take(&mut this.nodes[cell as usize].relaid) {
                    let children = std::mem::take(&mut this.nodes[cell as usize].children);
                    for &child in &children {
                        this.emit_frames(child);
                    }
                    this.nodes[cell as usize].children = children;
                }
                Some(main_of(size))
            };
            for &(index, cell) in &ready {
                let list = self.nodes[key as usize].list.as_ref().expect("list");
                let (at, row) = (list.item_at(index), list.row_extent(index));
                let column = list.column(cross, list.column_of(index));
                let Some(size) = place(self, cell, at, row, column) else {
                    continue;
                };
                let list = self.nodes[key as usize].list.as_ref().expect("list");
                if sticky && list.sticky.binary_search(&index).is_ok() {
                    let range = list.sticky_range(index, self.nodes[cell as usize].id, size);
                    sticky_cells.push(range);
                }
            }
            let full = self.nodes[key as usize]
                .list
                .as_ref()
                .expect("list")
                .full_width(cross);
            for (extra, at) in extras
                .iter()
                .zip([blocks.header, blocks.footer, blocks.body])
            {
                if extra.cell != u32::MAX {
                    place(self, extra.cell, at, 0.0, full);
                }
            }
            self.cells_scratch = ready;

            sticky_cells.sort_by_key(|c| c.cell);
            let list = self.nodes[key as usize].list.as_mut().expect("list");
            if sticky_cells != list.sent_sticky {
                let cells = sticky_cells
                    .iter()
                    .map(|r| StickyCell {
                        cell: r.cell,
                        start: r.start,
                        end: r.end,
                    })
                    .collect();
                list.sent_sticky = sticky_cells;
                self.mount.push(MountItem::StickyCells { id, cells });
            }

            let content = if horizontal {
                Size::new(content_len, cross)
            } else {
                Size::new(cross, content_len)
            };
            // Platforms clamp the offset to the content: grow the content
            // before scrolling, scroll before shrinking it.
            let old_len = self.nodes[key as usize]
                .scroll
                .as_ref()
                .and_then(|s| s.content)
                .map_or(0.0, |c| if horizontal { c.width } else { c.height });
            if anchor != 0.0 {
                self.nodes[key as usize]
                    .list
                    .as_mut()
                    .expect("list")
                    .issue_scroll_by(anchor);
            }
            let scroll_by = (anchor != 0.0).then(|| {
                let (dx, dy) = if horizontal {
                    (anchor, 0.0)
                } else {
                    (0.0, anchor)
                };
                MountItem::ScrollBy { id, dx, dy }
            });
            // Platforms clamp the offset to the content and the viewport:
            // scroll before the viewport grows or the content shrinks, and
            // after the content grows (a shrinking viewport's frame comes
            // first anyway).
            let frame_at = grew
                .then(|| {
                    self.mount
                        .iter()
                        .position(|m| matches!(m, MountItem::Frame { id: f, .. } if *f == id))
                })
                .flatten();
            match (scroll_by, frame_at) {
                (Some(item), Some(at)) => {
                    self.mount.insert(at, item);
                    self.set_scroll_content(key, content);
                }
                (Some(item), None) if content_len < old_len => {
                    self.mount.push(item);
                    self.set_scroll_content(key, content);
                }
                (item, _) => {
                    self.set_scroll_content(key, content);
                    self.mount.extend(item);
                }
            }
        }
    }
}

/// How far out of sight unbound list cells are moved.
const HIDDEN: f32 = 1.0e6;

/// The content box (inside padding and border) of a laid-out node.
fn content_box(l: &taffy::Layout) -> Rect {
    Rect {
        x: l.padding.left + l.border.left,
        y: l.padding.top + l.border.top,
        width: (l.size.width - l.padding.left - l.padding.right - l.border.left - l.border.right)
            .max(0.0),
        height: (l.size.height - l.padding.top - l.padding.bottom - l.border.top - l.border.bottom)
            .max(0.0),
    }
}

/// Size of an image with a known natural size: fill a known dimension and
/// keep the aspect ratio for the other.
fn image_size(natural: Size, w: Option<f32>, h: Option<f32>) -> taffy::Size<f32> {
    let ratio = if natural.height > 0.0 {
        natural.width / natural.height
    } else {
        1.0
    };
    let (width, height) = match (w, h) {
        (Some(w), None) => (w, w / ratio),
        (None, Some(h)) => (h * ratio, h),
        (Some(w), Some(h)) => (w, h),
        (None, None) => (natural.width, natural.height),
    };
    taffy::Size { width, height }
}

/// Measures a text block: per-node cache, then the shared content-addressed
/// cache, then the platform engine.
fn measure_text<M: TextEngine>(
    engine: &M,
    cache: &mut MeasureCache,
    ctx: &mut TextContext<M::Prepared>,
    max_width: Option<f32>,
) -> Size {
    if let Some((w, size)) = ctx.cache
        && w == max_width
    {
        return size;
    }
    let size = match cache.get(ctx.layout_hash, max_width) {
        Some(size) => size,
        None => {
            let prepared = ctx
                .prepared
                .get_or_insert_with(|| engine.prepare(&ctx.text));
            let size = engine.measure(prepared, max_width);
            // Round up so rendering never truncates the last glyph.
            let size = Size::new(size.width.ceil(), size.height.ceil());
            cache.insert(ctx.layout_hash, max_width, size);
            size
        }
    };
    ctx.cache = Some((max_width, size));
    size
}

fn definite(size: Size) -> taffy::Size<AvailableSpace> {
    taffy::Size {
        width: AvailableSpace::Definite(size.width),
        height: AvailableSpace::Definite(size.height),
    }
}

/// Applies one property to a control. Returns its new natural size when it
/// changed (activity indicator sizes).
fn apply_native(control: &mut ControlState, prop: NativeProp, sizes: ControlSizes) -> Option<Size> {
    use NativeProp as P;
    match (&mut control.props, prop) {
        (NativeProps::Switch(p), P::SwitchValue(v)) => {
            p.value = v;
            control.js_value = if v { 1.0 } else { 0.0 };
            control.js_set = true;
        }
        (NativeProps::Switch(p), P::Disabled(v)) => p.disabled = v,
        (NativeProps::Switch(p), P::TrackOn(c)) => p.track_on = c,
        (NativeProps::Switch(p), P::TrackOff(c)) => p.track_off = c,
        (NativeProps::Switch(p), P::Thumb(c)) => p.thumb = c,
        (NativeProps::Slider(p), P::SliderValue(v)) => {
            p.value = v;
            control.js_value = v;
            control.js_set = true;
        }
        (NativeProps::Slider(p), P::Minimum(v)) => p.min = v,
        (NativeProps::Slider(p), P::Maximum(v)) => p.max = v,
        (NativeProps::Slider(p), P::Step(v)) => p.step = v.max(0.0),
        (NativeProps::Slider(p), P::Disabled(v)) => p.disabled = v,
        (NativeProps::Slider(p), P::TrackOn(c)) => p.min_track = c,
        (NativeProps::Slider(p), P::TrackOff(c)) => p.max_track = c,
        (NativeProps::Slider(p), P::Thumb(c)) => p.thumb = c,
        (NativeProps::Activity(p), P::Animating(v)) => p.animating = v,
        (NativeProps::Activity(p), P::Color(c)) => p.color = c,
        (NativeProps::Activity(p), P::HidesWhenStopped(v)) => p.hides_when_stopped = v,
        (NativeProps::Activity(p), P::Large(v)) => {
            p.large = v;
            return Some(if v {
                sizes.activity_large
            } else {
                sizes.activity_small
            });
        }
        (NativeProps::Switch(p), P::TrackColors(on, off)) => (p.track_on, p.track_off) = (on, off),
        (NativeProps::WebView(p), P::Source(v)) => p.source = v,
        (NativeProps::WebView(p), P::InjectedJavaScript(v)) => p.injected_js = v,
        (NativeProps::WebView(p), P::InjectedJavaScriptBefore(v)) => p.injected_js_before = v,
        (NativeProps::WebView(p), P::UserAgent(v)) => p.user_agent = v,
        (NativeProps::WebView(p), P::ScrollEnabled(v)) => p.scroll_enabled = v,
        (NativeProps::WebView(p), P::JavaScriptEnabled(v)) => p.javascript_enabled = v,
        (NativeProps::WebView(p), P::BackForwardGestures(v)) => p.back_forward_gestures = v,
        (NativeProps::Video(p), P::Source(v)) => {
            p.source = match v {
                crate::WebSource::Uri(uri) => Some(uri),
                _ => None,
            }
        }
        (NativeProps::Video(p), P::Paused(v)) => p.paused = v,
        (NativeProps::Video(p), P::Muted(v)) => p.muted = v,
        (NativeProps::Video(p), P::Volume(v)) => p.volume = v.clamp(0.0, 1.0),
        (NativeProps::Video(p), P::Rate(v)) => p.rate = v.max(0.0),
        (NativeProps::Video(p), P::Repeat(v)) => p.repeat = v,
        (NativeProps::Video(p), P::VideoResizeMode(v)) => p.resize_mode = v,
        (NativeProps::Video(p), P::Controls(v)) => p.controls = v,
        (NativeProps::Video(p), P::ProgressInterval(v)) => p.progress_interval_ms = v.max(16.0),
        _ => {}
    }
    None
}

/// `style` with its font metrics multiplied for the user's text size.
fn scaled(mut style: ResolvedTextStyle, multiplier: f32) -> ResolvedTextStyle {
    if multiplier != 1.0 {
        style.font_size *= multiplier;
        style.line_height *= multiplier;
    }
    style
}

fn root_style(viewport: Size) -> taffy::Style {
    taffy::Style {
        size: taffy::Size {
            width: taffy::Dimension::length(viewport.width),
            height: taffy::Dimension::length(viewport.height),
        },
        ..Style::default().layout
    }
}
