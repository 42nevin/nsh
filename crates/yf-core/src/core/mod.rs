mod list;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use slab::Slab;
use taffy::{AvailableSpace, NodeId as LayoutId, Overflow, Position, TaffyTree};
use yf_anim::AnimProp;

use self::list::ListState;
use crate::{
    geometry::{Rect, Size},
    mount::{MountBatch, MountItem, ViewKind},
    ops::{EventKind, EventMask, ImageSource, NodeId, Op, Prop, ROOT_ID, ResizeMode, Tag},
    style::{Style, TextStyle, ViewStyle},
    text::{AttributedText, MeasureCache, ResolvedTextStyle, TextEngine, TextLayout},
};

/// Slab index of a node. Internal links use keys, never ids, to avoid the
/// id -> key indirection on hot paths.
type Key = u32;

const NO_KEY: Key = Key::MAX;

/// Dirty flags, processed once per commit.
mod dirty {
    /// Emit [`crate::MountItem::ViewStyle`].
    pub(super) const VIEW: u16 = 1 << 0;
    /// Emit [`crate::MountItem::Events`].
    pub(super) const EVENTS: u16 = 1 << 1;
    /// Rebuild the attributed text of a text block.
    pub(super) const TEXT_BUILD: u16 = 1 << 2;
    /// Lay out the text block again and emit [`crate::MountItem::Text`].
    pub(super) const TEXT_MOUNT: u16 = 1 << 3;
    /// Emit [`crate::MountItem::Image`].
    pub(super) const IMAGE: u16 = 1 << 4;
    /// Emit [`crate::MountItem::ScrollConfig`].
    pub(super) const SCROLL: u16 = 1 << 5;
    /// Emit [`crate::MountItem::Refreshing`].
    pub(super) const REFRESHING: u16 = 1 << 6;
    /// Emit [`crate::MountItem::AnimBindings`].
    pub(super) const ANIM: u16 = 1 << 7;
    /// Emit [`crate::MountItem::PanValues`].
    pub(super) const PAN: u16 = 1 << 8;
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
}

impl Kind {
    /// Kinds whose native view hosts child views.
    fn is_container(self) -> bool {
        matches!(self, Kind::View | Kind::Scroll | Kind::List | Kind::Cell)
    }

    fn view_kind(self) -> ViewKind {
        match self {
            Kind::Text => ViewKind::Text,
            Kind::Image => ViewKind::Image,
            Kind::Scroll | Kind::List => ViewKind::Scroll,
            Kind::View | Kind::Cell | Kind::RawText => ViewKind::View,
        }
    }

    /// Kinds whose taffy style is derived from the JS style (and must keep
    /// the JS style around to re-derive it).
    fn derives_layout(self) -> bool {
        matches!(self, Kind::Text | Kind::Scroll | Kind::List | Kind::Cell)
    }
}

#[derive(Debug, Default)]
struct ScrollState {
    horizontal: bool,
    shows_indicator: bool,
    refreshing: bool,
    scroll_value: Option<yf_anim::NodeId>,
    /// Last content size sent to the platform.
    content: Option<Size>,
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
    /// Raw text content.
    text: String,
    image: Option<Box<ImageState>>,
    scroll: Option<Box<ScrollState>>,
    list: Option<Box<ListState>>,
    /// Recycling slot of a list cell.
    slot: Option<u32>,
    pan: (Option<yf_anim::NodeId>, Option<yf_anim::NodeId>),
    /// The native view exists.
    created: bool,
    /// Last frame sent to the platform.
    frame: Rect,
    dirty: u16,
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
            text: String::new(),
            image: matches!(kind, Kind::Image).then(Box::default),
            scroll: matches!(kind, Kind::Scroll | Kind::List).then(Box::default),
            list: matches!(kind, Kind::List).then(|| Box::new(ListState::new())),
            slot: None,
            pan: (None, None),
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

/// Measured leaves: text blocks and images with a known natural size.
enum LeafContext<P> {
    Text(TextContext<P>),
    Image(Option<Size>),
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
    list_binds: Vec<ListBind>,
    events: Vec<(NodeId, EventKind)>,
    /// Reused buffer of `(item index, cell)` for list updates.
    cells_scratch: Vec<(u32, Key)>,
    root: Key,
    viewport: Size,
    mount: Vec<MountItem>,
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
            list_binds: Vec::new(),
            events: Vec::new(),
            cells_scratch: Vec::new(),
            root,
            viewport,
            mount: Vec::new(),
            queue: Vec::new(),
            needs_layout: true,
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

    /// The platform scrolled a scroll view or list to `(x, y)`.
    pub fn set_scroll_offset(&mut self, id: NodeId, x: f32, y: f32) {
        let Ok(key) = self.key(id) else { return };
        let node = &mut self.nodes[key as usize];
        let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
        if let Some(list) = node.list.as_mut() {
            let offset = if horizontal { x } else { y };
            if offset != list.offset {
                list.offset = offset;
                list.dirty = true;
            }
        }
    }

