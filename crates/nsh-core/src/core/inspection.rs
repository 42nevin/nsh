//! Instance-scoped inspection of live native views. Host session and window
//! identities belong to the embedder; a target here belongs to one core.
use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::{Core, Kind, NO_KEY, dirty};
use crate::{NodeId, Points, TextEngine};

/// A node incarnation within one running surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ViewTarget {
    /// Renderer node identifier.
    pub node: NodeId,
    /// Changes when the node is released and its identifier reused.
    pub generation: u64,
}

/// The common corner radius before and after an optional preview. Individual
/// corner declarations still take precedence, as in the normal style cascade.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Radius {
    /// Latest underlying common radius (points), not an old style snapshot.
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
    /// Application-supplied identifier, independent of runtime identity.
    pub test_id: Option<Arc<str>>,
    /// Writable common radius and the current local preview.
    pub radius: Radius,
}

/// A preview was refused without changing the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InspectError {
    /// The node is no longer the selected incarnation.
    Stale,
    /// The value is not finite or is negative.
    InvalidRadius,
}

#[derive(Default)]
pub(super) struct Inspection {
    next_generation: u64,
    // Allocated only for views inspected by the host, never a mirror tree.
    entries: FxHashMap<NodeId, (ViewTarget, Option<Points>)>,
}

impl Inspection {
    pub(super) fn forget(&mut self, id: NodeId) {
        self.entries.remove(&id);
    }

    pub(super) fn radius(&self, id: NodeId) -> Option<Points> {
        self.entries.get(&id).and_then(|(_, radius)| *radius)
    }
}

impl<M: TextEngine> Core<M> {
    /// Selects a mounted ordinary view. Recycled list subtrees do not yet
    /// expose a stable logical-item incarnation through this interface.
    pub fn inspect_view(&mut self, id: NodeId) -> Option<InspectedView> {
        let key = self.key(id).ok()?;
        let node = &self.nodes[key as usize];
        if !node.created || node.kind != Kind::View || node.inline || key == self.root {
            return None;
        }
        let mut parent = node.parent;
        while parent != NO_KEY {
            let ancestor = &self.nodes[parent as usize];
            if ancestor.kind == Kind::Cell {
                return None;
            }
            parent = ancestor.parent;
        }
        let (target, preview) = *self.inspection.entries.entry(id).or_insert_with(|| {
            self.inspection.next_generation = self
                .inspection
                .next_generation
                .checked_add(1)
                .expect("view incarnation exhausted");
            (
                ViewTarget {
                    node: id,
                    generation: self.inspection.next_generation,
                },
                None,
            )
        });
        Some(InspectedView {
            target,
            test_id: node.props.as_ref().and_then(|props| props.test_id.clone()),
            radius: Radius {
                underlying: node.view_style.border_radius,
                preview,
            },
        })
    }

    /// Sets or removes a preview through the normal view-style mount. The
    /// renderer continues to own and update the underlying declaration.
    pub fn preview_radius(
        &mut self,
        target: ViewTarget,
        radius: Option<Points>,
    ) -> Result<Radius, InspectError> {
        let (current, previous) = self
            .inspection
            .entries
            .get_mut(&target.node)
            .ok_or(InspectError::Stale)?;
        if *current != target {
            return Err(InspectError::Stale);
        }
        if radius.is_some_and(|r| !r.0.is_finite() || r.0 < 0.0) {
            return Err(InspectError::InvalidRadius);
        }
        let changed = *previous != radius;
        *previous = radius;
        let key = self
            .key(target.node)
            .expect("an inspected view remains live");
        if changed {
            self.mark(key, dirty::VIEW);
        }
        Ok(Radius {
            underlying: self.nodes[key as usize].view_style.border_radius,
            preview: radius,
        })
    }
}
