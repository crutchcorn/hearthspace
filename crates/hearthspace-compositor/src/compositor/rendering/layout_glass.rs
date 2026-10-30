//! Shared bounds and corner masks for compositor-backed native layout glass.
use crate::compositor::{App, windows::window_app_id};
use hearthspace_common::frame_layout as layout;
use smithay::utils::{Logical, Rectangle, Scale};

impl App {
    /// An empty material backing tracks its group while the native resize commits.
    /// Client controls keep their own scale and are never stretched with it.
    pub(in crate::compositor) fn layout_backing_scale(&self, index: usize) -> Option<Scale<f64>> {
        self.frame_group_identity(index)?;
        let bounds = self
            .layout_motion_rect(index)
            .or_else(|| self.layout_target_rect(index))?;
        let native = self.window_geometry_size(index);
        Some(Scale {
            x: bounds.width / f64::from(native.w.max(1)),
            y: bounds.height / f64::from(native.h.max(1)),
        })
    }

    pub(super) fn layout_glass_rect(&self, index: usize) -> Option<Rectangle<i32, Logical>> {
        let id = window_app_id(&self.windows[index].surface)?;
        let glass = self.frame_group_has_material(index)
            || layout::frame_group_grip_from_app_id(&id).is_some()
            || layout::layout_editor_from_app_id(&id).is_some()
            || layout::layout_confirm_from_app_id(&id).is_some()
            || layout::layout_tooltip_from_app_id(&id).is_some()
            || layout::pinned_window_reveal_from_app_id(&id).is_some()
            || self.frame_widget_has_material(index);
        if !glass {
            return None;
        }
        let bounds = self
            .layout_motion_rect(index)
            .or_else(|| self.layout_target_rect(index))?;
        let offset = self.canvas_peek_offset(index);
        Some(Rectangle::new(
            (
                (bounds.x + offset.x).round() as i32,
                (bounds.y + offset.y).round() as i32,
            )
                .into(),
            (bounds.width.round() as i32, bounds.height.round() as i32).into(),
        ))
    }

    pub(super) fn layout_glass_radius(&self, index: usize) -> f64 {
        let Some(id) = window_app_id(&self.windows[index].surface) else {
            return 20.;
        };
        if layout::layout_tooltip_from_app_id(&id).is_some()
            || layout::frame_group_grip_from_app_id(&id).is_some()
        {
            10.
        } else {
            20.
        }
    }
}
