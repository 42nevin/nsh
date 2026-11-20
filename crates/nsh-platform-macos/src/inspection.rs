//! Native, window-scoped inspection independent of input hit-test admission.
mod shape;

use nsh_core::{
    Bounds, Color, CornerRadii, NodeId, Point, Points, Size,
    inspection::{Property, Value},
};
use nsh_engine::EngineControl;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow};
use shape::P;

use crate::{AppView, view::NshView};

/// A run of one mounted engine. Replacing an app in the same AppView starts a new run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Epoch {
    /// Stable surface identity for the life of the AppView.
    pub surface: u64,
    /// Engine incarnation on this surface.
    pub run: u64,
}

/// A query in the selected window's native root coordinates, in flipped points.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum SelectionArea {
    /// Frontmost element and alternatives beneath it.
    Point(Point<Points>),
    /// Visible elements intersecting a retained region.
    Region(Bounds<Points>),
}

/// A mounted candidate. The renderer's (node, generation) must still be
/// verified by the engine when submitting any change.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Candidate {
    /// Renderer identifier.
    pub node: NodeId,
    /// AppKit window number where the view was picked.
    pub window: isize,
    /// Runtime owning the native view.
    pub epoch: Epoch,
    /// Axis-aligned bounds of the visible outline in window-root points.
    pub visible: Bounds<Points>,
    /// Projected and clipped outline in the same root coordinates.
    pub outline: Vec<Point<Points>>,
    view: NshView,
}

impl Candidate {
    /// Samples the current native backing radius, which can be clamped.
    pub fn mounted_corner_radius(&self, app: &AppView) -> Option<Points> {
        self.is_current(app)
            .then(|| self.view.ns().layer())
            .flatten()
            .map(|layer| Points(layer.cornerRadius() as f32))
    }

    /// Whether this is still the mounted incarnation in the captured window.
    /// The engine also verifies logical row incarnation before accepting edits.
    pub fn is_current(&self, app: &AppView) -> bool {
        let surface = app.surface();
        surface.inspection_epoch() == Some(self.epoch)
            && surface.mounter.borrow().views.get(&self.node) == Some(&self.view)
            && checked_root(app, &self.view, self.window).is_some()
            && !self
                .view
                .state()
                .live
                .as_ref()
                .is_some_and(|live| live.dying())
    }

    /// The current geometry, independent of when the candidate was chosen.
    pub fn remeasure(&self, app: &AppView) -> Option<Self> {
        if !self.is_current(app) {
            return None;
        }
        let root = checked_root(app, &self.view, self.window)?;
        let polygon = showing(&self.view, &root)?;
        candidate(&self.view, self.window, self.epoch, polygon)
    }

    /// Native measurements where they have a scalar interpretation. Padding
    /// and gap are measured by comparing child frames, not reading declarations.
    pub fn mounted_property(&self, app: &AppView, p: Property) -> Option<Value> {
        if !self.is_current(app) {
            return None;
        }
        let state = self.view.state();
        let points = |p: Points| Some(Value::Points(p));
        match p {
            Property::Width => points(state.frame.size.width),
            Property::Height => points(state.frame.size.height),
            Property::Radius => self
                .view
                .ns()
                .layer()
                .and_then(|l| points(Points(l.cornerRadius() as f32))),
            Property::Background => Some(Value::Color(state.background())),
            Property::Opacity => {
                let mut value = self.view.ns().layer()?.opacity();
                let mut parent = self.view.parent_view();
                while let Some(v) = parent {
                    if let Some(l) = v.ns().layer() {
                        value *= l.opacity();
                    }
                    parent = v.parent_view();
                }
                Some(Value::Number(value))
            }
            Property::FontSize | Property::LineHeight | Property::TextColor => {
                let span = &state.text.as_ref()?.text.spans.first()?.style;
                match p {
                    Property::FontSize => points(span.font_size),
                    Property::LineHeight => points(span.line_height),
                    Property::TextColor => Some(Value::Color(Color(span.color.0))),
                    _ => unreachable!(),
                }
            }
            _ => None,
        }
    }
}

impl AppView {
    /// Candidates in the AppView's primary window, front-to-back.
    pub fn inspection_candidates(&self, area: SelectionArea) -> Vec<Candidate> {
        self.surface()
            .root
            .ns()
            .window()
            .map_or_else(Vec::new, |window| {
                self.inspection_window_candidates(&window, area)
            })
    }

    /// The renderer root view in `window`, if this AppView owns it. Coordinates
    /// from events should be converted here, not in a foreign window or pixels.
    pub fn inspection_root(&self, window: &NSWindow) -> Option<objc2::rc::Retained<NSView>> {
        crate::window::root_in(self.surface(), window).map(|root| Retained::from(root.ns()))
    }

    /// Candidates in any actual window owned by this surface.
    pub fn inspection_window_candidates(
        &self,
        window: &NSWindow,
        area: SelectionArea,
    ) -> Vec<Candidate> {
        let Some(epoch) = self.inspection_epoch() else {
            return Vec::new();
        };
        let Some(root) = crate::window::root_in(self.surface(), window) else {
            return Vec::new();
        };
        let bounds = root.state().frame.size;
        let clip = shape::rect((0.0, 0.0), (bounds.width.0, bounds.height.0));
        let mut matches = Vec::new();
        let query = Query {
            window: window.windowNumber(),
            epoch,
            area,
        };
        for child in children(&root) {
            visit(&child, &clip, &query, &mut matches);
        }
        matches
    }

