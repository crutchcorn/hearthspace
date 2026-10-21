//! Selection stays legible at every preview scale: two pixels with a five-pixel gap.
use super::App;
use crate::compositor::{rendering::HearthspaceRenderElement, selection::Style};
use smithay::{backend::renderer::gles::GlesRenderer, utils::Rectangle};

impl App {
    pub(in crate::compositor) fn canvas_search_selection_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        index: usize,
    ) -> Vec<HearthspaceRenderElement> {
        if !self.canvas_search_window_selected(self.windows[index].id) {
            return Vec::new();
        }
        let Some((pose, alpha)) = self.canvas_search_pose(index) else {
            return Vec::new();
        };
        let bounds = Rectangle::new(
            (pose.x.round() as i32 - 7, pose.y.round() as i32 - 7).into(),
            (
                pose.width.round() as i32 + 14,
                pose.height.round() as i32 + 14,
            )
                .into(),
        );
        let mut ink = self.palette().accent.components;
        ink[3] *= alpha as f32;
        let style = Style {
            ink,
            radius: 20. * self.window_render_scale(index) as f32 + 7.,
            stroke: 2.,
            fill: 0.,
        };
        self.canvas_search
            .selection
            .element(renderer, bounds, style)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}
