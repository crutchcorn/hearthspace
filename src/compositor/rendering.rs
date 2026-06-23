use smithay::{
    backend::renderer::{
        Color32F, RendererSuper,
        damage::OutputDamageTracker,
        element::{
            Kind, render_elements,
            solid::{SolidColorBuffer, SolidColorRenderElement},
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            utils::RescaleRenderElement,
        },
        gles::GlesRenderer,
    },
    utils::{Logical, Physical, Point, Rectangle},
    wayland::compositor::{SurfaceAttributes, TraversalAction, with_surface_tree_downward},
};
use wayland_server::protocol::wl_surface;

use super::{App, ManagedWindowKind};

render_elements! {
    pub(super) HearthspaceRenderElement<=GlesRenderer>;
    Surface = RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Solid = SolidColorRenderElement,
}

/// Persistent solid-color buffers backing a window's server-side decorations.
///
/// Reusing the same [`SolidColorBuffer`]s across frames keeps the resulting
/// render elements' ids stable, so the damage tracker can skip them when the
/// title bar has not moved or changed color instead of repainting every frame.
/// The X-mark count is fixed by [`close_button_x_rects`].
#[derive(Debug)]
pub(super) struct WindowDecorationBuffers {
    title_bar: SolidColorBuffer,
    close_button: SolidColorBuffer,
    close_marks: [SolidColorBuffer; 5],
}

impl Default for WindowDecorationBuffers {
    fn default() -> Self {
        Self {
            title_bar: SolidColorBuffer::default(),
            close_button: SolidColorBuffer::default(),
            close_marks: std::array::from_fn(|_| SolidColorBuffer::default()),
        }
    }
}

impl App {
    /// Render the current scene through an [`OutputDamageTracker`].
    ///
    /// The damage tracker compares this frame's elements against the previous
    /// frame and only clears and redraws the regions that actually changed,
    /// rather than repainting the whole output every frame. The returned value
    /// is the damaged region (in output coordinates) to hand to the backend's
    /// buffer swap, or `None` when nothing changed and the frame can be skipped.
    ///
    /// This is backend-agnostic: it only depends on a [`GlesRenderer`] and the
    /// shared [`App`] state, so both the winit and (future) DRM backends share
    /// it. Backend-specific buffer binding, frame submission, and client frame
    /// callbacks live in the backend's own render path.
    pub(super) fn render_frame(
        &mut self,
        renderer: &mut GlesRenderer,
        framebuffer: &mut <GlesRenderer as RendererSuper>::Framebuffer<'_>,
        damage_tracker: &mut OutputDamageTracker,
        age: usize,
    ) -> Result<Option<Vec<Rectangle<i32, Physical>>>, Box<dyn std::error::Error>> {
        let elements = self.collect_render_elements(renderer);
        let result = damage_tracker.render_output(
            renderer,
            framebuffer,
            age,
            &elements,
            Color32F::new(0.04, 0.05, 0.07, 1.0),
        )?;
        Ok(result.damage.cloned())
    }

    /// Collect every render element for the current frame in front-to-back
    /// (topmost-first) order, as expected by [`OutputDamageTracker`].
    ///
    /// Window surfaces are built at native scale and wrapped in a
    /// [`RescaleRenderElement`] so the viewport zoom is applied uniformly while
    /// the damage tracker still works in a single output coordinate space.
    fn collect_render_elements(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Vec<HearthspaceRenderElement> {
        let mut elements = Vec::new();
        for index in (0..self.windows.len()).rev() {
            for solid in self.title_bar_elements(index) {
                elements.push(HearthspaceRenderElement::from(solid));
            }

            let origin = self.surface_screen_origin(index);
            let scale = self.window_render_scale(index);
            for surface in self.window_render_elements(renderer, index) {
                elements.push(HearthspaceRenderElement::from(
                    RescaleRenderElement::from_element(surface, origin, scale),
                ));
            }
        }
        elements
    }

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
            // Built at native scale; the viewport zoom is applied by wrapping
            // these elements in a `RescaleRenderElement` in the caller.
            1.0,
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

    pub(super) fn title_bar_elements(
        &mut self,
        window_index: usize,
    ) -> Vec<SolidColorRenderElement> {
        if !self.has_compositor_chrome(window_index) {
            return Vec::new();
        }

        let title_rect = self.title_bar_rect(window_index);
        let close_rect = self.close_button_rect(window_index);
        let mark_rects = close_button_x_rects(close_rect);
        let title_color = if Some(window_index)
            == self
                .windows
                .iter()
                .rposition(|window| window.kind == ManagedWindowKind::Normal)
        {
            Color32F::new(0.19, 0.32, 0.55, 1.0)
        } else {
            Color32F::new(0.15, 0.18, 0.24, 1.0)
        };

        // Update and reuse the persisted buffers so their element ids stay
        // stable across frames; the damage tracker then skips them whenever the
        // title bar has not moved or changed color.
        let decorations = &mut self.windows[window_index].decoration_buffers;
        let mut elements = Vec::with_capacity(mark_rects.len() + 2);

        for (buffer, rect) in decorations.close_marks.iter_mut().zip(&mark_rects) {
            buffer.update(rect.size, Color32F::new(1.0, 0.95, 0.95, 1.0));
            elements.push(solid_element(buffer, rect.loc));
        }

        decorations
            .close_button
            .update(close_rect.size, Color32F::new(0.72, 0.10, 0.12, 1.0));
        elements.push(solid_element(&decorations.close_button, close_rect.loc));

        decorations.title_bar.update(title_rect.size, title_color);
        elements.push(solid_element(&decorations.title_bar, title_rect.loc));

        elements
    }
}

fn solid_element(
    buffer: &SolidColorBuffer,
    location: Point<i32, Logical>,
) -> SolidColorRenderElement {
    SolidColorRenderElement::from_buffer(
        buffer,
        location.to_physical(1),
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
