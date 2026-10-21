//! Pinned bodies retain client geometry while their exposed strip follows the frame.
use super::*;
impl App {
    pub(in crate::compositor) fn pinned_window_target(&self, id: u64) -> Option<&PlacedTarget> {
        self.layout_editor.placed.iter().find(|p| {
            p.target.target.kind == TargetKind::Window
                && p.target.target.id == id.to_string()
                && p.placement.is_frame()
        })
    }
    pub(in crate::compositor) fn layout_target_rect(&self, index: usize) -> Option<Rect> {
        if self.is_layout_tooltip(index) {
            return self.layout_tooltip_rect(index);
        }
        let id = super::super::window_app_id(&self.windows[index].surface).unwrap_or_default();
        if let Some((session, output)) = frame_layout::layout_confirm_from_app_id(&id) {
            let active = self
                .layout_editor
                .session
                .as_ref()
                .filter(|s| s.id == session && s.confirmation.is_some())?;
            let view = self.frame_output_rect(output.as_deref().unwrap_or(&active.output))?;
            let size = self.window_geometry_size(index);
            let width = f64::from(size.w).min((view.width - 48.).max(1.));
            let height = f64::from(size.h).min((view.height - 48.).max(1.));
            return Some(Rect {
                x: view.x + (view.width - width) / 2.,
                y: view.y + (view.height - height) / 2.,
                width,
                height,
            });
        }
        if let Some((session, output)) = frame_layout::layout_editor_from_app_id(&id) {
            let active = self
                .layout_editor
                .session
                .as_ref()
                .filter(|s| s.editing && s.id == session)?;
            let view = self.frame_output_rect(output.as_deref().unwrap_or(&active.output))?;
            let selected = active.selected.as_ref();
            let grouped = selected.is_some_and(|target| {
                self.layout_editor
                    .layouts
                    .get(&target.output)
                    .is_some_and(|layout| layout.group_for(&target.target).is_some())
            });
            let height =
                frame_layout::editor_height(selected.map(|target| &target.target), grouped);
            let width = (view.width - 24.).clamp(120., 260.);
            return Some(Rect {
                x: view.x + (view.width - width) / 2.,
                y: view.y + if view.width <= 760. { 78. } else { 90. },
                width,
                height: height
                    .min((view.height - if view.width <= 760. { 156. } else { 180. }).max(54.)),
            });
        }
        if let Some(id) = frame_layout::pinned_window_reveal_from_app_id(&id) {
            return self
                .pinned_window_target(id)
                .map(|p| self.pinned_reveal_rect(p));
        }
        let target = self.frame_target_for_window(index)?;
        let placed = self.frame_placed(&target)?;
        if target.target.kind == TargetKind::Window
            && placed.group.is_some()
            && !placed.placement.is_frame()
            && self.layout_drag_contains(index)
        {
            let size = self.window_canvas_rect(index).size;
            return Some(Rect {
                x: placed.rect.x,
                y: placed.rect.y,
                width: f64::from(size.w) * self.viewport_scale,
                height: f64::from(size.h) * self.viewport_scale,
            });
        }
        if matches!(target.target.kind, TargetKind::Window | TargetKind::Item)
            && !placed.placement.is_frame()
        {
            return None;
        }
        if self.frame_grip_identity(index).is_some() {
            return Some(grip_rect(placed.rect, placed.placement.edge()));
        }
        if target.target.kind == TargetKind::Window && placed.placement.is_frame() {
            return Some(self.pinned_full_rect(index, placed));
        }
        Some(placed.rect)
    }
    fn pinned_full_rect(&self, index: usize, placed: &PlacedTarget) -> Rect {
        let view = self
            .frame_output_rect(&placed.target.output)
            .unwrap_or_default();
        let size = self.window_canvas_rect(index).size;
        let mut rect = frame_layout::frame_bounds(
            placed.placement,
            f64::from(size.w),
            f64::from(size.h),
            view,
            false,
        );
        // The client owns its full buffer extent, even when the preview exceeds the output.
        // Only translate its body off the frame; never squeeze it into the visible strip.
        rect.width = f64::from(size.w);
        rect.height = f64::from(size.h);
        if placed.placement.edge() == Some(frame_layout::FrameEdge::Right) {
            rect.x = view.width - rect.width;
        }
        if placed.placement.edge() == Some(frame_layout::FrameEdge::Bottom) {
            rect.y = view.height - rect.height;
        }
        if placed.group.is_some() {
            if placed.vertical {
                rect.y = placed.rect.y - view.y;
            } else {
                rect.x = placed.rect.x - view.x;
            }
        }
        let expanded = self.layout_editor.expanded_window == Some(self.windows[index].id);
        let rest = if placed.group.is_some() {
            match placed.placement.edge().unwrap() {
                frame_layout::FrameEdge::Top => placed.rect.y + placed.rect.height - view.y,
                frame_layout::FrameEdge::Bottom => view.y + view.height - placed.rect.y,
                frame_layout::FrameEdge::Left => placed.rect.x + placed.rect.width - view.x,
                frame_layout::FrameEdge::Right => view.x + view.width - placed.rect.x,
            }
        } else {
            frame_layout::PINNED_EXPOSED_SIZE
        };
        let exposed = rest
            + if self.layout_editor.hovered_window == Some(self.windows[index].id)
                && !self.layout_editing()
            {
                frame_layout::PINNED_HOVER_SIZE - frame_layout::PINNED_EXPOSED_SIZE
            } else {
                0.
            };
        let offset = frame_layout::pinned_window_offset(
            rect,
            placed.placement.edge().unwrap(),
            view,
            expanded,
            exposed,
        );
        rect.x += view.x + offset.x;
        rect.y += view.y + offset.y;
        rect
    }
    pub(in crate::compositor::layout_editor) fn pinned_reveal_rect(
        &self,
        placed: &PlacedTarget,
    ) -> Rect {
        if placed.group.is_some() {
            return placed.rect;
        }
        let view = self
            .frame_output_rect(&placed.target.output)
            .unwrap_or_default();
        let mut rect = placed.rect;
        let exposed = if placed.target.target.id.parse::<u64>().ok()
            == self.layout_editor.hovered_window
            && !self.layout_editing()
        {
            100.
        } else {
            42.
        };
        match placed.placement.edge().unwrap() {
            frame_layout::FrameEdge::Top => {
                rect.y = view.y;
                rect.height = exposed;
            }
            frame_layout::FrameEdge::Bottom => {
                rect.y = view.y + view.height - exposed;
                rect.height = exposed;
            }
            frame_layout::FrameEdge::Left => {
                rect.x = view.x;
                rect.width = exposed;
            }
            frame_layout::FrameEdge::Right => {
                rect.x = view.x + view.width - exposed;
                rect.width = exposed;
            }
        }
        rect
    }
}
