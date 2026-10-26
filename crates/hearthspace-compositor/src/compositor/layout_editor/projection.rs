//! Attachment projection changes visible placement without rewriting a window's canvas home.
use super::{App, Rect};
use hearthspace_common::frame_layout;
use smithay::utils::{Logical, Point};
impl App {
    pub(in crate::compositor) fn layout_surface_mapped(&mut self, index: usize) {
        let id = super::window_app_id(&self.windows[index].surface).unwrap_or_default();
        if frame_layout::layout_editor_from_app_id(&id).is_some()
            || frame_layout::layout_confirm_from_app_id(&id).is_some()
        {
            if self.layout_surface_visible(index) {
                if let Some(surface) = self.windows[index].surface.wl_surface() {
                    self.set_keyboard_focus_to_window(index, surface);
                }
            } else {
                self.windows[index].surface.send_close();
            }
        }
        self.reconcile_layout_editor();
    }
    pub(in crate::compositor) fn layout_focus_changed(&mut self, index: Option<usize>) {
        if let Some(index) = index {
            self.reveal_layout_member(index, None);
        }
        if self.layout_editing() || self.modal_confirmation_active() {
            return;
        }
        let focused = index.map(|i| self.windows[i].id);
        if self.layout_editor.expanded_window.is_some()
            && self.layout_editor.expanded_window != focused
        {
            let id = self.layout_editor.expanded_window.take().unwrap();
            if let Some(index) = self.windows.iter().position(|w| w.id == id)
                && let Some(to) = self.layout_target_rect(index)
            {
                let from = self.layout_motion_rect(index).unwrap_or_else(|| {
                    let view = self
                        .frame_output_rect(&self.pinned_window_target(id).unwrap().target.output)
                        .unwrap_or_default();
                    Rect {
                        x: to
                            .x
                            .clamp(view.x, (view.x + view.width - to.width).max(view.x)),
                        y: to
                            .y
                            .clamp(view.y, (view.y + view.height - to.height).max(view.y)),
                        ..to
                    }
                });
                self.queue_layout_motion(id, from, to, super::motion::REVEAL_DURATION_MS);
            }
            self.layout_changed();
        }
    }
    pub(in crate::compositor) fn frame_popup_origin(
        &self,
        index: usize,
    ) -> Option<Point<i32, Logical>> {
        let id = super::window_app_id(&self.windows[index].surface)?;
        let target = self.layout_editor.popup_anchors.get(&id)?;
        let placed = self.frame_placed(target)?;
        let view = self.frame_output_rect(&target.output)?;
        let size = self.window_geometry_size(index);
        let width = f64::from(size.w);
        let height = f64::from(size.h);
        let gap = 12.;
        let (mut x, mut y) = match placed.placement.edge() {
            Some(frame_layout::FrameEdge::Top) => {
                (placed.rect.x, placed.rect.y + placed.rect.height + gap)
            }
            Some(frame_layout::FrameEdge::Bottom) => (placed.rect.x, placed.rect.y - height - gap),
            Some(frame_layout::FrameEdge::Left) => {
                (placed.rect.x + placed.rect.width + gap, placed.rect.y)
            }
            Some(frame_layout::FrameEdge::Right) => (placed.rect.x - width - gap, placed.rect.y),
            None => (placed.rect.x, placed.rect.y + placed.rect.height + gap),
        };
        x = x.clamp(
            view.x + 12.,
            (view.x + view.width - width - 12.).max(view.x + 12.),
        );
        y = y.clamp(
            view.y + 12.,
            (view.y + view.height - height - 12.).max(view.y + 12.),
        );
        Some(
            Point::from((x.round() as i32, y.round() as i32))
                - self.window_surface_geometry_loc(index),
        )
    }
    pub(in crate::compositor) fn frame_window_attached(&self, index: usize) -> bool {
        self.pinned_window_target(self.windows[index].id).is_some()
    }
}

impl App {
    pub(in crate::compositor) fn layout_window_clip(
        &self,
        index: usize,
    ) -> Option<smithay::utils::Rectangle<i32, Logical>> {
        if let Some(clip) = self.layout_group_clip(index) {
            return Some(clip);
        }
        let placed = self.pinned_window_target(self.windows[index].id)?;
        if placed.group.is_none()
            || self.layout_editor.expanded_window == Some(self.windows[index].id)
            || self.canvas_preview_active()
        {
            return None;
        }
        let view = self.frame_output_rect(&placed.target.output)?;
        let full = self
            .layout_motion_rect(index)
            .or_else(|| self.layout_target_rect(index))?;
        let rect = if placed.vertical {
            Rect {
                x: full.x.min(view.x),
                y: placed.rect.y,
                width: (full.x + full.width).max(view.x + view.width) - full.x.min(view.x),
                height: placed.rect.height,
            }
        } else {
            Rect {
                x: placed.rect.x,
                y: full.y.min(view.y),
                width: placed.rect.width,
                height: (full.y + full.height).max(view.y + view.height) - full.y.min(view.y),
            }
        };
        Some(smithay::utils::Rectangle::new(
            (rect.x.floor() as i32, rect.y.floor() as i32).into(),
            (rect.width.ceil() as i32, rect.height.ceil() as i32).into(),
        ))
    }
}
