//! Scroll long groups within the output while retaining every member's native surface.
use super::{App, LayoutTarget, PlacedTarget, Point, Rect, Target, TargetKind};
use smithay::utils::{Logical, Rectangle};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct State {
    offsets: HashMap<(String, String), f64>,
}

fn inside(rect: Rect, point: Point) -> bool {
    point.x >= rect.x
        && point.y >= rect.y
        && point.x < rect.x + rect.width
        && point.y < rect.y + rect.height
}

impl App {
    fn group_inset(&self, target: &Target) -> f64 {
        if target.target.id == "system" && self.compact_frame_bars(&target.output).0 {
            0.
        } else if self.target_placement(target).is_frame() {
            5.
        } else {
            5. * self.viewport_scale
        }
    }

    pub(super) fn frame_group_clip_rect(&self, group: &PlacedTarget) -> Rect {
        let inset = self.group_inset(&group.target);
        Rect {
            x: group.rect.x + inset,
            y: group.rect.y + inset,
            width: (group.rect.width - inset * 2.).max(1.),
            height: (group.rect.height - inset * 2.).max(1.),
        }
    }

    pub(super) fn project_frame_scroll(&self, placed: &mut [PlacedTarget]) {
        let groups: Vec<_> = placed
            .iter()
            .filter(|p| {
                p.target.target.kind == TargetKind::Group
                    && !super::compact::backing(&p.target.target.id)
            })
            .cloned()
            .collect();
        for group in groups {
            let clip = self.frame_group_clip_rect(&group);
            let extent = member_extent(placed, &group);
            let visible = if group.vertical {
                clip.height
            } else {
                clip.width
            };
            let maximum = (extent - visible).max(0.);
            let offset = self
                .layout_editor
                .overflow
                .offsets
                .get(&(group.target.output.clone(), group.target.target.id.clone()))
                .copied()
                .unwrap_or(0.)
                .clamp(0., maximum);
            for member in placed.iter_mut().filter(|p| {
                p.target.output == group.target.output
                    && p.group.as_deref() == Some(&group.target.target.id)
            }) {
                if group.vertical {
                    member.rect.y -= offset;
                } else {
                    member.rect.x -= offset;
                }
            }
        }
    }

    fn frame_scroll_maximum(&self, group: &PlacedTarget) -> f64 {
        let clip = self.frame_group_clip_rect(group);
        (member_extent(&self.layout_editor.placed, group)
            - if group.vertical {
                clip.height
            } else {
                clip.width
            })
        .max(0.)
    }

    pub(in crate::compositor) fn layout_scroll_group_at(&self, point: Point) -> Option<Target> {
        if self.session_locked()
            || self.canvas_preview_active()
            || self.modal_confirmation_active()
            || self.layout_editor.drag.is_some()
        {
            return None;
        }
        let hit = self.hit_test((point.x, point.y).into())?;
        let index = match hit {
            crate::compositor::HitTarget::Client { window_index, .. }
            | crate::compositor::HitTarget::TitleBar { window_index }
            | crate::compositor::HitTarget::TitlebarButton { window_index, .. }
            | crate::compositor::HitTarget::ResizeBorder { window_index, .. } => window_index,
        };
        let target = self.frame_group_for_window(index)?;
        let group = self.frame_placed(&target)?;
        (inside(group.rect, point) && self.frame_scroll_maximum(group) > 0.).then_some(target)
    }

    pub(in crate::compositor) fn scroll_layout_group(
        &mut self,
        target: &Target,
        delta: f64,
    ) -> bool {
        let Some(group) = self.frame_placed(target) else {
            return false;
        };
        let maximum = self.frame_scroll_maximum(group);
        if maximum <= 0. || !delta.is_finite() {
            return false;
        }
        let key = (target.output.clone(), target.target.id.clone());
        let current = self
            .layout_editor
            .overflow
            .offsets
            .get(&key)
            .copied()
            .unwrap_or(0.)
            .clamp(0., maximum);
        let next = (current + delta).clamp(0., maximum);
        if next != current {
            self.layout_editor.overflow.offsets.insert(key, next);
            self.layout_editor.placed = self.frame_placements();
            self.layout_changed();
        }
        true
    }

