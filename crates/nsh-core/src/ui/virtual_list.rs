//! Lists of a component's own rows ([`virtual_list`]): virtualized and
//! recycled like `List`, the rows rendered in Rust. The core binds the
//! cells itself (no JS), within the commit that needs them. The layout
//! methods both kinds of list share are [`ListLayout`].

use std::{
    hash::{Hash, Hasher},
    rc::Rc,
};

use super::{
    Accessible, AnyElement, Interactive, Position, ScrollEvent,
    element::{ElKind, ElNode, Element, IntoElement, ListConfig, Listener},
    events,
};
use crate::{EventKind, ListContentStyle, Prop, Tag};

/// How a list lays out its rows: [`super::ListElement`] (the app's rows)
/// and [`VirtualList`] (the component's).
pub trait ListLayout: Sized {
    #[doc(hidden)]
    fn config(&mut self) -> &mut ListConfig;

    #[doc(hidden)]
    fn content(&mut self) -> &mut ListContentStyle {
        self.config().content.get_or_insert_default()
    }

    /// The size of rows not measured yet, in points (default 50).
    fn estimated_item_size(mut self, points: f32) -> Self {
        self.config().estimated = Some(points);
        self
    }

    /// Rows side by side (vertical lists).
    fn columns(mut self, columns: u32) -> Self {
        self.config().columns = Some(columns.max(1));
        self
    }

    /// Scrolls horizontally.
    fn horizontal(mut self) -> Self {
        self.config().horizontal = Some(true);
        self
    }

    /// Item 0 at the end (chats), starting scrolled to the end.
    fn inverted(mut self) -> Self {
        self.config().inverted = Some(true);
        self
    }

    /// Whether the scroll indicator shows (default yes).
    fn shows_indicator(mut self, shows: bool) -> Self {
        self.config().shows_indicator = Some(shows);
        self
    }

    /// Padding around the rows, in points.
    fn content_padding(mut self, points: f32) -> Self {
        self.content().padding = crate::EdgeInsets::all(points);
        self
    }

    /// Horizontal padding around the rows.
    fn content_padding_x(mut self, points: f32) -> Self {
        let p = &mut self.content().padding;
        (p.left, p.right) = (points, points);
        self
    }

    /// Vertical padding around the rows.
    fn content_padding_y(mut self, points: f32) -> Self {
        let p = &mut self.content().padding;
        (p.top, p.bottom) = (points, points);
        self
    }

    /// Padding above the rows.
    fn content_padding_top(mut self, points: f32) -> Self {
        self.content().padding.top = points;
        self
    }

    /// Padding below the rows.
    fn content_padding_bottom(mut self, points: f32) -> Self {
        self.content().padding.bottom = points;
        self
    }

    /// Space between rows (columns in horizontal lists).
    fn row_gap(mut self, points: f32) -> Self {
        self.content().row_gap = points;
        self
    }

    /// Space between columns ([`Self::columns`]).
    fn column_gap(mut self, points: f32) -> Self {
        self.content().column_gap = points;
        self
    }
}

/// Renders row `index` (of `Position::count`).
pub(crate) type RowFn = Rc<dyn Fn(usize, Position) -> AnyElement>;

/// A list of the component's own rows, as the render makes it.
#[derive(Clone)]
pub(crate) struct VirtualListEl {
    pub(crate) config: ListConfig,
    pub(crate) count: u32,
    pub(crate) row: RowFn,
    /// The rows' identities, for splices (rows keep their cells and sizes
    /// when rows are added or removed before them).
    pub(crate) keys: Option<Rc<[u64]>>,
}

/// A virtualized, recycling list of `count` rows rendered by `row`: only
/// the rows near the viewport exist. `row(index, at)` builds a row from the
/// component's data (move it in: `let rows = self.rows.clone()`); it runs
/// again when the cell shows another row and when the component renders.
/// Give the rows' identities with [`VirtualList::keys`] when rows come and
/// go other than at the end.
pub fn virtual_list<R: IntoElement>(
    count: usize,
    row: impl Fn(usize, Position) -> R + 'static,
) -> VirtualList {
    let mut node = ElNode::new(Tag::List);
    node.kind = ElKind::VirtualList(Box::new(VirtualListEl {
        config: ListConfig::default(),
        count: count as u32,
        row: Rc::new(move |index, at| row(index, at).into_element()),
        keys: None,
    }));
    VirtualList(node)
}

/// A list of the component's own rows ([`virtual_list`]).
#[derive(Clone)]
pub struct VirtualList(ElNode);

impl VirtualList {
    fn list(&mut self) -> &mut VirtualListEl {
        match &mut self.0.kind {
            ElKind::VirtualList(list) => list,
            _ => unreachable!("a VirtualList is a virtual list"),
        }
    }

    /// The rows' identities, in order (any hashable id per row).
    pub fn keys<K: Hash>(mut self, keys: impl IntoIterator<Item = K>) -> Self {
        let keys: Rc<[u64]> = keys
            .into_iter()
            .map(|k| {
                let mut h = rustc_hash::FxHasher::default();
                k.hash(&mut h);
                h.finish()
            })
            .collect();
        self.list().keys = Some(keys);
        self
    }

    /// Scrolled close to the end (load more).
    pub fn on_end_reached(self, listener: Listener<()>) -> Self {
        self.on(EventKind::EndReached, events::typed(listener, events::unit))
    }

    /// Shows the pull-to-refresh spinner.
    pub fn refreshing(mut self, refreshing: bool) -> Self {
        super::host::set(&mut self.0, Prop::Refreshing(refreshing));
        self
    }

    /// Pulled down to refresh (enables pull to refresh).
    pub fn on_refresh(self, listener: Listener<()>) -> Self {
        self.on(EventKind::Refresh, events::typed(listener, events::unit))
    }

    /// It moved.
    pub fn on_scroll(self, listener: Listener<ScrollEvent>) -> Self {
        self.on(EventKind::Scroll, events::typed(listener, events::scroll))
    }
}

impl ListLayout for VirtualList {
    fn config(&mut self) -> &mut ListConfig {
        &mut self.list().config
    }
}

impl Element for VirtualList {
    fn node(&mut self) -> &mut ElNode {
        &mut self.0
    }
}

impl IntoElement for VirtualList {
    fn into_element(self) -> AnyElement {
        AnyElement(self.0)
    }
}

impl Accessible for VirtualList {}
impl Interactive for VirtualList {}

/// How the rows changed from `old` to `new` keys: `(at, removed,
/// inserted)` by the common start and end (as `List` does), `None` when
/// they are the same.
pub(crate) fn splice(old: &[u64], new: &[u64]) -> Option<(u32, u32, u32)> {
    let max = old.len().min(new.len());
    let start = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take(max - start)
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old.len() - start - end;
    let inserted = new.len() - start - end;
    (removed > 0 || inserted > 0).then_some((start as u32, removed as u32, inserted as u32))
}

#[cfg(test)]
mod tests {
    #[test]
    fn splices_keep_the_common_ends() {
        use super::splice;
        assert_eq!(splice(&[1, 2, 3], &[1, 2, 3]), None);
        assert_eq!(splice(&[1, 2, 3], &[0, 1, 2, 3]), Some((0, 0, 1)));
        assert_eq!(splice(&[1, 2, 3], &[1, 3]), Some((1, 1, 0)));
        assert_eq!(splice(&[1, 2, 3], &[1, 2, 3, 4]), Some((3, 0, 1)));
        assert_eq!(splice(&[1, 1], &[1]), Some((1, 1, 0)));
    }
}
