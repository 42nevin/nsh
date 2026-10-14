//! Virtualized list state: item sizes, offsets, the render window and cell
//! recycling. Pure bookkeeping; the core feeds it measurements and scroll
//! offsets and applies its decisions to the tree.

use rustc_hash::FxHashMap;

use crate::fenwick::Fenwick;

/// How far beyond the viewport cells are kept rendered, in viewports.
const OVERSCAN: f32 = 1.0;
/// `onEndReached` fires when the viewport end is this close to the content
/// end, in viewports.
const END_THRESHOLD: f32 = 1.0;

/// A recycling slot: one cell node, bound to at most one item.
#[derive(Debug, Clone, Copy)]
pub(super) struct Slot {
    /// Cell node key (`u32::MAX` until the runtime created the cell).
    pub(super) cell: u32,
    /// Item this slot is assigned to.
    pub(super) index: Option<u32>,
    /// Item the cell's content currently shows (confirmed by the runtime).
    pub(super) rendered: Option<u32>,
}

impl Slot {
    /// True when the cell shows its assigned item, so its size and
    /// position belong to that item.
    pub(super) fn ready(&self) -> Option<u32> {
        match (self.index, self.rendered) {
            (Some(i), Some(r)) if i == r && self.cell != u32::MAX => Some(i),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(super) struct ListState {
    pub(super) count: u32,
    estimated: f32,
    /// Measured main-axis sizes; NaN = not measured yet.
    sizes: Vec<f32>,
    offsets: Fenwick,
    pub(super) slots: Vec<Slot>,
    by_index: FxHashMap<u32, u32>,
    free: Vec<u32>,
    /// Main-axis scroll offset reported by the platform.
    pub(super) offset: f32,
    /// Main-axis size of the list viewport (from layout).
    pub(super) viewport: f32,
    /// Window, bindings or positions need recomputing.
    pub(super) dirty: bool,
    /// Cross-axis size the cells were last laid out at.
    pub(super) cells_cross: f32,
    end_reached_at: Option<u32>,
}

impl ListState {
    pub(super) fn new() -> Self {
        Self {
            count: 0,
            estimated: 50.0,
            sizes: Vec::new(),
            offsets: Fenwick::default(),
            slots: Vec::new(),
            by_index: FxHashMap::default(),
            free: Vec::new(),
            offset: 0.0,
            viewport: 0.0,
            dirty: true,
            cells_cross: f32::NAN,
            end_reached_at: None,
        }
    }

    fn effective(&self, i: usize) -> f64 {
        let s = self.sizes[i];
        if s.is_nan() {
            self.estimated as f64
        } else {
            s as f64
        }
    }

    fn rebuild_offsets(&mut self) {
        self.offsets = Fenwick::from_sizes((0..self.sizes.len()).map(|i| self.effective(i)));
    }

    pub(super) fn set_count(&mut self, count: u32) {
        if count == self.count {
            return;
        }
        self.count = count;
        self.sizes.resize(count as usize, f32::NAN);
        self.rebuild_offsets();
        // Items past the end lose their slots on the next window update.
        self.dirty = true;
        if self.end_reached_at.is_some_and(|c| c != count) {
            self.end_reached_at = None;
        }
    }

    pub(super) fn set_estimated(&mut self, estimated: f32) {
        if estimated > 0.0 && estimated != self.estimated {
            self.estimated = estimated;
            self.rebuild_offsets();
            self.dirty = true;
        }
    }

    /// Forgets all measurements (orientation change).
    pub(super) fn clear_sizes(&mut self) {
        self.sizes.fill(f32::NAN);
        self.rebuild_offsets();
        self.dirty = true;
    }

    /// Offset of item `i` along the main axis.
    pub(super) fn offset_of(&self, i: u32) -> f32 {
        self.offsets.prefix(i as usize) as f32
    }

    /// Total content length along the main axis.
    pub(super) fn total(&self) -> f32 {
        self.offsets.total() as f32
    }

    /// Records a measured size. Returns the change if the item lies
    /// entirely above the viewport (the caller scrolls by it so the
    /// visible content stays put).
    pub(super) fn measured(&mut self, i: u32, size: f32) -> f32 {
        let idx = i as usize;
        if idx >= self.sizes.len() || self.sizes[idx] == size {
            return 0.0;
        }
        let delta = size as f64 - self.effective(idx);
        let above_viewport = self.offset_of(i + 1) <= self.offset;
        self.sizes[idx] = size;
        self.offsets.add(idx, delta);
        self.dirty = true;
        if above_viewport { delta as f32 } else { 0.0 }
    }

    pub(super) fn add_slot_cell(&mut self, slot: u32, cell: u32) {
        let s = slot as usize;
        if s >= self.slots.len() {
            self.slots.resize(
                s + 1,
                Slot {
                    cell: u32::MAX,
                    index: None,
                    rendered: None,
                },
            );
        }
        self.slots[s].cell = cell;
        self.dirty = true;
    }

    pub(super) fn remove_cell(&mut self, cell: u32) {
        for slot in &mut self.slots {
            if slot.cell == cell {
                slot.cell = u32::MAX;
                slot.rendered = None;
                self.dirty = true;
            }
        }
    }

    pub(super) fn rendered(&mut self, cell: u32, index: u32) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.cell == cell) {
            slot.rendered = Some(index);
            self.dirty = true;
        }
    }

    /// Recomputes the render window. New `(slot, index)` bindings are
    /// pushed to `binds`. Returns true if `onEndReached` should fire.
    pub(super) fn update_window(&mut self, binds: &mut Vec<(u32, u32)>) -> bool {
        self.dirty = false;
        if self.count == 0 || self.viewport <= 0.0 {
            for slot in 0..self.slots.len() {
                self.unbind(slot as u32);
            }
            return false;
        }
        let overscan = self.viewport * OVERSCAN;
        let start = (self.offset - overscan).max(0.0);
        let end = self.offset + self.viewport + overscan;
        let first = self.offsets.find(start as f64) as u32;
        let last = (self.offsets.find(end as f64) as u32).min(self.count - 1);

        for slot in 0..self.slots.len() as u32 {
            if let Some(i) = self.slots[slot as usize].index
                && !(first..=last).contains(&i)
            {
                self.unbind(slot);
            }
        }
        for i in first..=last {
            if self.by_index.contains_key(&i) {
                continue;
            }
            let slot = match self.free.pop() {
                Some(slot) => slot,
                None => {
                    self.slots.push(Slot {
                        cell: u32::MAX,
                        index: None,
                        rendered: None,
                    });
                    self.slots.len() as u32 - 1
                }
            };
            self.slots[slot as usize].index = Some(i);
            self.by_index.insert(i, slot);
            binds.push((slot, i));
        }

        let near_end = self.offset + self.viewport >= self.total() - self.viewport * END_THRESHOLD;
        if near_end && self.end_reached_at != Some(self.count) {
            self.end_reached_at = Some(self.count);
            return true;
        }
        false
    }

    fn unbind(&mut self, slot: u32) {
        if let Some(i) = self.slots[slot as usize].index.take() {
            self.by_index.remove(&i);
            self.free.push(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(count: u32, estimated: f32, viewport: f32) -> ListState {
        let mut l = ListState::new();
        l.set_estimated(estimated);
        l.set_count(count);
        l.viewport = viewport;
        l
    }

    #[test]
    fn window_covers_viewport_plus_overscan() {
        let mut l = list(1000, 50.0, 400.0);
        let mut binds = Vec::new();
        l.update_window(&mut binds);
        // [0, 800] at 50 per item -> items 0..=16.
        assert_eq!(binds.len(), 17);
        assert_eq!(binds[0], (0, 0));

        // Scroll down a screen: items scrolled far above are recycled.
        l.offset = 2000.0;
        binds.clear();
        l.update_window(&mut binds);
        let indices: Vec<u32> = binds.iter().map(|b| b.1).collect();
        assert_eq!(indices.first(), Some(&32));
        assert_eq!(indices.last(), Some(&56));
        // Slots were reused, not grown without bound.
        assert!(l.slots.len() <= 25 + 17, "{}", l.slots.len());
    }

    #[test]
    fn measurements_shift_offsets_and_anchor_above_viewport() {
        let mut l = list(100, 50.0, 400.0);
        l.offset = 1000.0;
        // Item 0 is above the viewport: growing it must scroll by the same.
        assert_eq!(l.measured(0, 80.0), 30.0);
        assert_eq!(l.offset_of(1), 80.0);
        assert_eq!(l.total(), 100.0 * 50.0 + 30.0);
        // Item 30 is in view: no anchoring.
        assert_eq!(l.measured(30, 10.0), 0.0);
    }

    #[test]
    fn end_reached_fires_once_per_count() {
        let mut l = list(10, 50.0, 400.0);
        let mut binds = Vec::new();
        assert!(
            l.update_window(&mut binds),
            "500 px of content in a 400 px viewport"
        );
        assert!(!l.update_window(&mut binds));
        l.set_count(20);
        l.offset = 600.0;
        assert!(l.update_window(&mut binds));
    }

    #[test]
    fn shrinking_count_unbinds() {
        let mut l = list(100, 50.0, 400.0);
        let mut binds = Vec::new();
        l.update_window(&mut binds);
        l.set_count(3);
        binds.clear();
        l.update_window(&mut binds);
        assert_eq!(l.slots.iter().filter(|s| s.index.is_some()).count(), 3);
    }
}
