use super::{App, Rect};
use hearthspace_common::canvas_search::motion::{
    ease, interpolate, opacity_ease, reveal_ease, visible_travel,
};
use std::time::{Duration, Instant};
const DURATION: Duration = Duration::from_millis(360);

pub(super) struct Pose {
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
impl App {
    pub(super) fn canvas_search_motion_start(&self) -> Instant {
        let now = Instant::now();
        if self.canvas_search.reduced_motion
            || std::env::var_os("HEARTHSPACE_REDUCED_MOTION").is_some_and(|value| value == "1")
        {
            now - DURATION
        } else {
            now
        }
    }
    pub(in crate::compositor) fn canvas_search_pose(&self, index: usize) -> Option<(Rect, f64)> {
        self.canvas_search
            .poses
            .get(&self.windows[index].id)
            .map(|pose| pose.current(Instant::now()))
    }
    pub(in crate::compositor) fn canvas_search_animating(&self) -> bool {
        self.canvas_search
            .poses
            .values()
            .any(|p| p.started.elapsed() < DURATION)
    }
    pub(in crate::compositor) fn advance_canvas_search(&mut self) {
        if let Some(session) = self.canvas_search.session.as_ref() {
            let output = self
                .output_logical_rect_for_name(session.output.as_deref())
                .map(super::scene::rect);
            match output {
                None => self.close_canvas_search(true),
                Some(view) if view != session.view => {
                    let s = self.canvas_search.session.as_mut().unwrap();
                    s.anchor.x =
                        (s.anchor.x - s.view.x + view.x).clamp(view.x, view.x + view.width);
                    s.anchor.y =
                        (s.anchor.y - s.view.y + view.y).clamp(view.y, view.y + view.height);
                    s.view = view;
                    s.signature = None;
                }
                _ => {}
            }
        }
        if self.canvas_search_active() {
            self.rebuild_canvas_search();
            self.refresh_canvas_search_hover();
        }
        if self.canvas_search_animating() {
            self.request_redraw();
        } else if !self.canvas_search_active() && !self.canvas_search.poses.is_empty() {
            self.canvas_search.poses.clear();
            self.request_redraw();
        }
    }
    pub(super) fn retarget_canvas_search_exit(&mut self, view: Option<Rect>, reveal: bool) {
        let now = Instant::now();
        let started = self.canvas_search_motion_start();
        let targets: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, w)| {
                self.canvas_search.poses.get(&w.id).map(|p| {
                    (
                        w.id,
                        super::scene::rect(self.base_window_screen_bounds(index)),
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
            self.canvas_search.poses.insert(
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
