//! Stable native surface identities and visibility are distinct from canvas placement.
use super::{App, LayoutTarget, Target, TargetKind, window_app_id};
use hearthspace_common::frame_layout::*;
use smithay::utils::{Logical, Point};

impl App {
    fn resolved_frame_output(&self, output: Option<String>) -> Option<String> {
        output.or_else(|| self.capture_outputs().first().map(|o| o.name.clone()))
    }
    pub(in crate::compositor) fn frame_widget_identity(
        &self,
        index: usize,
    ) -> Option<(String, String)> {
        let (output, id) = frame_widget_from_app_id(&window_app_id(&self.windows[index].surface)?)?;
        Some((self.resolved_frame_output(output)?, id))
    }
    pub(in crate::compositor) fn frame_group_identity(
        &self,
        index: usize,
    ) -> Option<(String, String)> {
        let (output, id) = frame_group_from_app_id(&window_app_id(&self.windows[index].surface)?)?;
        Some((self.resolved_frame_output(output)?, id))
    }
    pub(super) fn frame_grip_identity(&self, index: usize) -> Option<(String, String)> {
        let (output, id) =
            frame_group_grip_from_app_id(&window_app_id(&self.windows[index].surface)?)?;
        Some((self.resolved_frame_output(output)?, id))
    }
    pub(in crate::compositor) fn frame_target_for_window(&self, index: usize) -> Option<Target> {
        if let Some((output, id)) = self.frame_widget_identity(index) {
            return Some(Target {
                output,
                target: LayoutTarget::widget(id),
            });
        }
        if let Some((output, id)) = self
            .frame_group_identity(index)
            .or_else(|| self.frame_grip_identity(index))
        {
            return Some(Target {
                output,
                target: LayoutTarget::new(TargetKind::Group, id),
            });
        }
        let target = if let Some(id) = self.desktop_id(index) {
            LayoutTarget::new(TargetKind::Item, id)
        } else if self.is_navigable_window(index) {
            LayoutTarget::new(TargetKind::Window, self.windows[index].id.to_string())
        } else {
            return None;
        };
        self.layout_editor
            .placed
            .iter()
            .find(|p| p.target.target == target)
            .map(|p| p.target.clone())
    }
    pub(in crate::compositor) fn is_layout_surface(&self, index: usize) -> bool {
        window_app_id(&self.windows[index].surface).is_some_and(|id| {
            frame_widget_from_app_id(&id).is_some()
                || frame_group_from_app_id(&id).is_some()
                || frame_group_grip_from_app_id(&id).is_some()
                || layout_editor_from_app_id(&id).is_some()
                || layout_tooltip_from_app_id(&id).is_some()
                || pinned_window_reveal_from_app_id(&id).is_some()
                || layout_confirm_from_app_id(&id).is_some()
        })
    }
    pub(in crate::compositor) fn layout_surface_visible(&self, index: usize) -> bool {
        if self.session_locked() || self.canvas_preview_active() {
            return false;
        }
        let Some(id) = window_app_id(&self.windows[index].surface) else {
            return false;
        };
        if layout_tooltip_from_app_id(&id).is_some() {
            return self.layout_tooltip_rect(index).is_some();
        }
        if layout_confirm_from_app_id(&id).is_some() {
            return self.layout_confirmation_surface(index);
        }
        if let Some((session, output)) = layout_editor_from_app_id(&id) {
            return self.layout_editor.session.as_ref().is_some_and(|s| {
                s.editing
                    && s.id == session
                    && self.resolved_frame_output(output).as_deref() == Some(s.output.as_str())
            });
        }
        if let Some(id) = pinned_window_reveal_from_app_id(&id) {
            return !self.layout_editing()
                && self.layout_editor.expanded_window != Some(id)
                && self.pinned_window_target(id).is_some();
        }
        if self.frame_grip_identity(index).is_some() && !self.layout_editing() {
            return false;
        }
        self.frame_target_for_window(index)
            .and_then(|target| self.frame_placed(&target))
            .is_some_and(|p| p.placement != ObjectPlacement::Hidden)
    }
    pub(in crate::compositor) fn layout_surface_origin(
        &self,
        index: usize,
    ) -> Option<Point<i32, Logical>> {
        let rect = self
            .layout_motion_rect(index)
            .or_else(|| self.layout_target_rect(index))?;
        let inset = self.window_surface_geometry_loc(index);
        Some(Point::from((rect.x.round() as i32, rect.y.round() as i32)) - inset)
    }
}

impl App {
    pub(in crate::compositor) fn layout_render_scale(&self, index: usize) -> Option<f64> {
        let target = self.frame_target_for_window(index)?;
        let placement = self.target_placement(&target);
        Some(if placement.is_frame() {
            1.
        } else {
            self.viewport_scale
        })
    }
    pub(in crate::compositor) fn layout_render_layer(&self, index: usize) -> Option<i32> {
        if self.layout_drag_contains(index) {
            // The backing is a separate native window, often mapped after its
            // controls. Keep it below them when raising the whole dragged group.
            return Some(if self.frame_group_identity(index).is_some() {
                114
            } else {
                115
            });
        }
        let id = window_app_id(&self.windows[index].surface).unwrap_or_default();
        if layout_confirm_from_app_id(&id).is_some() {
            return Some(140);
        }
        if layout_tooltip_from_app_id(&id).is_some() {
            return Some(121);
        }
        if layout_editor_from_app_id(&id).is_some() {
            return Some(120);
        }
        if pinned_window_reveal_from_app_id(&id).is_some() {
            return Some(105);
        }
        let target = self.frame_target_for_window(index)?;
        let layer = match self.target_placement(&target) {
            ObjectPlacement::Background { .. } => {
                if target.target.kind == TargetKind::Window {
                    1
                } else {
                    0
                }
            }
            ObjectPlacement::Canvas { .. } => match target.target.kind {
                TargetKind::Item => 0,
                TargetKind::Widget | TargetKind::Group => 5,
                TargetKind::Window => 10,
            },
            ObjectPlacement::Frame { .. } => 100,
            ObjectPlacement::Hidden => -1,
        };
        Some(
            if target.target.kind == TargetKind::Group && self.frame_grip_identity(index).is_none()
            {
                layer - 1
            } else {
                layer
            },
        )
    }
}
