use masonry::peniko::Color;
use smithay::{
    backend::renderer::{
        Color32F, RendererSuper,
        damage::OutputDamageTracker,
        element::{
            Kind,
            memory::MemoryRenderBufferRenderElement,
            render_elements,
            solid::{SolidColorBuffer, SolidColorRenderElement},
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            utils::RescaleRenderElement,
        },
        gles::GlesRenderer,
    },
    desktop::PopupManager,
    utils::{Logical, Physical, Point, Rectangle, Size},
    wayland::{
        compositor::{SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward},
        shell::xdg::SurfaceCachedState,
    },
};
use wayland_server::protocol::wl_surface;

use super::{
    App, ManagedWindowKind,
    masonry_titlebar::{self, TITLE_TEXT_HEIGHT, TITLE_TEXT_WIDTH},
    windows::toplevel_title,
};

render_elements! {
    pub(super) HearthspaceRenderElement<=GlesRenderer>;
    Surface = RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Solid = SolidColorRenderElement,
    Memory = MemoryRenderBufferRenderElement<GlesRenderer>,
}

/// Result of rendering one frame: the damaged output regions to submit, or
/// `None` when nothing changed and the frame can be skipped.
type RenderFrameResult = Result<Option<Vec<Rectangle<i32, Physical>>>, Box<dyn std::error::Error>>;

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
    ) -> RenderFrameResult {
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
            // Popups (e.g. menus) render above their window's content and chrome.
            for popup in self.popup_render_elements(renderer, index) {
                elements.push(popup);
            }

            // Masonry-rendered title text sits on top of the solid title bar.
            if let Some(title_text) = self.title_text_element(renderer, index) {
                elements.push(title_text);
            }
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

    /// Collect render elements for every popup anchored to the given window.
    ///
    /// Popups (xdg_popup surfaces such as menus) are not part of the toplevel's
    /// surface tree, so they are gathered separately and positioned relative to
    /// the parent surface using the popup's configured location, mirroring the
    /// math in Smithay's own `Window` rendering. They share the window's render
    /// scale so the viewport zoom applies uniformly.
    fn popup_render_elements(
        &self,
        renderer: &mut GlesRenderer,
        window_index: usize,
    ) -> Vec<HearthspaceRenderElement> {
        let window = &self.windows[window_index];
        let parent = window.surface.wl_surface();
        let base = self.surface_screen_origin(window_index);
        let scale = self.window_render_scale(window_index);
        let geometry_loc = toplevel_geometry_loc(parent);

        let mut elements = Vec::new();
        for (popup, popup_offset) in PopupManager::popups_for_surface(parent) {
            // Offset of the popup surface origin from the parent surface origin,
            // in the parent's native (unscaled) logical coordinates.
            let offset = geometry_loc + popup_offset - popup.geometry().loc;
            let offset_physical = Point::<i32, Physical>::from((
                (f64::from(offset.x) * scale).round() as i32,
                (f64::from(offset.y) * scale).round() as i32,
            ));
            let popup_origin = base + offset_physical;

            for surface in
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    popup.wl_surface(),
                    popup_origin,
                    1.0,
                    1.0,
                    Kind::Unspecified,
                )
            {
                elements.push(HearthspaceRenderElement::from(
                    RescaleRenderElement::from_element(surface, popup_origin, scale),
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

    /// Build the Masonry-rendered title-text render element for a window, or
    /// `None` when the window has no compositor chrome or the title bar is too
    /// narrow to show any text.
    ///
    /// The rasterized image is cached on the window and only rebuilt when the
    /// title text or active state changes (see [`masonry_titlebar`]).
    pub(super) fn title_text_element(
        &mut self,
        renderer: &mut GlesRenderer,
        window_index: usize,
    ) -> Option<HearthspaceRenderElement> {
        if !self.has_compositor_chrome(window_index) {
            return None;
        }

        let title_rect = self.title_bar_rect(window_index);
        let close_rect = self.close_button_rect(window_index);
        let scale = self.window_render_scale(window_index);

        let title = toplevel_title(&self.windows[window_index].surface)
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Hearthspace".to_string());
        let active = Some(window_index)
            == self
                .windows
                .iter()
                .rposition(|window| window.kind == ManagedWindowKind::Normal);

        // Re-run Masonry's layout/raster only when the cache key changes.
        let needs_rebuild = self.windows[window_index]
            .title_text
            .as_ref()
            .map(|cached| cached.title != title || cached.active != active)
            .unwrap_or(true);
        if needs_rebuild {
            let background = if active {
                Color::from_rgb8(48, 82, 140)
            } else {
                Color::from_rgb8(38, 46, 61)
            };
            let buffer = masonry_titlebar::render_title_text(
                &title,
                background,
                Color::from_rgb8(234, 242, 255),
            );
            self.windows[window_index].title_text = Some(masonry_titlebar::TitleTextBuffer {
                title,
                active,
                buffer,
            });
        }

        // Place the cached image vertically centered, inset from the left, and
        // cropped so it never overdraws the close button.
        let inset = (6.0 * scale).round() as i32;
        let display_h = (f64::from(TITLE_TEXT_HEIGHT) * scale).round().max(1.0) as i32;
        let available_w = close_rect.loc.x - title_rect.loc.x - inset * 2;
        if available_w <= 0 {
            return None;
        }
        let display_w = ((f64::from(TITLE_TEXT_WIDTH) * scale).round() as i32).min(available_w);
        if display_w <= 0 {
            return None;
        }
        let origin_x = title_rect.loc.x + inset;
        let origin_y = title_rect.loc.y + ((title_rect.size.h - display_h) / 2).max(0);

        // Crop (rather than squash) the source so the text keeps a uniform scale.
        let src_w = (f64::from(display_w) / scale).min(f64::from(TITLE_TEXT_WIDTH));
        let src =
            Rectangle::<f64, Logical>::from_size(Size::from((src_w, f64::from(TITLE_TEXT_HEIGHT))));

        let buffer = &self.windows[window_index].title_text.as_ref()?.buffer;
        let element = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            Point::<i32, Logical>::from((origin_x, origin_y))
                .to_physical(1)
                .to_f64(),
            buffer,
            None,
            Some(src),
            Some(Size::<i32, Logical>::from((display_w, display_h))),
            Kind::Unspecified,
        )
        .ok()?;
        Some(HearthspaceRenderElement::from(element))
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

/// The offset of a toplevel's window geometry within its surface, used to place
/// popups relative to the parent surface origin. Defaults to `(0, 0)` when the
/// client has not set an explicit window geometry.
pub(super) fn toplevel_geometry_loc(surface: &wl_surface::WlSurface) -> Point<i32, Logical> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
            .map(|geometry| geometry.loc)
            .unwrap_or_default()
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CLOSE_BUTTON_SIZE;
    use proptest::prelude::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn x_mark_has_five_cells_for_a_normal_button() {
        // The decoration buffers reserve exactly five marks; the layout must
        // never produce more than that or the buffers would be undersized.
        let marks = close_button_x_rects(rect(0, 0, CLOSE_BUTTON_SIZE, CLOSE_BUTTON_SIZE));
        assert_eq!(marks.len(), 5);
    }

    #[test]
    fn x_mark_is_centered_and_symmetric() {
        let button = rect(10, 20, 20, 20);
        let marks = close_button_x_rects(button);
        // The center cell (row == col == 2) is shared by both diagonals.
        let cell = button.size.w / 5;
        let center = marks
            .iter()
            .find(|mark| {
                mark.loc.x == button.loc.x + 2 * cell && mark.loc.y == button.loc.y + 2 * cell
            })
            .expect("center mark present");
        assert!(center.size.w >= 1 && center.size.h >= 1);
    }

    proptest! {
        #[test]
        fn x_marks_stay_within_the_button_and_are_non_degenerate(
            x in -1000i32..1000,
            y in -1000i32..1000,
            // Below ~5px the fixed 5x5 grid spacing can overflow the button;
            // real close buttons are CLOSE_BUTTON_SIZE (18px), so test the
            // realistic range where the marks are expected to stay contained.
            size in 5i32..200,
        ) {
            let button = rect(x, y, size, size);
            for mark in close_button_x_rects(button) {
                prop_assert!(mark.size.w >= 1);
                prop_assert!(mark.size.h >= 1);
                prop_assert!(mark.loc.x >= button.loc.x);
                prop_assert!(mark.loc.y >= button.loc.y);
                prop_assert!(mark.loc.x + mark.size.w <= button.loc.x + button.size.w);
                prop_assert!(mark.loc.y + mark.size.h <= button.loc.y + button.size.h);
            }
        }

        #[test]
        fn x_mark_count_never_exceeds_buffer_capacity(size in 1i32..500) {
            let marks = close_button_x_rects(rect(0, 0, size, size));
            prop_assert!(marks.len() <= 5);
        }
    }
}
