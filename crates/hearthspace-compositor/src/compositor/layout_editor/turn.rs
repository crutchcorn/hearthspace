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
        let active = !self.layout_editor.turn.turns.is_empty();
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
        self.layout_editor
            .turn
            .turns
            .retain(|key, turn| !reduced && !turn.finished(now) && valid.contains(key));
        if active {
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
mod tests {
    use super::*;
    #[test]
    fn reflow_wait_does_not_consume_motion_time_and_translation_shares_the_turn_clock() {
        let queued = Instant::now();
        let mut turn = Turn {
            target: Target {
                output: "test".into(),
                target: hearthspace_common::frame_layout::LayoutTarget::widget("tools"),
            },
            members: vec![1],
            bounds: Rect {
                x: 24.,
                y: 200.,
                width: 54.,
                height: 246.,
            },
            pivot: Point { x: -18., y: 323. },
            from_angle: -std::f64::consts::FRAC_PI_2,
            translation: Point { x: 480., y: -160. },
            pointer_anchor: false,
            started: None,
        };
        let committed = queued + Duration::from_millis(600);
        assert_eq!(turn.angle(committed), turn.from_angle);
        assert_eq!(turn.offset(committed), turn.translation);
        assert!(!turn.finished(committed));
        turn.started = Some(committed);
        for elapsed in [0, 16, 50, 100, 180, 280] {
            let now = committed + Duration::from_millis(elapsed);
            let fraction = turn.angle(now) / turn.from_angle;
            assert!((turn.offset(now).x - turn.translation.x * fraction).abs() < 1e-8);
            assert!((turn.offset(now).y - turn.translation.y * fraction).abs() < 1e-8);
            assert_eq!(turn.bounds.width, 54.);
            assert_eq!(turn.bounds.height, 246.);
        }
        assert!(turn.finished(committed + TURN_DURATION));
        assert_eq!(turn.angle(committed + TURN_DURATION), 0.);
        assert_eq!(
            turn.offset(committed + TURN_DURATION),
            Point { x: 0., y: 0. }
        );
    }

    #[test]
    fn quarter_turn_preserves_the_pointer_grip_and_swaps_toolbar_extent() {
        let bounds = Rect {
            x: 100.,
            y: 100.,
            width: 54.,
            height: 200.,
        };
        let pivot = Point { x: 127., y: 127. };
        let rotated = rotated_bounds(bounds, pivot, -std::f64::consts::FRAC_PI_2);
        assert!((rotated.width - 200.).abs() < 0.0001);
        assert!((rotated.height - 54.).abs() < 0.0001);
        assert!((rotated.x - 100.).abs() < 0.0001);
        assert!((rotated.y - 100.).abs() < 0.0001);
        assert_eq!(rotated_bounds(bounds, pivot, 0.), bounds);
    }
}
