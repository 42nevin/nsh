//! Instance-scoped inspection of live views. The host adds its window and
//! runtime epoch to these engine-local addresses.
use std::sync::Arc;

use rustc_hash::FxHashMap;
use taffy::style::{ExpandedDimension, ExpandedLengthPercentage, ExpandedLengthPercentageAuto};

use super::{Core, Key, Kind, NO_KEY};
use crate::{
    Color, LineHeight, NodeId, Points, Prop, Style, StyleKey, StyleValue, TextEngine,
    animation::AnimationProp, geometry::Percentage,
};

/// A node incarnation within one running surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ViewTarget {
    /// Renderer node identifier.
    pub node: NodeId,
    /// Changes when the mounted identity ends, including when a list rebinds it.
    pub generation: u64,
}

/// The ordinary properties supported by the inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum Property {
    Padding,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    Gap,
    RowGap,
    ColumnGap,
    Width,
    Height,
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
    Background,
    Opacity,
    Radius,
    FontSize,
    LineHeight,
    TextColor,
}

impl Property {
    const ALL: [Self; 20] = [
        Self::Padding,
        Self::PaddingTop,
        Self::PaddingRight,
        Self::PaddingBottom,
        Self::PaddingLeft,
        Self::Gap,
        Self::RowGap,
        Self::ColumnGap,
        Self::Width,
        Self::Height,
        Self::MinWidth,
        Self::MaxWidth,
        Self::MinHeight,
        Self::MaxHeight,
        Self::Background,
        Self::Opacity,
        Self::Radius,
        Self::FontSize,
        Self::LineHeight,
        Self::TextColor,
    ];

    fn key(self) -> StyleKey {
        use Property as P;
        use StyleKey as K;
        match self {
            P::Padding => K::Padding,
            P::PaddingTop => K::PaddingTop,
            P::PaddingRight => K::PaddingRight,
            P::PaddingBottom => K::PaddingBottom,
            P::PaddingLeft => K::PaddingLeft,
            P::Gap => K::Gap,
            P::RowGap => K::RowGap,
            P::ColumnGap => K::ColumnGap,
            P::Width => K::Width,
            P::Height => K::Height,
            P::MinWidth => K::MinWidth,
            P::MaxWidth => K::MaxWidth,
            P::MinHeight => K::MinHeight,
            P::MaxHeight => K::MaxHeight,
            P::Background => K::BackgroundColor,
            P::Opacity => K::Opacity,
            P::Radius => K::BorderRadius,
            P::FontSize => K::FontSize,
            P::LineHeight => K::LineHeight,
            P::TextColor => K::Color,
        }
    }

    fn applies_to(self, kind: Kind) -> bool {
        match self {
            Self::Padding
            | Self::PaddingTop
            | Self::PaddingRight
            | Self::PaddingBottom
            | Self::PaddingLeft => {
                matches!(kind, Kind::View | Kind::Scroll | Kind::List | Kind::Text)
            }
            Self::Gap | Self::RowGap | Self::ColumnGap => matches!(kind, Kind::View | Kind::Scroll),
            Self::Width
            | Self::Height
            | Self::MinWidth
            | Self::MaxWidth
            | Self::MinHeight
            | Self::MaxHeight => !kind.is_root() && kind != Kind::Cell,
            Self::Background | Self::Opacity | Self::Radius => true,
            Self::FontSize | Self::LineHeight | Self::TextColor => {
                matches!(kind, Kind::Text | Kind::View | Kind::Input)
            }
        }
    }

    fn animation(self) -> Option<AnimationProp> {
        match self {
            Self::Background => Some(AnimationProp::BackgroundColor),
            Self::Opacity => Some(AnimationProp::Opacity),
            Self::TextColor => Some(AnimationProp::Color),
            _ => None,
        }
    }

    fn accepts(self, value: Value) -> bool {
        match (self, value) {
            (Self::Opacity, Value::Opacity(n)) => (0.0..=1.0).contains(&n.0),
            (Self::Background | Self::TextColor, Value::Color(_)) => true,
            (Self::FontSize, Value::Points(p)) => p.0.is_finite() && p.0 > 0.0,
            (Self::Opacity | Self::Background | Self::TextColor, _) => false,
            (_, Value::Points(p)) => p.0.is_finite() && p.0 >= 0.0,
            _ => false,
        }
    }
}

