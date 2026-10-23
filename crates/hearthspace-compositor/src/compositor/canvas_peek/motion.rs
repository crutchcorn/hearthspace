//! Temporary world offsets animate movement to already-settled canvas positions.
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use hearthspace_common::{
    canvas_search::motion::reveal_ease,
    frame_layout::{ObjectPlacement, Point as WorldPoint},
};
use smithay::utils::{Logical, Point};

use crate::compositor::{App, ManagedWindowKind};

const DURATION: Duration = Duration::from_millis(300);

pub(super) struct State {
    offsets: BTreeMap<u64, Offset>,
    sampled_at: Instant,
}

impl Default for State {
    fn default() -> Self {
        Self {
            offsets: BTreeMap::new(),
            sampled_at: Instant::now(),
        }
    }
}

struct Offset {
    from: WorldPoint,
    started: Instant,
}

impl Offset {
    fn current(&self, now: Instant) -> WorldPoint {
        let progress =
            now.saturating_duration_since(self.started).as_secs_f64() / DURATION.as_secs_f64();
        let remaining = 1.0 - reveal_ease(progress);
        WorldPoint {
            x: self.from.x * remaining,
            y: self.from.y * remaining,
        }
    }
}

impl State {
    fn needs_animation_frame(&self) -> bool {
        // An overdue offset still needs a final advance and redraw to retire it.
        !self.offsets.is_empty()
    }

    fn offset(&self, id: u64) -> WorldPoint {
        self.offsets
            .get(&id)
            .map_or(WorldPoint::default(), |offset| {
                offset.current(self.sampled_at)
            })
    }

    fn screen_offset(&self, id: u64, scale: f64) -> Point<f64, Logical> {
        let offset = self.offset(id);
        // Only the viewport's scale applies to a displacement, never its pan.
        (offset.x * scale, offset.y * scale).into()
    }

    fn queue(&mut self, id: u64, delta: WorldPoint, reduced: bool, now: Instant) {
        if reduced || !delta.x.is_finite() || !delta.y.is_finite() {
            self.offsets.remove(&id);
            return;
        }
        if delta == WorldPoint::default() {
            return;
        }
        // Retarget from the last sampled display position, not a later clock
        // reading. The caller has already applied new canonical coordinates.
        let current = self.offset(id);
        let from = WorldPoint {
            x: current.x + delta.x,
            y: current.y + delta.y,
        };
        if from == WorldPoint::default() {
            self.offsets.remove(&id);
        } else {
            self.offsets.insert(id, Offset { from, started: now });
        }
    }

    fn advance(&mut self, now: Instant, reduced: bool, eligible: impl Fn(u64) -> bool) -> bool {
        self.sampled_at = now;
        let was_active = self.needs_animation_frame();
        self.offsets.retain(|id, offset| {
            !reduced && now.saturating_duration_since(offset.started) < DURATION && eligible(*id)
        });
        was_active
    }
}

impl App {
    /// Queue `old_position - new_position` after changing the canonical position.
    pub(in crate::compositor) fn queue_canvas_peek_motion(&mut self, id: u64, delta: WorldPoint) {
        let allowed = !self.canvas_peek_motion_suppressed()
            && self
                .windows
                .iter()
                .position(|window| window.id == id)
                .is_some_and(|index| self.canvas_peek_motion_allowed(index));
        self.canvas_peek
            .motion
            .queue(id, delta, !allowed, Instant::now());
        self.request_redraw();
    }

    pub(in crate::compositor) fn cancel_canvas_peek_motion(&mut self, id: u64) {
        if self.canvas_peek.motion.offsets.remove(&id).is_some() {
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn clear_canvas_peek_motion(&mut self) {
        if self.canvas_peek.motion.needs_animation_frame() {
            self.canvas_peek.motion.offsets.clear();
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn advance_canvas_peek_motion(&mut self) {
        if !self.canvas_peek.motion.needs_animation_frame() {
            return;
        }
        let suppressed = self.canvas_peek_motion_suppressed();
        let eligible: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.canvas_peek_motion_allowed(*index))
            .map(|(_, window)| window.id)
            .collect();
        if self
            .canvas_peek
            .motion
            .advance(Instant::now(), suppressed, |id| eligible.contains(&id))
        {
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn canvas_peek_needs_animation_frame(&self) -> bool {
        // Keep the retirement wake even after reduced motion is enabled.
        self.canvas_peek.motion.needs_animation_frame()
    }

    /// One frame's offset in screen logical units, shared by input and rendering.
    pub(in crate::compositor) fn canvas_peek_offset(&self, index: usize) -> Point<f64, Logical> {
        let Some(window) = self.windows.get(index) else {
            return (0.0, 0.0).into();
        };
        if !self.canvas_peek.motion.offsets.contains_key(&window.id)
            || self.canvas_peek_motion_suppressed()
            || !self.canvas_peek_motion_allowed(index)
        {
            return (0.0, 0.0).into();
        }
        self.canvas_peek
            .motion
            .screen_offset(window.id, self.viewport_scale)
    }

    fn canvas_peek_motion_suppressed(&self) -> bool {
        self.layout_reduced_motion() || self.canvas_preview_presenting() || self.overview_active()
    }

    fn canvas_peek_motion_allowed(&self, index: usize) -> bool {
        let Some(window) = self.windows.get(index) else {
            return false;
        };
        let control = self.frame_widget_identity(index).is_some();
        if !window.mapped
            || !(self.is_navigable_window(index)
                || window.kind == ManagedWindowKind::DesktopItem
                || control)
            || self.window_uses_output_coordinates(index)
            || self.layout_motion_rect(index).is_some()
            || (!control && self.layout_target_rect(index).is_some())
        {
            return false;
        }
        if let Some(target) = self.frame_target_for_window(index)
            && self.layout_editor.placed.iter().any(|placed| {
                placed.target == target
                    && (placed.placement.is_frame()
                        || placed.placement == ObjectPlacement::Hidden
                        || placed.group.is_some())
            })
        {
            return false;
        }
        // is_navigable_window excludes X11 menus, which already inherit their
        // parent's offset. Ordinary canvas transients keep their own positions.
        true
    }
}

#[cfg(test)]
#[path = "motion_tests.rs"]
mod tests;