    /// The currently mounted engine's identity, separate from host session identity.
    pub fn inspection_epoch(&self) -> Option<Epoch> {
        self.surface().inspection_epoch()
    }

    /// Updates after the native surface has applied mounts and native geometry
    /// changes. One active observer is owned by the embedding inspector.
    pub fn set_inspection_observer(&self, observer: Option<std::rc::Rc<dyn Fn()>>) {
        self.surface().set_inspection_observer(observer);
    }

    /// The current engine for batched, asynchronous inspection commands.
    pub fn inspection_control(&self) -> Option<EngineControl> {
        self.surface().event_control()
    }
}

struct Query {
    window: isize,
    epoch: Epoch,
    area: SelectionArea,
}

fn checked_root(app: &AppView, view: &NshView, window: isize) -> Option<NshView> {
    let win = view.ns().window()?;
    if win.windowNumber() != window {
        return None;
    }
    let root = crate::window::root_in(app.surface(), &win)?;
    view.ns().isDescendantOf(root.ns()).then_some(root)
}

fn children(view: &NshView) -> Vec<NshView> {
    let container = view.container_view();
    let mut out = Vec::new();
    let subviews: Vec<_> = view.ns().subviews().iter().collect();
    for child in subviews.into_iter().rev() {
        if container.as_deref() == Some(&*child) {
            let children: Vec<_> = child.subviews().iter().collect();
            out.extend(
                children
                    .into_iter()
                    .rev()
                    .filter_map(|s| NshView::from_ns(&s)),
            );
        } else if let Some(nsh) = NshView::from_ns(&child) {
            out.push(nsh);
        }
    }
    out
}

fn hidden(view: &NshView) -> bool {
    let s = view.state();
    s.style.hidden
        || s.opacity() <= 0.0
        || view.ns().isHiddenOrHasHiddenAncestor()
        || s.live.as_ref().is_some_and(|l| l.dying())
}

fn outline(view: &NshView) -> Option<Vec<P>> {
    let state = view.state();
    let size = state.frame.size;
    if size.width <= Points::ZERO || size.height <= Points::ZERO {
        return None;
    }
    let mut radii = state.style.radii();
    if state.kind == nsh_core::ViewKind::Scroll {
        // Scroll decor uses the largest corner uniformly on its container.
        radii = CornerRadii::all(radii.to_array().into_iter().reduce(Points::max).unwrap());
        radii.continuous = state.style.continuous_corners;
    }
    let polygon = if radii.to_array().into_iter().any(|p| p > Points::ZERO) {
        shape::outline(size, radii)
    } else {
        shape::rect((0.0, 0.0), (size.width.0, size.height.0))
    };
    drop(state);
    polygon
        .into_iter()
        .map(|p| crate::input::to_top(view, p, true).filter(|p| p.0.is_finite() && p.1.is_finite()))
        .collect()
}

fn candidate(view: &NshView, window: isize, epoch: Epoch, polygon: Vec<P>) -> Option<Candidate> {
    let ((l, t), (r, b)) = shape::bounds(&polygon)?;
    Some(Candidate {
        node: view.id(),
        window,
        epoch,
        view: view.clone(),
        visible: Bounds::new(
            Point::new(Points(l), Points(t)),
            Size::new(Points(r - l), Points(b - t)),
        ),
        outline: polygon
            .into_iter()
            .map(|(x, y)| Point::new(Points(x), Points(y)))
            .collect(),
    })
}

fn showing(view: &NshView, root: &NshView) -> Option<Vec<P>> {
    if hidden(view) {
        return None;
    }
    let mut polygon = outline(view)?;
    let mut current = view.parent_view();
    while let Some(ancestor) = current {
        if hidden(&ancestor) {
            return None;
        }
        if ancestor == *root || ancestor.state().clips() {
            polygon = shape::intersection(polygon, &outline(&ancestor)?);
            if polygon.is_empty() {
                return None;
            }
        }
        if ancestor == *root {
            return Some(polygon);
        }
        current = ancestor.parent_view();
    }
    None
}

fn visit(view: &NshView, clip: &[P], query: &Query, matches: &mut Vec<Candidate>) {
    if hidden(view) {
        return;
    }
    let visible = outline(view).map(|path| shape::intersection(path, clip));
    let child_clip = if view.state().clips() {
        let Some(path) = visible.as_deref().filter(|v| !v.is_empty()) else {
            return;
        };
        path
    } else {
        clip
    };
    if hit(child_clip, query.area) {
        for child in children(view) {
            visit(&child, child_clip, query, matches);
        }
    }
    if let Some(polygon) = visible.filter(|path| hit(path, query.area))
        && let Some(c) = candidate(view, query.window, query.epoch, polygon)
    {
        matches.push(c);
    }
}

fn hit(shape: &[P], area: SelectionArea) -> bool {
    match area {
        SelectionArea::Point(p) => shape::contains(shape, (p.x.0, p.y.0)),
        SelectionArea::Region(r) => !shape::intersection(
            shape::rect(
                (r.origin.x.0, r.origin.y.0),
                (
                    r.origin.x.0 + r.size.width.0,
                    r.origin.y.0 + r.size.height.0,
                ),
            ),
            shape,
        )
        .is_empty(),
    }
}
