use smithay::{
    backend::renderer::{
        Color32F,
        element::{
            Kind,
            solid::{SolidColorBuffer, SolidColorRenderElement},
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
        },
        gles::GlesRenderer,
    },
    utils::{Logical, Rectangle},
    wayland::compositor::{SurfaceAttributes, TraversalAction, with_surface_tree_downward},
};
use wayland_server::protocol::wl_surface;

use super::{App, ManagedWindowKind};

impl App {
    pub(super) fn window_render_elements(
        &self,
        renderer: &mut GlesRenderer,
        window_index: usize,
    ) -> Vec<WaylandSurfaceRenderElement<GlesRenderer>> {
        let window = &self.windows[window_index];
        render_elements_from_surface_tree(
            renderer,
            window.surface.wl_surface(),
            self.surface_screen_origin(window_index),
            self.window_render_scale(window_index),
            1.0,
            Kind::Unspecified,
        )
    }

    pub(super) fn window_render_scale(&self, window_index: usize) -> f64 {
        match self.windows[window_index].kind {
            ManagedWindowKind::Normal => self.viewport_scale,
            ManagedWindowKind::ShellBar => 1.0,
        }
    }

    pub(super) fn title_bar_elements(&self, window_index: usize) -> Vec<SolidColorRenderElement> {
        if !self.has_compositor_chrome(window_index) {
            return Vec::new();
        }

        let mut elements = Vec::new();

        let rect = self.title_bar_rect(window_index);
        let close_rect = self.close_button_rect(window_index);

        for x_rect in close_button_x_rects(close_rect) {
            elements.push(solid_element(x_rect, Color32F::new(1.0, 0.95, 0.95, 1.0)));
        }

        elements.push(solid_element(
            close_rect,
            Color32F::new(0.72, 0.10, 0.12, 1.0),
        ));

        let focused_color = Color32F::new(0.19, 0.32, 0.55, 1.0);
        let unfocused_color = Color32F::new(0.15, 0.18, 0.24, 1.0);

        elements.push(solid_element(
            rect,
            if Some(window_index)
                == self
                    .windows
                    .iter()
                    .rposition(|window| window.kind == ManagedWindowKind::Normal)
            {
                focused_color
            } else {
                unfocused_color
            },
        ));

        elements
    }
}

fn solid_element(rect: Rectangle<i32, Logical>, color: Color32F) -> SolidColorRenderElement {
    let buffer = SolidColorBuffer::new(rect.size, color);
    SolidColorRenderElement::from_buffer(
        &buffer,
        rect.loc.to_physical(1),
        1.0,
        1.0,
        Kind::Unspecified,
    )
}

fn close_button_x_rects(rect: Rectangle<i32, Logical>) -> Vec<Rectangle<i32, Logical>> {
    let cell = (rect.size.w / 5).max(1);
    let mark_size = cell.min(rect.size.h / 5).max(1);
    let mut rects = Vec::new();

    for row in 1..4 {
        for col in 1..4 {
            if row == col || row + col == 4 {
                rects.push(Rectangle::new(
                    (
                        rect.loc.x + col * cell + (cell - mark_size) / 2,
                        rect.loc.y + row * cell + (cell - mark_size) / 2,
                    )
                        .into(),
                    (mark_size, mark_size).into(),
                ));
            }
        }
    }

    rects
}

pub(super) fn send_frames_surface_tree(surface: &wl_surface::WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}
