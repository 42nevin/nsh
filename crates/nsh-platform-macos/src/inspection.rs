//! Native geometry for a development inspector. Selection is independent of
//! input hit testing: noninteractive decoration remains a candidate.
use nsh_core::{Bounds, NodeId, Point, Points, Size};
use nsh_engine::EngineControl;
use objc2_app_kit::NSView;

use crate::{AppView, native::place_in, view::NshView};

/// A query in the surface root's flipped logical points (not backing pixels).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum SelectionArea {
    /// The frontmost matching element and alternatives beneath it.
    Point(Point<Points>),
    /// Elements intersecting the retained region.
    Region(Bounds<Points>),
}

/// A mounted candidate. This identifier is for discovery; use the engine's
/// incarnation-qualified target when requesting a property change.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Candidate {
    /// Node identifier in this surface.
    pub node: NodeId,
    /// Visible rectangle after clipping to ancestors and the surface.
    pub visible: Bounds<Points>,
    view: NshView,
}

impl Candidate {
    /// Samples the current backing layer, not the requested style. The native
    /// radius can differ from the declaration (for example a short view clamps
    /// its corners). A retired candidate has no measurement.
    pub fn mounted_corner_radius(&self, app: &AppView) -> Option<Points> {
        self.is_current(app)
            .then(|| self.view.ns().layer())
            .flatten()
            .map(|layer| Points(layer.cornerRadius() as f32))
    }

    /// Still the same mounted view, not a different view reusing its id.
    pub fn is_current(&self, app: &AppView) -> bool {
        app.surface().mounter.borrow().views.get(&self.node) == Some(&self.view)
    }
}

impl AppView {
    /// Front-to-back, descendants before their containing view. Inspector UI
    /// hosted outside the surface root never participates in this traversal.
    pub fn inspection_candidates(&self, area: SelectionArea) -> Vec<Candidate> {
        let root = &self.surface().root;
        let mut matches = Vec::new();
        for child in root.child_views().into_iter().rev() {
            visit(&child, root.ns(), area, &mut matches);
        }
        matches
    }

    /// The current engine for batched, asynchronous inspection commands.
    /// A replaced engine does not acquire the previous engine's requests.
    pub fn inspection_control(&self) -> Option<EngineControl> {
        self.surface().event_control()
    }
}

fn visit(view: &NshView, root: &NSView, area: SelectionArea, matches: &mut Vec<Candidate>) {
    if view.ns().isHiddenOrHasHiddenAncestor() {
        return;
    }
    for child in view.child_views().into_iter().rev() {
        visit(&child, root, area, matches);
    }
    let Some(place) = place_in(view.ns(), root) else {
        return;
    };
    let r = place.clip;
    let visible = Bounds::new(
        Point::new(Points(r.origin.x as f32), Points(r.origin.y as f32)),
        Size::new(Points(r.size.width as f32), Points(r.size.height as f32)),
    );
    if match area {
        SelectionArea::Point(p) => visible.contains(&p),
        SelectionArea::Region(r) => visible.intersects(&r),
    } {
        matches.push(Candidate {
            node: view.id(),
            view: view.clone(),
            visible,
        });
    }
}
