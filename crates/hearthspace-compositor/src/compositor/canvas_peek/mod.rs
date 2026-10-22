//! Release-time canvas settlement. Canonical targets persist; only travel is projected.
use std::collections::BTreeMap;

use hearthspace_common::{
    canvas_peek::{self, Scene},
    frame_layout::{Point, Rect},
};

use super::App;

mod layout_drop;
mod motion;
mod scene;
mod transactions;

#[derive(Default)]
pub(super) struct State {
    motion: motion::State,
    pub(super) homes: BTreeMap<String, Point>,
    previous: BTreeMap<u64, scene::Snapshot>,
    drops: Vec<u64>,
    restore_requested: bool,
}

impl State {
    pub(super) fn with_homes(homes: BTreeMap<String, Point>) -> Self {
        Self {
            homes,
            ..Self::default()
        }
    }
}

impl App {
    pub(in crate::compositor) fn reconcile_canvas_peek(&mut self) {
        let motion_was_active = self.canvas_peek_needs_animation_frame();
        self.advance_canvas_peek_motion();
        if motion_was_active {
            self.finish_canvas_peek_returns();
        }
        // Search/switching own temporary layouts. Focused views never push the
        // scene underneath them; neither do intermediate move/resize samples.
        if self.session_locked()
            || self.canvas_preview_presenting()
            || self.overview_active()
            || self
                .windows
                .iter()
                .any(|w| w.mapped && w.presentation.output_pinned())
        {
            self.clear_canvas_peek_motion();
            return;
        }
        if self.drag.is_some()
            || self.canvas_peek_transaction_pending()
            || self
                .windows
                .iter()
                .any(|w| w.mapped && w.presentation.restoring)
            || self.desktop_gesture_active()
            || self.layout_drag_active()
            || self.group_has_pointer_capture()
            || self.group_animating()
        {
            return;
        }
        let (mut scene, snapshot) = self.canvas_peek_scene();
        if snapshot == self.canvas_peek.previous
            && self.canvas_peek.drops.is_empty()
            && !self.canvas_peek.restore_requested
        {
            return;
        }
        self.canvas_peek.restore_requested = false;
        // A deliberate move establishes a fresh home. Automatic moves update
        // the snapshot below, so subsequent native commits retain that home.
        for item in &mut scene.decorations {
            if let Some(old) = self.canvas_peek.previous.get(&item.id)
                && let Some(new) = snapshot.get(&item.id)
                && (old.bounds.x != new.bounds.x
                    || old.bounds.y != new.bounds.y
                    || old.background != new.background)
            {
                item.home = None;
            }
        }
        let before = scene.clone();
        let drops = std::mem::take(&mut self.canvas_peek.drops);
        if drops.is_empty() {
            let only_removals = snapshot
                .iter()
                .all(|(id, entry)| self.canvas_peek.previous.get(id) == Some(entry));
            if only_removals {
                canvas_peek::restore_background(&mut scene);
            } else {
                canvas_peek::settle(&mut scene, None);
            }
        } else {
            for id in drops {
                layout_drop::settle(&mut scene, id);
            }
        }
        self.apply_canvas_peek(&before, &scene);
        self.canvas_peek.previous = self.canvas_peek_scene().1;
    }

