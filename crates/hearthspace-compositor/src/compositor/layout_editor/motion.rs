//! Frame placement and reveal motion retain the current visual pose when interrupted.
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use hearthspace_common::{
    canvas_search::motion::{ease, interpolate},
    frame_layout::Rect,
};

use super::App;

pub(super) const WIDGET_DURATION_MS: u64 = 280;
pub(super) const PLACEMENT_DURATION_MS: u64 = 300;
pub(super) const REVEAL_DURATION_MS: u64 = 360;

pub(super) struct State {
    poses: BTreeMap<u64, Pose>,
    pub sampled_at: Instant,
}

impl Default for State {
    fn default() -> Self {
        Self {
            poses: BTreeMap::new(),
            sampled_at: Instant::now(),
        }
    }
}

struct Pose {
    from: Rect,
    to: Rect,
    started: Instant,
    duration: Duration,
}

impl Pose {
    fn current(&self, now: Instant) -> Rect {
        let progress =
            now.saturating_duration_since(self.started).as_secs_f64() / self.duration.as_secs_f64();
        interpolate(self.from, self.to, ease(progress))
    }

    fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= self.duration
    }
}

impl State {
    pub(super) fn remove(&mut self, members: &[u64]) {
        self.poses.retain(|id, _| !members.contains(id));
    }

    fn queue(
        &mut self,
        id: u64,
        from: Rect,
        to: Rect,
        duration: Duration,
        reduced: bool,
        now: Instant,
    ) {
        if reduced || duration.is_zero() {
            self.poses.remove(&id);
            return;
        }
        if self.poses.get(&id).is_some_and(|pose| pose.to == to) {
            return;
        }
        let from = self.poses.get(&id).map_or(from, |pose| pose.current(now));
        if from == to {
            self.poses.remove(&id);
            return;
        }
        self.poses.insert(
            id,
            Pose {
                from,
                to,
                started: now,
                duration,
            },
        );
    }
}

impl App {
    pub(in crate::compositor) fn layout_animating(&self) -> bool {
        if self.layout_reduced_motion() {
            return false;
        }
        self.layout_editor
            .motion
            .poses
            .values()
            .any(|pose| !pose.finished(Instant::now()))
            || self.layout_turn_animating()
            || self.layout_guides_animating()
    }
    pub(in crate::compositor) fn queue_layout_motion(
        &mut self,
        window_id: u64,
        from: Rect,
        to: Rect,
        duration_ms: u64,
    ) {
        let reduced = self.layout_reduced_motion();
        self.layout_editor.motion.queue(
            window_id,
            from,
            to,
            Duration::from_millis(duration_ms),
            reduced,
            Instant::now(),
        );
        self.request_redraw();
    }

    pub(in crate::compositor) fn layout_motion_rect(&self, index: usize) -> Option<Rect> {
        if self.layout_reduced_motion() {
            return None;
        }
        let id = self.windows[index].id;
        // Every surface, outline and glass mask in a frame samples one pose.
        let now = self.layout_editor.motion.sampled_at;
        let turn = self
            .layout_editor
            .turn
            .turns
            .values()
            .find(|turn| turn.members.contains(&id));
        if let Some(turn) = turn
            && let Some(target) = self.layout_target_rect(index)
        {
            let offset = turn.offset(now);
            return Some(Rect {
                x: target.x + offset.x,
                y: target.y + offset.y,
                ..target
            });
        }
        self.layout_editor
            .motion
            .poses
            .get(&id)
            .map(|pose| pose.current(now))
    }

    pub(in crate::compositor) fn advance_layout_motion(&mut self) {
        let now = Instant::now();
        self.layout_editor.motion.sampled_at = now;
        let reduced = self.layout_reduced_motion();
        let was_active = !self.layout_editor.motion.poses.is_empty();
        self.layout_editor.motion.poses.retain(|id, pose| {
            !reduced
                && !pose.finished(now)
                && self
                    .windows
                    .iter()
                    .any(|window| window.id == *id && window.mapped)
        });
        if was_active {
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn clear_layout_motion(&mut self) {
        if !self.layout_editor.motion.poses.is_empty() {
            self.layout_editor.motion.poses.clear();
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn layout_reduced_motion(&self) -> bool {
        self.canvas_preview.reduced_motion
            || std::env::var_os("HEARTHSPACE_REDUCED_MOTION").is_some_and(|value| value == "1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupting_a_reveal_restarts_at_the_live_pose_without_a_jump() {
        let now = Instant::now();
        let mut state = State::default();
        let hidden = Rect {
            x: 1238.0,
            y: 160.0,
            width: 640.0,
            height: 480.0,
        };
        let open = Rect { x: 640.0, ..hidden };
        let duration = Duration::from_millis(REVEAL_DURATION_MS);
        state.queue(1, hidden, open, duration, false, now);
        let halfway = now + duration / 2;
        let visible = state.poses[&1].current(halfway);
        assert!(visible.x > open.x && visible.x < hidden.x);
        state.queue(1, open, hidden, duration, false, halfway);
        assert_eq!(state.poses[&1].current(halfway), visible);
        assert_eq!(state.poses[&1].current(halfway + duration), hidden);
    }

    #[test]
    fn repeated_layout_measurement_does_not_restart_motion_and_reduced_motion_clears_it() {
        let now = Instant::now();
        let mut state = State::default();
        let from = Rect {
            x: 30.0,
            y: 30.0,
            width: 110.0,
            height: 54.0,
        };
        let to = Rect {
            x: 400.0,
            y: 300.0,
            ..from
        };
        let duration = Duration::from_millis(WIDGET_DURATION_MS);
        state.queue(1, from, to, duration, false, now);
        state.queue(
            1,
            from,
            to,
            duration,
            false,
            now + Duration::from_millis(200),
        );
        assert!(state.poses[&1].finished(now + duration));
        state.queue(1, from, to, duration, true, now);
        assert!(state.poses.is_empty());
    }
}
