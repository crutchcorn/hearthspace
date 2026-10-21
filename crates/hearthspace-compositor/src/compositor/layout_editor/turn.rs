//! Toolbar orientation changes rotate the composed native control around its grabbed point.
use super::{App, Target, render_turn::TurnCache};
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
}

pub(super) struct Turn {
    pub target: Target,
    pub members: Vec<u64>,
    pub bounds: Rect,
    pub pivot: Point,
    pub from_angle: f64,
    pub started: Instant,
}

impl Turn {
    pub fn angle(&self, now: Instant) -> f64 {
        self.from_angle
            * (1.0
                - ease(
                    now.saturating_duration_since(self.started).as_secs_f64()
                        / TURN_DURATION.as_secs_f64(),
                ))
    }
}

impl App {
    pub(super) fn layout_turn_animating(&self) -> bool {
        self.layout_editor
            .turn
            .turns
            .values()
            .any(|turn| turn.started.elapsed() < TURN_DURATION)
    }
    pub(in crate::compositor) fn queue_layout_turn(
        &mut self,
        target: Target,
        members: Vec<u64>,
        bounds: Rect,
        pivot: Point,
        from_angle: f64,
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
        self.layout_editor.turn.turns.insert(
            key,
            Turn {
                target,
                members,
                bounds,
                pivot,
                from_angle: angle,
                started: now,
            },
        );
        self.request_redraw();
    }

    pub(in crate::compositor) fn advance_layout_turn(&mut self) {
        // Native orientation can commit a later reflow. Preserve the grabbed fraction and
        // the original clock; a new buffer must not restart a nearly completed turn.
        let updates: Vec<_> = self
            .layout_editor
            .turn
            .turns
            .iter()
            .filter_map(|(key, turn)| {
                let bounds = self.frame_placed(&turn.target)?.rect;
                (bounds != turn.bounds).then(|| {
                    let x = (turn.pivot.x - turn.bounds.x) / turn.bounds.width.max(1.);
                    let y = (turn.pivot.y - turn.bounds.y) / turn.bounds.height.max(1.);
                    (
                        key.clone(),
                        bounds,
                        Point {
                            x: bounds.x + bounds.width * x,
                            y: bounds.y + bounds.height * y,
                        },
                    )
                })
            })
            .collect();
        for (key, bounds, pivot) in updates {
            if let Some(turn) = self.layout_editor.turn.turns.get_mut(&key) {
                turn.bounds = bounds;
                turn.pivot = pivot;
            }
        }
        let active = !self.layout_editor.turn.turns.is_empty();
        let reduced = self.layout_reduced_motion();
        self.layout_editor.turn.turns.retain(|_, turn| {
            !reduced
                && turn.started.elapsed() < TURN_DURATION
                && turn.members.iter().any(|id| {
                    self.windows
                        .iter()
                        .any(|window| window.id == *id && window.mapped)
                })
        });
        self.layout_editor
            .turn
            .cache
            .retain(self.layout_editor.turn.turns.keys());
        if active {
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn clear_layout_turn(&mut self) {
        self.layout_editor.turn.turns.clear();
        self.layout_editor.turn.cache = Default::default();
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
