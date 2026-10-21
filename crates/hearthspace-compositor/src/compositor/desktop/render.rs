//! A rounded screen-space marquee drawn with the shared selection primitive.
use super::{App, Gesture, geometry};
use crate::compositor::rendering::HearthspaceRenderElement;
pub(super) use crate::compositor::selection::Cache;
use smithay::backend::renderer::gles::GlesRenderer;

impl App {
    pub(in crate::compositor) fn desktop_selection_elements(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Vec<HearthspaceRenderElement> {
        let Some(Gesture::Marquee { start, current, .. }) = &self.desktop.gesture else {
            return Vec::new();
        };
        let rectangle = geometry::rectangle(
            self.canvas_to_screen(*start),
            self.canvas_to_screen(*current),
        )
        .to_i32_up();
        if rectangle.is_empty() {
            return Vec::new();
        }
        let ink = self.palette().accent.components;
        self.desktop
            .selection_cache
            .element(
                renderer,
                rectangle,
                crate::compositor::selection::Style {
                    ink,
                    radius: 5.,
                    stroke: 1.,
                    fill: 0.07,
                },
            )
            .into_iter()
            .map(Into::into)
            .collect()
    }
}