/// A declaration, preview, or computed style value, with its mode intact.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// An absolute length in logical points.
    Points(Points),
    /// Opacity as a fraction of full opacity.
    Opacity(Percentage),
    /// A typed RGBA color.
    Color(Color),
    /// A percentage as a fraction from the underlying declaration.
    Percent(Percentage),
    /// An authored font size as a fraction of inherited font size.
    FontRelative(Percentage),
    /// An explicit line-height declaration, before inheritance or preview.
    LineHeight(LineHeight),
    /// No local text declaration.
    Inherit,
    /// The font's normal line height.
    Normal,
    /// Automatic size or constraint.
    Auto,
    /// One of the intrinsic sizing modes.
    Intrinsic(Intrinsic),
    /// An edge using a platform environment inset.
    Environment,
    /// A shorthand whose constituent values differ.
    Mixed,
}

/// The intrinsic dimension modes that must not be flattened into a measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[allow(missing_docs)]
pub enum Intrinsic {
    MinContent,
    MaxContent,
    FitContent,
    Other,
}

/// The effective declaration and local preview for a particular property.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct PropertyState {
    /// Property address on this view.
    pub property: Property,
    /// Application declaration before the preview, never a measurement.
    pub underlying: Value,
    /// Installed temporary value, if this address has one.
    pub preview: Option<Value>,
    /// Engine-resolved value where distinct from the declaration (e.g. text inheritance).
    pub resolved: Option<Value>,
    /// Whether a preview may be written (as opposed to read or cleared).
    pub writable: bool,
}

/// The common corner radius before and after an optional preview.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Radius {
    /// Latest underlying common radius (points).
    pub underlying: Points,
    /// Temporary common radius, if set.
    pub preview: Option<Points>,
}

/// Current selection metadata. A logical identifier never authorizes mutation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct InspectedView {
    /// Incarnation-qualified property owner.
    pub target: ViewTarget,
    /// Application-supplied locator.
    pub test_id: Option<Arc<str>>,
    /// Nearest logical container first, in the renderer tree.
    pub ancestors: Vec<NodeId>,
    /// Properties the target's renderer consumes.
    pub properties: Vec<PropertyState>,
    /// Convenience projection of the radius property.
    pub radius: Radius,
}

/// A preview was refused without changing the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InspectError {
    /// The node is no longer the selected incarnation.
    Stale,
    /// The radius is not finite or is negative.
    InvalidRadius,
    /// The value has the wrong mode or is out of range.
    InvalidValue,
    /// The renderer or an active value binding owns this property.
    Unsupported,
}

struct Entry {
    target: ViewTarget,
    authored: Vec<(Property, Value)>,
    previews: Vec<(Property, Value)>,
}

#[derive(Default)]
pub(super) struct Inspection {
    next_generation: u64,
    entries: FxHashMap<NodeId, Entry>,
    retired: Vec<ViewTarget>,
}

impl Inspection {
    pub(super) fn tracks(&self, node: NodeId) -> bool {
        self.entries.contains_key(&node)
    }

    pub(super) fn active(&self) -> bool {
        !self.entries.is_empty()
    }

    // Feed currentColor's consumers the actual local override before resolving
    // the cascade. Animated bindings still own their properties.
    pub(super) fn text_color(&self, node: NodeId) -> Option<Color> {
        match self
            .entries
            .get(&node)?
            .previews
            .iter()
            .find(|p| p.0 == Property::TextColor)?
            .1
        {
            Value::Color(c) => Some(c),
            _ => unreachable!("validated inspection color"),
        }
    }

