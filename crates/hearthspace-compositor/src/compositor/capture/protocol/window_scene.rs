//! Client content at committed backing density, without canvas or desktop effects.
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Color32F, Offscreen,
            damage::OutputDamageTracker,
            element::{
                Kind, render_elements,
                surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
                utils::RescaleRenderElement,
            },
            gles::{GlesRenderbuffer, GlesRenderer},
        },
    },
    desktop::find_popup_root_surface,
    utils::{Logical, Physical, Point, Rectangle, Transform},
    wayland::compositor::get_parent,
};

use super::{App, toplevel::WindowGeometry};
use crate::compositor::rendering::HearthspaceRenderElement;

render_elements! {
    WindowCaptureElement<=GlesRenderer>;
    Surface = WaylandSurfaceRenderElement<GlesRenderer>,
    Cursor = RescaleRenderElement<HearthspaceRenderElement>,
}

impl App {
    pub(super) fn render_capture_window(
        &mut self,
        renderer: &mut GlesRenderer,
        index: usize,
        geometry: WindowGeometry,
        draw_cursor: bool,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut elements: Vec<WindowCaptureElement> = Vec::new();
        if draw_cursor {
            elements.extend(self.capture_window_cursor(renderer, index, geometry));
        }
        for (surface, origin) in self.capture_window_trees(index, geometry.rect.loc) {
            elements.extend(
                render_elements_from_surface_tree::<_, WindowCaptureElement>(
                    renderer,
                    &surface,
                    (origin - geometry.rect.loc)
                        .to_f64()
                        .to_physical(geometry.scale)
                        .to_i32_round(),
                    geometry.scale,
                    1.0,
                    Kind::Unspecified,
                ),
            );
        }
        let size = (geometry.size.w, geometry.size.h);
        let mut buffer: GlesRenderbuffer = renderer.create_buffer(Fourcc::Abgr8888, size.into())?;
        let mut target = renderer.bind(&mut buffer)?;
        // The target clips subsurfaces/popups to the selected window geometry.
        // Clearing every frame prevents transparent content exposing any prior
        // scene, shell, wallpaper, or another capture's pixels.
        OutputDamageTracker::new(size, geometry.scale, Transform::Normal).render_output(
            renderer,
            &mut target,
            0,
            &elements,
            Color32F::new(0., 0., 0., 1.),
        )?;
        crate::compositor::capture::read_pixels(renderer, &target, size.into(), false)
    }

    fn capture_window_cursor(
        &mut self,
        renderer: &mut GlesRenderer,
        index: usize,
        geometry: WindowGeometry,
    ) -> Vec<WindowCaptureElement> {
        let Some(mut surface) = self.pointer.current_focus() else {
            return Vec::new();
        };
        // PopupManager indexes popup roots, not the subsurfaces receiving input.
        while let Some(parent) = get_parent(&surface) {
            surface = parent;
        }
        let owner = self
            .popups
            .find_popup(&surface)
            .and_then(|popup| find_popup_root_surface(&popup).ok())
            .and_then(|root| self.window_index_for_surface(&root))
            .or_else(|| self.window_index_for_surface(&surface));
        #[cfg(feature = "xwayland")]
        let owner = owner.map(|i| self.x11_popup_anchor(i).map_or(i, |(parent, _)| parent));
        if owner != Some(index) {
            return Vec::new();
        }
        let local = self
            .window_projection(index)
            .screen_to_surface(self.pointer_location)
            - geometry.rect.loc.to_f64();
        let bounds: Rectangle<i32, Logical> = Rectangle::from_size(geometry.rect.size);
        if local.x < 0.
            || local.y < 0.
            || local.x >= f64::from(bounds.size.w)
            || local.y >= f64::from(bounds.size.h)
        {
            return Vec::new();
        }
        let previous_location = std::mem::replace(&mut self.pointer_location, local);
        let previous_visible = std::mem::replace(&mut self.software_cursor_visible, true);
        let elements = self.software_cursor_elements(renderer, bounds);
        self.pointer_location = previous_location;
        self.software_cursor_visible = previous_visible;
        elements
            .into_iter()
            .map(|element| {
                WindowCaptureElement::from(RescaleRenderElement::from_element(
                    element,
                    Point::<i32, Physical>::from((0, 0)),
                    geometry.scale,
                ))
            })
            .collect()
    }
}
