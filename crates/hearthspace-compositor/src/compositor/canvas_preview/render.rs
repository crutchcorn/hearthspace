//! Search keeps its legible screen-space ring; switching scales the reference outline.
use super::App;
use crate::compositor::{rendering::HearthspaceRenderElement, selection::Style};
use smithay::{backend::renderer::gles::GlesRenderer, utils::Rectangle};

impl App {
    pub(in crate::compositor) fn canvas_preview_selection_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        index: usize,
    ) -> Vec<HearthspaceRenderElement> {
        let switching = self.window_switcher_selected_id() == Some(self.windows[index].id);
        if !self.canvas_search_window_selected(self.windows[index].id) && !switching {
            return Vec::new();
        }
        let Some((pose, alpha)) = self.canvas_preview_pose(index) else {
            return Vec::new();
        };
        let window_scale = self.window_render_scale(index);
        let outline_scale = if switching { window_scale } else { 1. };
        let padding = 7. * outline_scale;
        let bounds = if switching {
            // Enclose the fractional outline without clipping its outer edge.
            let left = (pose.x - padding).floor() as i32;
            let top = (pose.y - padding).floor() as i32;
            let right = (pose.x + pose.width + padding).ceil() as i32;
            let bottom = (pose.y + pose.height + padding).ceil() as i32;
            Rectangle::new(
                (left, top).into(),
                (
                    right.saturating_sub(left).max(1),
                    bottom.saturating_sub(top).max(1),
                )
                    .into(),
            )
        } else {
            Rectangle::new(
                (pose.x.round() as i32 - 7, pose.y.round() as i32 - 7).into(),
                (
                    pose.width.round() as i32 + 14,
                    pose.height.round() as i32 + 14,
                )
                    .into(),
            )
        };
        let mut ink = self.palette().accent.components;
        ink[3] *= alpha as f32;
        let style = Style {
            ink,
            radius: (20. * window_scale + padding) as f32,
            stroke: (2. * outline_scale) as f32,
            fill: 0.,
        };
        self.canvas_preview
            .selection
            .element(renderer, bounds, style)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}