    pub(in crate::compositor) fn layout_group_axis(
        &mut self,
        horizontal: f64,
        vertical: f64,
    ) -> bool {
        if self.keyboard.modifier_state().logo {
            return false;
        }
        let Some(target) = self.layout_scroll_group_at(Point {
            x: self.pointer_location.x,
            y: self.pointer_location.y,
        }) else {
            return false;
        };
        let vertical_group = self.frame_placed(&target).is_some_and(|p| p.vertical);
        let delta = if vertical_group || horizontal.abs() <= vertical.abs() {
            vertical
        } else {
            horizontal
        };
        self.scroll_layout_group(&target, delta)
    }

    pub(in crate::compositor) fn scroll_layout_group_motion(
        &mut self,
        target: &Target,
        dx: f64,
        dy: f64,
    ) -> bool {
        let Some(group) = self.frame_placed(target) else {
            return false;
        };
        let delta = if group.vertical { -dy } else { -dx };
        self.scroll_layout_group(target, delta)
    }

    pub(super) fn frame_group_for_window(&self, index: usize) -> Option<Target> {
        let target = self.frame_target_for_window(index).or_else(|| {
            let id = super::window_app_id(&self.windows[index].surface)?;
            let id = hearthspace_common::frame_layout::pinned_window_reveal_from_app_id(&id)?;
            Some(self.pinned_window_target(id)?.target.clone())
        })?;
        if target.target.kind == TargetKind::Group {
            return Some(target);
        }
        let group = self.frame_placed(&target)?.group.as_ref()?;
        Some(Target {
            output: target.output,
            target: LayoutTarget::new(TargetKind::Group, group),
        })
    }

    pub(in crate::compositor) fn layout_group_clip(
        &self,
        index: usize,
    ) -> Option<Rectangle<i32, Logical>> {
        let target = self.frame_group_for_window(index)?;
        if self.frame_group_identity(index).is_some()
            || self.frame_grip_identity(index).is_some()
            || self.canvas_preview_active()
            || self.layout_drag_contains(index)
        {
            return None;
        }
        let group = self.frame_placed(&target)?;
        if self.frame_scroll_maximum(group) <= 0. {
            return None;
        }
        let mut clip = self.frame_group_clip_rect(group);
        if self.pinned_window_target(self.windows[index].id).is_some() {
            if self.layout_editor.expanded_window == Some(self.windows[index].id) {
                return None;
            }
            let view = self.frame_output_rect(&target.output)?;
            if group.vertical {
                clip.x = view.x;
                clip.width = view.width;
            } else {
                clip.y = view.y;
                clip.height = view.height;
            }
        }
        Some(Rectangle::new(
            (clip.x.round() as i32, clip.y.round() as i32).into(),
            (
                clip.width.round().max(1.) as i32,
                clip.height.round().max(1.) as i32,
            )
                .into(),
        ))
    }

    pub(super) fn reveal_layout_member(&mut self, index: usize, local: Option<Rect>) {
        let Some(target) = self.frame_group_for_window(index) else {
            return;
        };
        let Some(group) = self.frame_placed(&target) else {
            return;
        };
        let clip = self.frame_group_clip_rect(group);
        let vertical = group.vertical;
        let Some(mut rect) = self.layout_target_rect(index) else {
            return;
        };
        if let Some(local) = local {
            rect.x += local.x;
            rect.y += local.y;
            rect.width = local.width;
            rect.height = local.height;
        }
        let (start, length, low, visible) = if vertical {
            (rect.y, rect.height, clip.y, clip.height)
        } else {
            (rect.x, rect.width, clip.x, clip.width)
        };
        let delta = if start < low {
            start - low
        } else if start + length > low + visible {
            (start + length.min(visible)) - (low + visible)
        } else {
            0.
        };
        self.scroll_layout_group(&target, delta);
    }
}

fn member_extent(placed: &[PlacedTarget], group: &PlacedTarget) -> f64 {
    let mut start = f64::INFINITY;
    let mut end = f64::NEG_INFINITY;
    for member in placed.iter().filter(|p| {
        p.target.output == group.target.output
            && p.group.as_deref() == Some(&group.target.target.id)
    }) {
        let (position, length) = if group.vertical {
            (member.rect.y, member.rect.height)
        } else {
            (member.rect.x, member.rect.width)
        };
        start = start.min(position);
        end = end.max(position + length);
    }
    (end - start).max(0.)
}
