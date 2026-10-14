use smithay::{
    backend::renderer::{
        element::{
            Kind,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            utils::RescaleRenderElement,
        },
        gles::GlesRenderer,
    },
    utils::{Logical, Rectangle},
};

use super::{App, HearthspaceRenderElement};

impl App {
    pub(super) fn collect_lock_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
    ) -> Vec<HearthspaceRenderElement> {
        let mut elements = self.software_cursor_elements(renderer, output);
        if let Some(surface) = self.lock_surface_on_output(output) {
            let origin = output.loc.to_physical(1);
            let surfaces: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    renderer,
                    &surface,
                    origin,
                    1.0,
                    1.0,
                    Kind::Unspecified,
                );
            for surface in surfaces {
                elements.push(HearthspaceRenderElement::from(
                    RescaleRenderElement::from_element(surface, origin, 1.0),
                ));
            }
        }
        // render_frame_at supplies an opaque black clear. A missing, destroyed,
        // transparent or still-configuring lock surface cannot reveal clients.
        elements
    }
}
