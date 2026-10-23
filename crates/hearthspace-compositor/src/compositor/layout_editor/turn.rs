//! Toolbar orientation changes rotate the composed native control around its grabbed point.
use super::{App, Target, turn_cache::TurnCache};
use hearthspace_common::{
    canvas_search::motion::ease,
    frame_layout::{Point, Rect},
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub(super) const TURN_DURATION: Duration = Duration::from_millis(280);

#[derive(Default)]
pub(super) struct State {
    pub turns: BTreeMap<String, Turn>,
    pub cache: TurnCache,
    pub presented: BTreeMap<String, Presented>,
}

impl State {
    /// Retire completed/invalid turns and report whether their pixels changed.
    fn retire(&mut self, now: Instant, reduced: bool, valid: &[String]) -> bool {
        // Pending reflow has no advancing clock. Redrawing its unchanged cached
        // pixels for every Wayland request delays clients' resize roundtrips.
        // Input and surface commits already request a frame when pixels change.
        let running = self.turns.values().any(Turn::needs_animation_frame);
        let previous_count = self.turns.len();
        self.turns
            .retain(|key, turn| !reduced && !turn.finished(now) && valid.contains(key));
        running || self.turns.len() != previous_count
    }
}

#[derive(Clone)]
pub(super) struct Presented {
    pub bounds: Rect,
    pub pivot: Point,
    pub angle: f64,
    pub members: Vec<(u64, Rect)>,
    pub vertical: bool,
}

pub(super) struct Turn {
    pub target: Target,
    pub members: Vec<u64>,
    pub bounds: Rect,
    pub pivot: Point,
    pub from_angle: f64,
    pub translation: Point,
    pub pointer_anchor: bool,
    pub started: Option<Instant>,
}

impl Turn {
    fn needs_animation_frame(&self) -> bool {
        // Retain timer demand through the final advance, even if the deadline
        // passed between the previous render and the next event-loop dispatch.
        self.started.is_some()
    }

    fn remaining(&self, now: Instant) -> f64 {
        self.started.map_or(1., |started| {
            1. - ease(
                now.saturating_duration_since(started).as_secs_f64() / TURN_DURATION.as_secs_f64(),
            )
        })
    }

    pub fn angle(&self, now: Instant) -> f64 {
        self.from_angle * self.remaining(now)
    }

    pub fn offset(&self, now: Instant) -> Point {
        let remaining = self.remaining(now);
        Point {
            x: self.translation.x * remaining,
            y: self.translation.y * remaining,
        }
    }

    fn finished(&self, now: Instant) -> bool {
        self.started
            .is_some_and(|started| now.saturating_duration_since(started) >= TURN_DURATION)
    }
}

impl App {
    pub(super) fn layout_turn_animating(&self) -> bool {
        self.layout_editor
            .turn
            .turns
            .values()
            .any(|turn| !turn.finished(Instant::now()))
    }
    pub(super) fn layout_turn_needs_animation_frame(&self) -> bool {
        self.layout_editor
            .turn
            .turns
            .values()
            .any(Turn::needs_animation_frame)
    }
    pub(super) fn layout_target_turn_active(&self, target: &Target) -> bool {
        self.layout_editor.turn.turns.contains_key(&format!(
            "{}/{}",
            target.output,
            target.target.key()
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::compositor) fn queue_layout_turn(
        &mut self,
        target: Target,
        members: Vec<u64>,
        bounds: Rect,
        pivot: Point,
        from_angle: f64,
        translation: Point,
    ) {
        let key = format!("{}/{}", target.output, target.target.key());
        if self.layout_reduced_motion() || members.is_empty() {
            self.layout_editor.turn.turns.remove(&key);
            return;
        }
        let now = Instant::now();
        let previous = self
            .layout_editor
            .turn
            .turns
            .get(&key)
            .map_or(0., |turn| turn.angle(now));
        let angle = (previous + from_angle)
            .clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
        self.layout_editor.motion.remove(&members);
        self.layout_editor.turn.turns.insert(
            key,
            Turn {
                target,
                members,
                bounds,
                pivot,
                from_angle: angle,
                translation,
                pointer_anchor: self.layout_editor.drag.is_some()
                    || self.layout_editor.drop_anchor.is_some(),
                started: None,
            },
        );
        self.request_redraw();
    }

    pub(in crate::compositor) fn advance_layout_turn(&mut self) {
        // Xilem windows commit independently. Wait for one coherent final layout, then
        // start the same clock for the rotation and its anchor-to-destination translation.
        let ready: Vec<_> = self
            .layout_editor
            .turn
            .turns
            .iter()
            .filter(|(_, turn)| turn.started.is_none() && self.layout_turn_ready(turn))
            .map(|(key, _)| key.clone())
            .collect();
        let now = self.layout_editor.motion.sampled_at;
        for key in ready {
            let target = self.layout_editor.turn.turns[&key].target.clone();
            let bounds = self.frame_placed(&target).unwrap().rect;
            let held = self.layout_drag_vertical(&target).is_some();
            let turn = self.layout_editor.turn.turns.get_mut(&key).unwrap();
            let source = Point {
                x: turn.pivot.x + turn.translation.x,
                y: turn.pivot.y + turn.translation.y,
            };
            let pivot = Point {
                x: bounds.x
                    + bounds.width * (turn.pivot.x - turn.bounds.x) / turn.bounds.width.max(1.),
                y: bounds.y
                    + bounds.height * (turn.pivot.y - turn.bounds.y) / turn.bounds.height.max(1.),
            };
            turn.translation = if held {
                Point { x: 0., y: 0. }
            } else {
                Point {
                    x: source.x - pivot.x,
                    y: source.y - pivot.y,
                }
            };
            turn.bounds = bounds;
            turn.pivot = pivot;
            turn.started = Some(now);
        }
        let reduced = self.layout_reduced_motion();
        let valid: Vec<_> = self
            .layout_editor
            .turn
            .turns
            .iter()
            .filter(|(_, turn)| {
                self.frame_placed(&turn.target).is_some()
                    && self.layout_turn_members(&turn.target) == turn.members
            })
            .map(|(key, _)| key.clone())
            .collect();
        if self.layout_editor.turn.retire(now, reduced, &valid) {
            self.request_redraw();
        }
    }

    fn layout_turn_ready(&self, turn: &Turn) -> bool {
        if self.frame_placed(&turn.target).is_none()
            || self.layout_turn_members(&turn.target) != turn.members
        {
            return false;
        }
        turn.members.iter().all(|id| {
            let Some(index) = self
                .windows
                .iter()
                .position(|window| window.id == *id && window.mapped)
            else {
                return false;
            };
            // Live app slots deliberately crop a larger app; only native toolbar
            // surfaces must reflow to the final layout dimensions.
            if !self.is_layout_surface(index) {
                return true;
            }
            let Some(target) = self.frame_target_for_window(index) else {
                return false;
            };
            let Some(placed) = self.frame_placed(&target) else {
                return false;
            };
            let Some(bounds) = self.layout_target_rect(index) else {
                return false;
            };
            let scale = if placed.placement.is_frame() {
                1.
            } else {
                self.viewport_scale
            };
            let native = self.window_geometry_size(index);
            (f64::from(native.w) - bounds.width / scale).abs() <= 1.
                && (f64::from(native.h) - bounds.height / scale).abs() <= 1.
        })
    }

    pub(in crate::compositor) fn clear_layout_turn(&mut self) {
        self.layout_editor.turn.turns.clear();
        self.layout_editor.turn.cache = Default::default();
        self.layout_editor.turn.presented.clear();
        self.request_redraw();
    }
}

pub(super) fn rotated_bounds(bounds: Rect, pivot: Point, angle: f64) -> Rect {
    let (sin, cos) = angle.sin_cos();
    let points = [
        (bounds.x, bounds.y),
        (bounds.x + bounds.width, bounds.y),
        (bounds.x, bounds.y + bounds.height),
        (bounds.x + bounds.width, bounds.y + bounds.height),
    ]
    .map(|(x, y)| Point {
        x: pivot.x + cos * (x - pivot.x) - sin * (y - pivot.y),
        y: pivot.y + sin * (x - pivot.x) + cos * (y - pivot.y),
    });
    let x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let right = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let bottom = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

#[cfg(test)]
#[path = "turn_clock_tests.rs"]
mod tests;