    /// Moves list render requests produced by the last commit into `out`.
    pub fn take_list_binds(&mut self, out: &mut Vec<ListBind>) {
        out.append(&mut self.list_binds);
    }

    /// Moves events the core raised (e.g. `onEndReached`) into `out`.
    pub fn take_events(&mut self, out: &mut Vec<(NodeId, EventKind)>) {
        out.append(&mut self.events);
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
        }
    }

    /// Finishes a turn: rebuilds text, runs layout, updates lists and
    /// returns the mount items, or `None` when nothing changed.
    pub fn commit(&mut self) -> Option<MountBatch> {
        self.flush_dirty();
        let relayout = self.needs_layout;
        if relayout {
            self.needs_layout = false;
            // Only walk the main tree when it was laid out again (a change
            // inside a list cell leaves it untouched).
            if self.compute_layout() {
                let root = self.root;
                let children = std::mem::take(&mut self.nodes[root as usize].children);
                for &child in &children {
                    self.emit_frames(child);
                }
                self.nodes[root as usize].children = children;
            }
        }
        self.update_lists(relayout);
        self.flush_text();
        if self.mount.is_empty() {
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
        self.sync_layout_style(key);
        Ok(())
    }

    /// Pushes the effective taffy style of `key`: the JS style, adjusted
    /// for scroll containers and list cells.
    fn sync_layout_style(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (Some(layout), Some(base)) = (node.layout, node.base_layout.as_deref()) else {
            return;
        };
        let mut style = base.clone();
        match node.kind {
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
        } = style;
        let view_changed = node.view_style != view;
        let text_changed = node.text_style != text;
        let anim_changed = node.animated != animated;
        node.view_style = view;
        node.text_style = text;
        node.animated = animated;
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
        if text_changed && self.nodes[key as usize].kind == Kind::Text {
            self.mark_text_block(key);
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
                }
            }
            Prop::Horizontal(horizontal) => {
                if let Some(scroll) = node.scroll.as_mut()
                    && scroll.horizontal != horizontal
                {
                    scroll.horizontal = horizontal;
                    flags |= dirty::SCROLL;
                    if let Some(list) = node.list.as_mut() {
                        list.clear_sizes();
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
                node.pan = (x, y);
                flags |= dirty::PAN;
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
        if let Some(child_layout) = self.nodes[child as usize].layout {
            // List cells stay independent layout roots.
            if self.nodes[parent as usize].list.is_none() {
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
                let before = self.next_native_sibling(parent, pos);
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
        if let Some(layout) = layout {
            if self.nodes[parent as usize].list.is_none() {
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

    /// The first sibling after `pos` that has a native view.
    fn next_native_sibling(&self, parent: Key, pos: usize) -> Option<NodeId> {
        self.nodes[parent as usize].children[pos + 1..]
            .iter()
            .map(|&c| &self.nodes[c as usize])
            .find(|c| c.created && c.layout.is_some())
            .map(|c| c.id)
    }

    /// Index among rendering (layout-backed) children of `parent` for a
    /// child at `pos` in the full children list.
    fn render_index(&self, parent: Key, pos: usize) -> u32 {
        self.nodes[parent as usize].children[..pos]
            .iter()
            .filter(|&&c| self.nodes[c as usize].layout.is_some())
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
        if node.pan != (None, None) {
            flags |= dirty::PAN;
        }
        if let Some(list) = node.list.as_mut() {
            list.dirty = true;
        }
        self.mount.push(MountItem::Create {
            id,
            kind: kind.view_kind(),
        });
        self.mark(key, flags);
        if !kind.is_container() {
            return;
        }
        // In order, each on top of the previous one.
        let children = std::mem::take(&mut self.nodes[key as usize].children);
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
        self.nodes[key as usize].children = children;
    }

    #[inline]
    fn mark(&mut self, key: Key, flags: u16) {
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
                self.mount.push(MountItem::PanValues {
                    id,
                    x: node.pan.0,
                    y: node.pan.1,
                });
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
            ..AttributedText::default()
        };
        let base = ResolvedTextStyle::default().inherit(&node.text_style);
        self.collect_text(key, base, &mut text);
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

    /// Lays out what changed. Returns true if the main tree was laid out.
    fn compute_layout(&mut self) -> bool {
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
        // Then every list cell that changed, at its list's cross size.
        for i in 0..self.lists.len() {
            let key = self.lists[i];
            let list = &self.nodes[key as usize];
            let Some(list_layout) = list.layout else {
                continue;
            };
            let horizontal = list.scroll.as_ref().is_some_and(|s| s.horizontal);
            let size = self.layout.layout(list_layout).expect("taffy layout").size;
            let available = if horizontal {
                taffy::Size {
                    width: AvailableSpace::MaxContent,
                    height: AvailableSpace::Definite(size.height),
                }
            } else {
                taffy::Size {
                    width: AvailableSpace::Definite(size.width),
                    height: AvailableSpace::MaxContent,
                }
            };
            // Laying out even a clean root costs a tree walk (taffy rounds the
            // whole subtree), so only dirty cells are laid out again — all
            // of them when the list's cross size changed.
            let cross = if horizontal { size.height } else { size.width };
            let list_state = self.nodes[key as usize].list.as_mut().expect("list");
            let resized = list_state.cells_cross != cross;
            list_state.cells_cross = cross;
            let cells = std::mem::take(&mut self.nodes[key as usize].children);
            for &cell in &cells {
                let Some(cell_layout) = self.nodes[cell as usize].layout else {
                    continue;
                };
                if resized || self.layout.dirty(cell_layout).unwrap_or(true) {
                    self.layout_root(cell_layout, available);
                    self.nodes[cell as usize].relaid = true;
                }
            }
            self.nodes[key as usize].children = cells;
        }
        main
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
        // List cells are positioned by `update_lists`.
        if kind.is_container() && kind != Kind::List {
            let children = std::mem::take(&mut self.nodes[key as usize].children);
            for &child in &children {
                self.emit_frames(child);
            }
            self.nodes[key as usize].children = children;
        }
    }

    fn set_frame(&mut self, key: Key, frame: Rect) {
        let node = &mut self.nodes[key as usize];
        if node.frame != frame {
            node.frame = frame;
            let (id, is_text) = (node.id, node.kind == Kind::Text);
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
            if !(relayout || list.dirty) {
                continue;
            }
            let (id, horizontal) = (node.id, node.scroll.as_ref().is_some_and(|s| s.horizontal));
            let l = *self
                .layout
                .layout(node.layout.expect("list layout"))
                .expect("taffy layout");
            let (main, cross) = if horizontal {
                (l.size.width, l.size.height)
            } else {
                (l.size.height, l.size.width)
            };

            // Sizes of cells showing their item.
            let mut ready = std::mem::take(&mut self.cells_scratch);
            ready.clear();
            ready.extend(
                list.slots
                    .iter()
                    .filter_map(|s| s.ready().map(|i| (i, s.cell))),
            );
            let mut anchor = 0.0;
            for &(index, cell) in &ready {
                if let Some(cell_layout) = self.nodes.get(cell as usize).and_then(|c| c.layout) {
                    let size = self.layout.layout(cell_layout).expect("taffy layout").size;
                    let main_size = if horizontal { size.width } else { size.height };
                    let list = self.nodes[key as usize].list.as_mut().expect("list");
                    anchor += list.measured(index, main_size);
                }
            }

            let list = self.nodes[key as usize].list.as_mut().expect("list");
            list.viewport = main;
            if anchor != 0.0 {
                list.offset += anchor;
                let (dx, dy) = if horizontal {
                    (anchor, 0.0)
                } else {
                    (0.0, anchor)
                };
                self.mount.push(MountItem::ScrollBy { id, dx, dy });
            }
            let mut binds = Vec::new();
            if list.update_window(&mut binds) {
                self.events.push((id, EventKind::EndReached));
            }
            if !binds.is_empty() {
                self.list_binds.push(ListBind { list: id, binds });
            }

            // Position cells that show their item (the window may have
            // changed which ones). Descendant frames only change when the
            // cell was laid out again.
            let list = self.nodes[key as usize].list.as_ref().expect("list");
            let total = list.total();
            ready.clear();
            ready.extend(
                list.slots
                    .iter()
                    .filter_map(|s| s.ready().map(|i| (i, s.cell))),
            );
            for &(index, cell) in &ready {
                let offset = self.nodes[key as usize]
                    .list
                    .as_ref()
                    .expect("list")
                    .offset_of(index);
                let Some(cell_layout) = self.nodes.get(cell as usize).and_then(|c| c.layout) else {
                    continue;
                };
                let size = self.layout.layout(cell_layout).expect("taffy layout").size;
                let frame = if horizontal {
                    Rect {
                        x: offset,
                        y: 0.0,
                        width: size.width,
                        height: cross,
                    }
                } else {
                    Rect {
                        x: 0.0,
                        y: offset,
                        width: cross,
                        height: size.height,
                    }
                };
                self.set_frame(cell, frame);
                if std::mem::take(&mut self.nodes[cell as usize].relaid) {
                    let children = std::mem::take(&mut self.nodes[cell as usize].children);
                    for &child in &children {
                        self.emit_frames(child);
                    }
                    self.nodes[cell as usize].children = children;
                }
            }
            self.cells_scratch = ready;
            let content = if horizontal {
                Size::new(total, cross)
            } else {
                Size::new(cross, total)
            };
            self.set_scroll_content(key, content);
        }
    }
}

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

fn root_style(viewport: Size) -> taffy::Style {
    taffy::Style {
        size: taffy::Size {
            width: taffy::Dimension::length(viewport.width),
            height: taffy::Dimension::length(viewport.height),
        },
        ..Style::default().layout
    }
}
