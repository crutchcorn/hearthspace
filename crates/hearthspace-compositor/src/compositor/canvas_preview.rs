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

#[derive(Default)]
pub(super) struct State {
    pub owner: Option<Owner>,
    pub poses: BTreeMap<u64, Pose>,
    pub reduced_motion: bool,
    pub selection: super::selection::Cache,
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
impl App {
    pub(in crate::compositor) fn canvas_preview_active(&self) -> bool {
        self.canvas_search_active() || self.window_switcher_active()
    }
    pub(in crate::compositor) fn canvas_preview_presenting(&self) -> bool {
        self.canvas_preview_active() || !self.canvas_preview.poses.is_empty()
    }
    pub(in crate::compositor) fn advance_canvas_preview(&mut self) {
        if self.canvas_preview_animating() {
            self.request_redraw();
        } else if !self.canvas_preview_active() && !self.canvas_preview.poses.is_empty() {
            self.canvas_preview.poses.clear();
            self.canvas_preview.owner = None;
            self.request_redraw();
        }
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
        self.canvas_preview
            .poses
            .get(&self.windows[index].id)
            .map(|pose| pose.current(Instant::now()))
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
