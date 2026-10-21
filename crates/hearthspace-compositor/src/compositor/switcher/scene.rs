use super::*;
use crate::compositor::{
    ManagedWindowKind,
    canvas_preview::{Pose, rect},
    windows::window_title,
};
use hearthspace_common::canvas_search::{park, window_switcher_layout};
use std::collections::{HashMap, HashSet};

impl App {
    pub(in crate::compositor) fn window_switcher_output(
        &self,
    ) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
        self.window_switcher
            .session
            .as_ref()
            .and_then(|s| self.output_logical_rect_for_name(s.output.as_deref()))
            .unwrap_or_else(|| self.primary_output_logical_rect())
    }
    fn switcher_targets(&self) -> HashMap<u64, Rect> {
        let Some(s) = self.window_switcher.session.as_ref() else {
            return HashMap::new();
        };
        let size_by_id: HashMap<_, _> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.is_navigable_window(*index))
            .map(|(index, window)| {
                let r = self.window_canvas_rect(index);
                (window.id, (f64::from(r.size.w), f64::from(r.size.h)))
            })
            .collect();
        let sizes: Vec<_> = s
            .ids
            .iter()
            .map(|id| size_by_id.get(id).copied().unwrap_or((1., 1.)))
            .collect();
        s.ids
            .iter()
            .zip(window_switcher_layout(s.view, &sizes))
            .zip(sizes)
            .map(|((id, p), (w, h))| {
                (
                    *id,
                    Rect {
                        x: p.x,
                        y: p.y,
                        width: w * p.scale,
                        height: h * p.scale,
                    },
                )
            })
            .collect()
    }
    pub(super) fn rebuild_window_switcher(&mut self) {
        let view = rect(self.window_switcher_output());
        let Some(s) = self.window_switcher.session.as_mut() else {
            return;
        };
        s.view = view;
        let selected = s.selected;
        let targets = self.switcher_targets();
        let now = self.canvas_preview_motion_start();
        let mut changed = false;
        for (i, w) in self.windows.iter().enumerate() {
            if !w.mapped
                || (w.kind != ManagedWindowKind::DesktopItem && !self.is_navigable_window(i))
            {
                continue;
            }
            let natural = rect(self.base_window_screen_bounds(i));
            let matches = targets.contains_key(&w.id);
            let target = targets.get(&w.id).copied().unwrap_or_else(|| {
                let p = park(natural, view);
                Rect {
                    x: p.x,
                    y: p.y,
                    ..natural
                }
            });
            let opacity = if matches { 1. } else { 0. };
            let previous = self.canvas_preview.poses.get(&w.id);
            if previous
                .is_some_and(|p| p.to == target && p.natural == natural && p.to_opacity == opacity)
            {
                continue;
            }
            let (from, from_opacity) = previous.map(|p| p.current(now)).unwrap_or((natural, 1.));
            self.canvas_preview.poses.insert(
                w.id,
                Pose {
                    from,
                    to: target,
                    from_opacity,
                    to_opacity: opacity,
                    started: now,
                    natural,
                    matches,
                    reveal: false,
                },
            );
            changed = true;
        }
        let live: HashSet<_> = self
            .windows
            .iter()
            .filter(|w| w.mapped)
            .map(|w| w.id)
            .collect();
        self.canvas_preview.poses.retain(|id, _| live.contains(id));
        let title = self
            .windows
            .iter()
            .find(|w| w.id == selected)
            .and_then(|w| window_title(&w.surface))
            .unwrap_or_else(|| "Untitled window".into())
            .chars()
            .take(4096)
            .collect::<String>();
        let preview = targets.get(&selected).copied().unwrap_or_default();
        let s = self.window_switcher.session.as_mut().unwrap();
        if s.caption_preview != preview || s.caption_title != title {
            s.caption_preview = preview;
            s.caption_title = title;
            s.caption_width = None;
            self.window_switcher_changed();
        } else if changed {
            self.request_redraw();
        }
    }
    pub(in crate::compositor) fn window_switcher_window_at(
        &self,
        location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) -> Option<u64> {
        let s = self.window_switcher.session.as_ref()?;
        s.ids.iter().find_map(|id| {
            let (r, alpha) = self
                .canvas_preview
                .poses
                .get(id)?
                .current(std::time::Instant::now());
            (alpha > 0.05
                && location.x >= r.x
                && location.x < r.x + r.width
                && location.y >= r.y
                && location.y < r.y + r.height)
                .then_some(*id)
        })
    }
}
