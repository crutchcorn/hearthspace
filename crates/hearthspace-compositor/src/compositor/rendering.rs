use smithay::{
    backend::renderer::{
        Color32F, RendererSuper,
        damage::OutputDamageTracker,
        element::{
            Kind,
            memory::MemoryRenderBufferRenderElement,
            render_elements,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            texture::TextureRenderElement,
            utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
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

use super::{App, ManagedWindowKind, masonry_titlebar, windows::toplevel_title};

mod background;

pub(in crate::compositor) use background::BackgroundImageTile;

type GlesTexture = <GlesRenderer as RendererSuper>::TextureId;

render_elements! {
    pub(super) HearthspaceRenderElement<=GlesRenderer>;
    Surface = RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Memory = MemoryRenderBufferRenderElement<GlesRenderer>,
    Texture = TextureRenderElement<GlesTexture>,
}

render_elements! {
    pub(super) TranslatedHearthspaceRenderElement<=GlesRenderer>;
    Scaled = RescaleRenderElement<RelocateRenderElement<HearthspaceRenderElement>>,
    Unscaled = RelocateRenderElement<HearthspaceRenderElement>,
}

/// Result of rendering one frame: the damaged output regions to submit, or
/// `None` when nothing changed and the frame can be skipped.
type RenderFrameResult = Result<Option<Vec<Rectangle<i32, Physical>>>, Box<dyn std::error::Error>>;

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
        self.render_frame_at(
            renderer,
            framebuffer,
            damage_tracker,
            age,
            Rectangle::from_size(self.output_logical_size()),
            self.primary_output_scale(),
        )
    }

    pub(super) fn render_frame_at(
        &mut self,
        renderer: &mut GlesRenderer,
        framebuffer: &mut <GlesRenderer as RendererSuper>::Framebuffer<'_>,
        damage_tracker: &mut OutputDamageTracker,
        age: usize,
        output_rect: Rectangle<i32, Logical>,
        output_scale: i32,
    ) -> RenderFrameResult {
        let output_location = output_rect.loc;
        let output_offset = Point::<i32, Physical>::from((-output_location.x, -output_location.y));
        let output_scale = output_scale.max(1);
        let render_scale = f64::from(output_scale);
        let foreground = self.collect_render_elements(renderer, output_rect);
        let mut elements = foreground
            .into_iter()
            .map(|element| {
                let relocated =
                    RelocateRenderElement::from_element(element, output_offset, Relocate::Relative);
                TranslatedHearthspaceRenderElement::from(RescaleRenderElement::from_element(
                    relocated,
                    Point::<i32, Physical>::from((0, 0)),
                    render_scale,
                ))
            })
            .collect::<Vec<_>>();

        // Background tiles are wallpaper pixels, not client logical pixels, so
        // they opt out of monitor DPI scaling while still being translated into
        // this output's framebuffer-local coordinate space.
        for tile in self.background_image_elements(renderer, output_rect, output_scale) {
            elements.push(TranslatedHearthspaceRenderElement::from(
                RelocateRenderElement::from_element(
                    HearthspaceRenderElement::from(tile),
                    output_offset,
                    Relocate::Relative,
                ),
            ));
        }
        let result = damage_tracker.render_output(
            renderer,
            framebuffer,
            age,
            &elements,
            // The repeated background image is opaque and covers the output.
            Color32F::new(0.0, 0.0, 0.0, 0.0),
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
        output_rect: Rectangle<i32, Logical>,
    ) -> Vec<HearthspaceRenderElement> {
        self.refresh_normal_window_outputs();

        let mut elements = Vec::new();
        if let Some(element) = self.software_cursor_element(renderer, output_rect) {
            elements.push(HearthspaceRenderElement::from(element));
        }

        for index in (0..self.windows.len()).rev() {
            // Popups (e.g. menus) render above their window's content and chrome.
            for popup in self.popup_render_elements(renderer, index) {
                elements.push(popup);
            }

            // Masonry-rasterized title bar (background, title, close button)
            // layered above the window's content.
            if let Some(titlebar) = self.titlebar_element(renderer, index) {
                elements.push(titlebar);
            }

            let scale = self.window_render_scale(index);
            let origins = match self.windows[index].kind {
                ManagedWindowKind::ShellBar => self.shell_bar_screen_origins(index),
                ManagedWindowKind::Launcher => self.launcher_screen_origins(index),
                ManagedWindowKind::ShellWatcher => Vec::new(),
                ManagedWindowKind::Normal => {
                    vec![self.surface_screen_origin(index)]
                }
            };
            for origin in origins {
                for surface in self.window_render_elements_at(renderer, index, origin) {
                    elements.push(HearthspaceRenderElement::from(
                        RescaleRenderElement::from_element(surface, origin, scale),
                    ));
                }
            }
        }

        elements
    }

    fn software_cursor_element(
        &mut self,
        renderer: &mut GlesRenderer,
        output_rect: Rectangle<i32, Logical>,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if !self.software_cursor_visible {
            return None;
        }
        if !point_in_logical_rect(self.pointer_location, output_rect) {
            return None;
        }

        let location = Point::<f64, Physical>::from((
            self.pointer_location.x.round() - f64::from(self.software_cursor.hotspot.0),
            self.pointer_location.y.round() - f64::from(self.software_cursor.hotspot.1),
        ));

        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            &self.software_cursor.buffer,
            None,
            None,
            None,
            Kind::Cursor,
        )
        .ok()
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

    fn window_render_elements_at(
        &self,
        renderer: &mut GlesRenderer,
        window_index: usize,
        origin: Point<i32, Physical>,
    ) -> Vec<WaylandSurfaceRenderElement<GlesRenderer>> {
        let window = &self.windows[window_index];
        render_elements_from_surface_tree(
            renderer,
            window.surface.wl_surface(),
            origin,
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
            ManagedWindowKind::ShellBar | ManagedWindowKind::Launcher => 1.0,
            ManagedWindowKind::ShellWatcher => 1.0,
        }
    }

    /// Build the Masonry-rendered title-bar render element for a window, or
    /// `None` when the window has no compositor chrome or the bar has no
    /// on-screen area.
    ///
    /// The whole bar (gradient background, title text, and rounded close
    /// button) is rasterized once at native (unscaled) size, cached on the
    /// window, and scaled to the on-screen title-bar rect by the memory
    /// element. The cache is only rebuilt when the bar width, title, or active
    /// state changes (see [`masonry_titlebar`]).
    pub(super) fn titlebar_element(
        &mut self,
        renderer: &mut GlesRenderer,
        window_index: usize,
    ) -> Option<HearthspaceRenderElement> {
        if !self.has_compositor_chrome(window_index) {
            return None;
        }

        let screen_rect = self.title_bar_rect(window_index);
        if screen_rect.size.w <= 0 || screen_rect.size.h <= 0 {
            return None;
        }

        // Native (unscaled) bar size the Masonry image is rasterized at; the
        // memory element scales it to `screen_rect` so the viewport zoom is
        // applied uniformly with the rest of the window.
        let native_size = self.title_bar_canvas_rect(window_index).size;
        let native_w = native_size.w.max(1);

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
            .titlebar
            .as_ref()
            .map(|cached| {
                cached.width != native_w || cached.title != title || cached.active != active
            })
            .unwrap_or(true);
        if needs_rebuild {
            let buffer = masonry_titlebar::render_titlebar(native_w, &title, active);
            self.windows[window_index].titlebar = Some(masonry_titlebar::TitlebarBuffer {
                width: native_w,
                title,
                active,
                buffer,
            });
        }

        let buffer = &self.windows[window_index].titlebar.as_ref()?.buffer;
        // Source the full native-sized Masonry image and let the memory element
        // stretch it to the (zoom-scaled) `screen_rect`. Passing the native
        // `src` is what makes the bar scale with the window: with `src = None`
        // the element would instead sample a screen-sized region out of the
        // native buffer, leaving the chrome at native size (so the close button
        // drifts off the right edge as the window grows under zoom).
        let native_src = Rectangle::from_size(Size::<f64, Logical>::from((
            f64::from(native_w),
            f64::from(native_size.h),
        )));
        let element = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            screen_rect.loc.to_physical(1).to_f64(),
            buffer,
            None,
            Some(native_src),
            Some(screen_rect.size),
            Kind::Unspecified,
        )
        .ok()?;
        Some(HearthspaceRenderElement::from(element))
    }
}

fn point_in_logical_rect(point: Point<f64, Logical>, rect: Rectangle<i32, Logical>) -> bool {
    point.x >= f64::from(rect.loc.x)
        && point.x < f64::from(rect.loc.x + rect.size.w.max(1))
        && point.y >= f64::from(rect.loc.y)
        && point.y < f64::from(rect.loc.y + rect.size.h.max(1))
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

/// The size of a toplevel's window geometry (the visible window excluding any
/// client-side decoration shadow margins). `None` when the client has not set
/// an explicit window geometry, in which case the caller falls back to the
/// surface-tree bounding box.
pub(super) fn toplevel_geometry_size(
    surface: &wl_surface::WlSurface,
) -> Option<Size<i32, Logical>> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
            .map(|geometry| geometry.size)
    })
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
    use smithay::utils::{Logical, Point, Rectangle};

    use super::point_in_logical_rect;

    #[test]
    fn point_in_logical_rect_excludes_other_outputs() {
        let rect = Rectangle::<i32, Logical>::new((1280, 0).into(), (1024, 768).into());

        assert!(point_in_logical_rect(Point::from((1280.0, 12.0)), rect));
        assert!(point_in_logical_rect(Point::from((2303.0, 767.0)), rect));
        assert!(!point_in_logical_rect(Point::from((1279.0, 12.0)), rect));
        assert!(!point_in_logical_rect(Point::from((2304.0, 12.0)), rect));
    }
}
