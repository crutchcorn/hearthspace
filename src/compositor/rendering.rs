use smithay::{
    backend::renderer::{
        Color32F, RendererSuper,
        damage::OutputDamageTracker,
        element::{
            Id, Kind,
            memory::MemoryRenderBufferRenderElement,
            render_elements,
            solid::SolidColorRenderElement,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            utils::RescaleRenderElement,
        },
        gles::GlesRenderer,
        utils::CommitCounter,
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
use crate::config::{BACKGROUND_DOT_SIZE, BACKGROUND_DOT_SPACING};

render_elements! {
    pub(super) HearthspaceRenderElement<=GlesRenderer>;
    Surface = RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Memory = MemoryRenderBufferRenderElement<GlesRenderer>,
    Solid = SolidColorRenderElement,
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
        let elements = self.collect_render_elements(renderer);
        let result = damage_tracker.render_output(
            renderer,
            framebuffer,
            age,
            &elements,
            // White canvas; the gray dot grid is drawn on top of it.
            Color32F::new(1.0, 1.0, 1.0, 1.0),
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

            // Masonry-rasterized title bar (background, title, close button)
            // layered above the window's content.
            if let Some(titlebar) = self.titlebar_element(renderer, index) {
                elements.push(titlebar);
            }

            let origin = self.surface_screen_origin(index);
            let scale = self.window_render_scale(index);
            for surface in self.window_render_elements(renderer, index) {
                elements.push(HearthspaceRenderElement::from(
                    RescaleRenderElement::from_element(surface, origin, scale),
                ));
            }
        }

        // The dot grid sits behind every window, directly on top of the white
        // clear color, so it is appended last in this front-to-back ordering.
        for dot in self.background_dot_elements() {
            elements.push(HearthspaceRenderElement::from(dot));
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

    /// Build the background dot-grid render elements for the current viewport.
    ///
    /// The dots are laid out on a fixed grid in *canvas* space, so they pan and
    /// zoom together with the viewport and frame canvas movement even when no
    /// windows are open. Only the dots that fall inside the visible region are
    /// emitted. Each visible slot reuses a stable [`Id`] across frames so the
    /// damage tracker can tell when a dot actually moves.
    fn background_dot_elements(&mut self) -> Vec<SolidColorRenderElement> {
        let spacing = f64::from(BACKGROUND_DOT_SPACING);
        if spacing <= 0.0 {
            return Vec::new();
        }

        // Visible canvas region (the screen rectangle mapped back into canvas
        // space). Grid bounds are expanded by one cell so dots partially inside
        // the edges are not clipped away.
        let top_left = self.screen_to_canvas(Point::from((0.0, 0.0)));
        let bottom_right = self.screen_to_canvas(Point::from((
            f64::from(self.output_size.w),
            f64::from(self.output_size.h),
        )));
        let first_x = (top_left.x / spacing).floor() as i64;
        let last_x = (bottom_right.x / spacing).ceil() as i64;
        let first_y = (top_left.y / spacing).floor() as i64;
        let last_y = (bottom_right.y / spacing).ceil() as i64;

        let dot_px = (f64::from(BACKGROUND_DOT_SIZE) * self.viewport_scale).round() as i32;
        let dot_px = dot_px.max(1);
        let half = dot_px / 2;
        // Light gray, matching the shell's neutral palette.
        let color = Color32F::new(0.78, 0.78, 0.80, 1.0);

        let mut positions = Vec::new();
        for gy in first_y..=last_y {
            for gx in first_x..=last_x {
                let canvas = Point::from((gx as f64 * spacing, gy as f64 * spacing));
                let screen = self.canvas_to_screen(canvas);
                positions.push(Point::<i32, Physical>::from((
                    screen.x.round() as i32 - half,
                    screen.y.round() as i32 - half,
                )));
            }
        }

        while self.background_dot_ids.len() < positions.len() {
            self.background_dot_ids.push(Id::new());
        }

        positions
            .into_iter()
            .enumerate()
            .map(|(slot, location)| {
                SolidColorRenderElement::new(
                    self.background_dot_ids[slot].clone(),
                    Rectangle::new(location, Size::from((dot_px, dot_px))),
                    CommitCounter::default(),
                    color,
                    Kind::Unspecified,
                )
            })
            .collect()
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
            ManagedWindowKind::ShellBar | ManagedWindowKind::Launcher => 1.0,
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