    pub(super) fn compose(&mut self, id: NodeId, kind: Kind, rtl: bool, style: &mut Style) {
        let Some(e) = self.entries.get_mut(&id) else {
            return;
        };
        e.authored.clear();
        e.authored.extend(
            Property::ALL
                .into_iter()
                .filter(|p| p.applies_to(kind))
                .map(|p| (p, declaration(p, style, rtl))),
        );
        for &(p, v) in &e.previews {
            // Binding changes in an authored rerender take precedence while bound.
            if animated(p, &style.animated) {
                continue;
            }
            match v {
                Value::Points(n) => {
                    style
                        .set_key(p.key(), StyleValue::Number(n.0))
                        .expect("validated inspection length");
                    match p {
                        Property::Padding => style.logical.padding = [None; 2],
                        Property::PaddingLeft => style.logical.padding[usize::from(rtl)] = None,
                        Property::PaddingRight => style.logical.padding[usize::from(!rtl)] = None,
                        _ => {}
                    }
                }
                Value::Opacity(n) => style
                    .set_key(p.key(), StyleValue::Number(n.0))
                    .expect("validated opacity"),
                Value::Color(c) => style
                    .apply_value(p.key(), &crate::style::PropertyValue::Color(Some(c)))
                    .expect("validated inspection color"),
                _ => unreachable!("validated inspection mode"),
            }
        }
    }
}

fn animated(p: Property, bindings: &[(AnimationProp, crate::animation::NodeId)]) -> bool {
    p.animation()
        .is_some_and(|key| bindings.iter().any(|&(current, _)| current == key))
}

fn shorthand<const N: usize>(values: [Value; N]) -> Value {
    if values[1..].iter().all(|v| *v == values[0]) {
        values[0]
    } else {
        Value::Mixed
    }
}

fn declaration(p: Property, style: &Style, rtl: bool) -> Value {
    use Property as P;
    use Value as V;
    let lp = |value: taffy::LengthPercentage| match value.expand() {
        ExpandedLengthPercentage::Length(v) => V::Points(Points(v)),
        ExpandedLengthPercentage::Percent(v) => V::Percent(Percentage(v)),
    };
    let constraint = |value: taffy::LengthPercentageAuto| match value.expand() {
        ExpandedLengthPercentageAuto::Length(v) => V::Points(Points(v)),
        ExpandedLengthPercentageAuto::Percent(v) => V::Percent(Percentage(v)),
        ExpandedLengthPercentageAuto::Auto => V::Auto,
    };
    let dimension = |value: taffy::Dimension| match value.expand() {
        ExpandedDimension::Length(v) => V::Points(Points(v)),
        ExpandedDimension::Percent(v) => V::Percent(Percentage(v)),
        ExpandedDimension::Auto => V::Auto,
        ExpandedDimension::MinContent => V::Intrinsic(Intrinsic::MinContent),
        ExpandedDimension::MaxContent => V::Intrinsic(Intrinsic::MaxContent),
        ExpandedDimension::FitContent => V::Intrinsic(Intrinsic::FitContent),
        _ => V::Intrinsic(Intrinsic::Other),
    };
    let l = &style.layout;
    let edge = |index: usize, value| {
        if style.pending.env.padding[index].is_some() {
            V::Environment
        } else {
            lp(value)
        }
    };
    let padding = || {
        [
            edge(0, l.padding.top),
            edge(
                1,
                style.logical.padding[usize::from(!rtl)].unwrap_or(l.padding.right),
            ),
            edge(2, l.padding.bottom),
            edge(
                3,
                style.logical.padding[usize::from(rtl)].unwrap_or(l.padding.left),
            ),
        ]
    };
    let t = &style.text;
    match p {
        P::Padding => shorthand(padding()),
        P::PaddingTop => padding()[0],
        P::PaddingRight => padding()[1],
        P::PaddingBottom => padding()[2],
        P::PaddingLeft => padding()[3],
        P::Gap => shorthand([lp(l.gap.height), lp(l.gap.width)]),
        P::RowGap => lp(l.gap.height),
        P::ColumnGap => lp(l.gap.width),
        P::Width => dimension(l.size.width),
        P::Height => dimension(l.size.height),
        P::MinWidth => constraint(l.min_size.width),
        P::MaxWidth => constraint(l.max_size.width),
        P::MinHeight => constraint(l.min_size.height),
        P::MaxHeight => constraint(l.max_size.height),
        P::Radius => V::Points(style.view.border_radius),
        P::Opacity => V::Opacity(Percentage(style.view.opacity)),
        P::Background => V::Color(style.view.background_color),
        P::TextColor => t.color.map_or(V::Inherit, V::Color),
        P::FontSize => t
            .font_size_em
            .map(|ratio| V::FontRelative(Percentage(ratio)))
            .or(t.font_size.map(V::Points))
            .unwrap_or(V::Inherit),
        P::LineHeight => t
            .line_height_em
            .map(LineHeight::Em)
            .or(t.line_height_multiple.map(LineHeight::Multiple))
            .or(t.line_height.map(LineHeight::Points))
            .or(style
                .pending
                .inspection_normal_line_height
                .then_some(LineHeight::Normal))
            .map_or(V::Inherit, V::LineHeight),
    }
}

