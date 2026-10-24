use super::App;
use hearthspace_common::canvas_search::Rect;
use hearthspace_common::canvas_search::motion::{
    ease, interpolate, opacity_ease, reveal_ease, visible_travel,
};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
const DURATION: Duration = Duration::from_millis(360);

pub(in crate::compositor) struct Pose {
    pub from: Rect,
    pub to: Rect,
    pub from_opacity: f64,
    pub to_opacity: f64,
    pub started: Instant,
    pub natural: Rect,
    pub matches: bool,
    pub reveal: bool,
}
impl Pose {
    pub fn current(&self, now: Instant) -> (Rect, f64) {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f64();
        let progress = (elapsed / DURATION.as_secs_f64()).min(1.);
        let p = if self.reveal {
            reveal_ease(progress)
        } else {
            ease(progress)
        };
        let alpha = opacity_ease((elapsed / 0.22).min(1.));
        (
            interpolate(self.from, self.to, p),
            self.from_opacity + (self.to_opacity - self.from_opacity) * alpha,
        )
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Owner {
    Search,
    Switcher,
}

pub(super) struct State {
    pub owner: Option<Owner>,
    sampled_at: Instant,
    pub poses: BTreeMap<u64, Pose>,
    pub reduced_motion: bool,
    pub selection: super::selection::Cache,
}
impl Default for State {
    fn default() -> Self {
        Self {
            owner: None,
            sampled_at: Instant::now(),
            poses: BTreeMap::new(),
            reduced_motion: false,
            selection: Default::default(),
        }
    }
}
mod render;
pub(in crate::compositor) fn rect(
    value: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
) -> Rect {
    Rect {
        x: f64::from(value.loc.x),
        y: f64::from(value.loc.y),
        width: f64::from(value.size.w),
        height: f64::from(value.size.h),
    }
}
impl State {
    /// Input and rendering read the same fractional pose by stable window ID.
    pub(in crate::compositor) fn pose(&self, id: u64) -> Option<(Rect, f64)> {
        self.poses
            .get(&id)
            .map(|pose| pose.current(self.sampled_at))
    }

    fn needs_animation_frame(&self) -> bool {
        // Wall time may have passed the deadline since the last frame. Keep
        // scheduling until an advance samples the endpoint and requests redraw.
        self.poses
            .values()
            .any(|pose| self.sampled_at.saturating_duration_since(pose.started) < DURATION)
    }

    pub(in crate::compositor) fn advance(&mut self, now: Instant, active: bool) -> bool {
        let needs_redraw = self.needs_animation_frame();
        self.sampled_at = now;
        if !active && !self.needs_animation_frame() && !self.poses.is_empty() {
            self.poses.clear();
            self.owner = None;
            return true;
        }
        needs_redraw
    }
}
impl App {
    pub(in crate::compositor) fn canvas_preview_active(&self) -> bool {
        self.canvas_search_active() || self.window_switcher_active()
    }
    pub(in crate::compositor) fn canvas_preview_presenting(&self) -> bool {
        self.canvas_preview_active() || !self.canvas_preview.poses.is_empty()
    }
    pub(in crate::compositor) fn advance_canvas_preview(&mut self) {
        let active = self.canvas_preview_active();
        if self.canvas_preview.advance(Instant::now(), active) {
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn canvas_preview_needs_animation_frame(&self) -> bool {
        self.canvas_preview.needs_animation_frame()
            || (!self.canvas_preview_active() && !self.canvas_preview.poses.is_empty())
    }

    pub(in crate::compositor) fn canvas_preview_motion_start(&self) -> Instant {
        let now = Instant::now();
        if self.canvas_preview.reduced_motion
            || std::env::var_os("HEARTHSPACE_REDUCED_MOTION").is_some_and(|value| value == "1")
        {
            now - DURATION
        } else {
            now
        }
    }
    pub(in crate::compositor) fn canvas_preview_pose(&self, index: usize) -> Option<(Rect, f64)> {
        self.canvas_preview.pose(self.windows[index].id)
    }
    pub(in crate::compositor) fn canvas_preview_animating(&self) -> bool {
        self.canvas_preview
            .poses
            .values()
            .any(|p| p.started.elapsed() < DURATION)
    }
    pub(in crate::compositor) fn retarget_canvas_preview_exit(
        &mut self,
        view: Option<Rect>,
        reveal: bool,
    ) {
        let now = Instant::now();
        let started = self.canvas_preview_motion_start();
        let targets: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, w)| {
                self.canvas_preview.poses.get(&w.id).map(|p| {
                    (
                        w.id,
                        rect(self.base_window_screen_bounds(index)),
                        p.current(now),
                        p.natural,
                    )
                })
            })
            .collect();
        for (id, to, (from, alpha), natural) in targets {
            let (from, to) = if reveal {
                view.and_then(|view| visible_travel(from, to, view))
                    .unwrap_or((to, to))
            } else {
                (from, to)
            };
            self.canvas_preview.poses.insert(
                id,
                Pose {
                    from,
                    to,
                    from_opacity: alpha,
                    to_opacity: 1.,
                    started,
                    natural,
                    matches: true,
                    reveal,
                },
            );
        }
    }
}

#[cfg(test)]
mod sampling_tests;
#[cfg(test)]
pub(in crate::compositor) mod test_support;