    fn apply_canvas_peek(&mut self, before: &Scene, after: &Scene) {
        let mut moved = false;
        for (id, from, to) in before
            .windows
            .iter()
            .filter_map(|old| {
                after
                    .windows
                    .iter()
                    .find(|new| new.id == old.id)
                    .map(|new| (old.id, old.bounds, new.bounds))
            })
            .chain(before.decorations.iter().filter_map(|old| {
                after
                    .decorations
                    .iter()
                    .find(|new| new.id == old.id)
                    .map(|new| (old.id, old.bounds, new.bounds))
            }))
        {
            let Some(index) = self.windows.iter().position(|w| w.id == id) else {
                continue;
            };
            let delta = Point {
                x: to.x - from.x,
                y: to.y - from.y,
            };
            if delta.x.abs() < 0.001 && delta.y.abs() < 0.001 {
                continue;
            }
            let origin = self
                .layout_motion_rect(index)
                .map(|pose| Point {
                    x: pose.x / self.viewport_scale + f64::from(self.viewport_offset.x),
                    y: pose.y / self.viewport_scale + f64::from(self.viewport_offset.y),
                })
                .unwrap_or(Point {
                    x: from.x,
                    y: from.y,
                });
            self.queue_layout_motion(id, Rect::default(), Rect::default(), 0);
            self.translate_canvas_peek_target(index, delta);
            self.queue_canvas_peek_motion(
                id,
                Point {
                    x: origin.x - to.x,
                    y: origin.y - to.y,
                },
            );
            moved = true;
        }
        for item in &after.decorations {
            if let Some(index) = self.windows.iter().position(|w| w.id == item.id)
                && let Some(key) = self.canvas_peek_key(index)
            {
                if let Some(home) = item.home {
                    self.canvas_peek.homes.insert(key, home);
                } else if self.canvas_peek_offset(index) != (0., 0.).into()
                    && let Some(home) = before
                        .decorations
                        .iter()
                        .find(|old| old.id == item.id)
                        .and_then(|old| old.home)
                    && item.bounds.x == home.x
                    && item.bounds.y == home.y
                {
                    // Keep the destination until the return is visibly done.
                    // A click can still interrupt this slide before arriving.
                    self.canvas_peek.homes.insert(key, home);
                } else {
                    self.canvas_peek.homes.remove(&key);
                }
            }
        }
        if moved {
            self.refresh_peek_layout_placements();
            self.desktop_changed();
            self.request_redraw();
        }
    }

    /// A gesture begins at the currently displayed position, including when it
    /// interrupts a slide. Settlement still waits for its eventual release.
    pub(in crate::compositor) fn grab_canvas_peek_target(&mut self, index: usize) {
        let id = self.windows[index].id;
        let returning = self
            .canvas_peek_key(index)
            .and_then(|key| self.canvas_peek.homes.get(&key))
            .zip(self.canvas_peek.previous.get(&id))
            .is_some_and(|(home, snapshot)| {
                home.x == snapshot.bounds.x && home.y == snapshot.bounds.y
            });
        let offset = self.canvas_peek_offset(index);
        let delta = Point {
            x: offset.x / self.viewport_scale,
            y: offset.y / self.viewport_scale,
        };
        self.cancel_canvas_peek_motion(id);
        if delta.x.abs() > 0.001 || delta.y.abs() > 0.001 {
            self.canvas_peek.restore_requested |= returning;
            self.translate_canvas_peek_target(index, delta);
            self.refresh_peek_layout_placements();
            // Adopting a slide's visible pose is not a manual placement. Use
            // it as the gesture baseline so a click or cancelled drag keeps
            // the original home. A subsequent committed move still clears it.
            if self.canvas_peek.previous.contains_key(&id)
                && let Some(snapshot) = self.canvas_peek_scene().1.remove(&id)
            {
                self.canvas_peek.previous.insert(id, snapshot);
            }
        }
    }

    fn finish_canvas_peek_returns(&mut self) {
        if self.canvas_peek.homes.is_empty() {
            return;
        }
        for item in self.canvas_peek_scene().0.decorations {
            if let Some(home) = item.home
                && item.bounds.x == home.x
                && item.bounds.y == home.y
                && let Some(index) = self.windows.iter().position(|w| w.id == item.id)
                && self.canvas_peek_offset(index) == (0., 0.).into()
            {
                self.forget_canvas_peek_home(index);
            }
        }
    }

    pub(in crate::compositor) fn canvas_peek_layout_drop(&mut self, id: u64) {
        if !self.canvas_peek.drops.contains(&id) {
            self.canvas_peek.drops.push(id);
        }
    }

    /// Gathering establishes its own window arrangement. Only displaced
    /// decorations return afterward; unrelated scene changes still settle.
    pub(in crate::compositor) fn canvas_peek_arrangement_finished(&mut self, ids: &[u64]) {
        let (_, snapshot) = self.canvas_peek_scene();
        for id in ids {
            if let Some(placed) = snapshot.get(id) {
                self.canvas_peek.previous.insert(*id, placed.clone());
            }
        }
        self.canvas_peek.restore_requested = true;
    }
}

fn rect(r: smithay::utils::Rectangle<i32, smithay::utils::Logical>) -> Rect {
    Rect {
        x: f64::from(r.loc.x),
        y: f64::from(r.loc.y),
        width: f64::from(r.size.w),
        height: f64::from(r.size.h),
    }
}