impl<M: TextEngine> Core<M> {
    /// Retires an incarnation and any inspected descendants before their view
    /// can show another item. Does not retire an ordinary in-tree reorder.
    pub(super) fn retire_inspection_subtree(&mut self, key: Key) {
        if !self.inspection.active() {
            return;
        }
        let mut pending = vec![key];
        while let Some(k) = pending.pop() {
            let node = &self.nodes[k as usize];
            pending.extend_from_slice(&node.children);
            if let Some(e) = self.inspection.entries.remove(&node.id) {
                self.inspection.retired.push(e.target);
                if !e.previews.is_empty() {
                    self.restyle(k);
                }
            }
        }
    }

    /// Takes each incarnation that became invalid since the previous drain.
    pub fn take_inspection_retirements(&mut self) -> Vec<ViewTarget> {
        std::mem::take(&mut self.inspection.retired)
    }

    /// A keyed reorder keeps each retained item. A splice or replacement
    /// invalidates the outgoing item even when the slot and index stay.
    pub(super) fn retire_inspection_list_items(&mut self, key: Key, prop: &Prop) {
        let node = &self.nodes[key as usize];
        if let Prop::CellIndex(index) = *prop {
            if self
                .nodes
                .get(node.parent as usize)
                .and_then(|p| p.list.as_ref())
                .is_some_and(|list| list.rebinds_cell(key, index))
            {
                self.retire_inspection_subtree(key);
            }
            return;
        }
        let Some(list) = node.list.as_ref() else {
            return;
        };
        let cells = match prop {
            Prop::Splice(at, removed, _) => {
                list.cells_losing_item(|i| i >= *at && i - *at < *removed)
            }
            Prop::ListDiff(diff) => list
                .cells_losing_item(|i| diff.old_len != list.count || diff.new_index(i).is_none()),
            Prop::Count(count) if *count < list.count => list.cells_losing_item(|i| i >= *count),
            _ => return,
        };
        for cell in cells {
            self.retire_inspection_subtree(cell);
        }
    }

    /// Describes an attached view in the active surface, without writing it.
    pub fn inspect_view(&mut self, id: NodeId) -> Option<InspectedView> {
        let key = self.key(id).ok()?;
        let node = &self.nodes[key as usize];
        if !node.created || node.inline || key == self.root || node.decoration {
            return None;
        }
        let mut ancestors = Vec::new();
        let mut parent = node.parent;
        while parent != NO_KEY {
            let ancestor = &self.nodes[parent as usize];
            if parent == self.root {
                break;
            }
            ancestors.push(ancestor.id);
            parent = ancestor.parent;
        }
        if parent == NO_KEY
            && !node.kind.is_root()
            && !ancestors.iter().any(|&id| {
                self.key(id)
                    .ok()
                    .is_some_and(|k| self.nodes[k as usize].kind.is_root())
            })
        {
            return None;
        }
        if !self.inspection.tracks(id) {
            self.inspection.next_generation = self
                .inspection
                .next_generation
                .checked_add(1)
                .expect("view incarnation exhausted");
            self.inspection.entries.insert(
                id,
                Entry {
                    target: ViewTarget {
                        node: id,
                        generation: self.inspection.next_generation,
                    },
                    authored: Vec::new(),
                    previews: Vec::new(),
                },
            );
            self.restyle(key);
        }
        let node = &self.nodes[key as usize];
        let entry = &self.inspection.entries[&id];
        let text = self.inherited_text_style(key).inherit(&node.text_style);
        let mut opacity = node.view_style.opacity;
        let mut parent = node.parent;
        while parent != NO_KEY {
            let ancestor = &self.nodes[parent as usize];
            opacity *= ancestor.view_style.opacity;
            parent = ancestor.parent;
        }
        let properties = entry
            .authored
            .iter()
            .map(|&(p, underlying)| PropertyState {
                property: p,
                underlying,
                preview: entry.previews.iter().find(|v| v.0 == p).map(|v| v.1),
                resolved: match p {
                    Property::Opacity => Some(Value::Opacity(Percentage(opacity))),
                    Property::TextColor => Some(Value::Color(text.color)),
                    Property::FontSize => Some(Value::Points(text.font_size)),
                    Property::LineHeight if text.line_height > Points::ZERO => {
                        Some(Value::Points(text.line_height))
                    }
                    Property::LineHeight => Some(Value::Normal),
                    _ => None,
                },
                writable: !animated(p, &node.animated),
            })
            .collect::<Vec<_>>();
        let radius = radius_of(
            *properties
                .iter()
                .find(|p| p.property == Property::Radius)
                .expect("an inspectable view has a radius"),
        );
        Some(InspectedView {
            target: entry.target,
            test_id: node.props.as_ref().and_then(|p| p.test_id.clone()),
            ancestors,
            properties,
            radius,
        })
    }

    /// Applies or removes one absolute preview through the renderer's normal
    /// style path. Relative declarations are retained, not replaced with
    /// computed values. Completion is engine acceptance, not native display.
    pub fn preview_property(
        &mut self,
        target: ViewTarget,
        p: Property,
        value: Option<Value>,
    ) -> Result<PropertyState, InspectError> {
        let entry = self
            .inspection
            .entries
            .get_mut(&target.node)
            .ok_or(InspectError::Stale)?;
        if entry.target != target {
            return Err(InspectError::Stale);
        }
        let key = self
            .key(target.node)
            .expect("an inspected view remains live");
        let node = &self.nodes[key as usize];
        if !p.applies_to(node.kind) || value.is_some() && animated(p, &node.animated) {
            return Err(InspectError::Unsupported);
        }
        if value.is_some_and(|v| !p.accepts(v)) {
            return Err(InspectError::InvalidValue);
        }
        let entry = self
            .inspection
            .entries
            .get_mut(&target.node)
            .expect("checked target");
        let at = entry.previews.binary_search_by_key(&p, |v| v.0);
        let changed = match (at, value) {
            (Ok(i), Some(value)) => std::mem::replace(&mut entry.previews[i].1, value) != value,
            (Ok(i), None) => {
                entry.previews.remove(i);
                true
            }
            (Err(i), Some(value)) => {
                entry.previews.insert(i, (p, value));
                true
            }
            (Err(_), None) => false,
        };
        if changed {
            self.restyle(key);
        }
        Ok(self
            .inspect_view(target.node)
            .expect("validated mounted target")
            .properties
            .into_iter()
            .find(|s| s.property == p)
            .expect("validated renderer property"))
    }

    /// Sets or removes a common-radius preview using the ordinary property owner.
    pub fn preview_radius(
        &mut self,
        target: ViewTarget,
        radius: Option<Points>,
    ) -> Result<Radius, InspectError> {
        self.preview_property(target, Property::Radius, radius.map(Value::Points))
            .map(radius_of)
            .map_err(|e| {
                if e == InspectError::InvalidValue {
                    InspectError::InvalidRadius
                } else {
                    e
                }
            })
    }
}

fn radius_of(state: PropertyState) -> Radius {
    let points = |v| match v {
        Value::Points(p) => p,
        _ => unreachable!("radius uses points"),
    };
    Radius {
        underlying: points(state.underlying),
        preview: state.preview.map(points),
    }
}
