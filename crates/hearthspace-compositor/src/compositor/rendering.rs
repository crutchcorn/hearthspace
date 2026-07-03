use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Color32F, ContextId, ImportMem, Renderer, RendererSuper,
            damage::OutputDamageTracker,
            element::{
                Id, Kind,
                memory::MemoryRenderBufferRenderElement,
                render_elements,
                surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
                texture::TextureRenderElement,
                utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
            },
            gles::GlesRenderer,
        },
    },
    desktop::PopupManager,
    utils::{Logical, Physical, Point, Rectangle, Size, Transform},
    wayland::{
        compositor::{SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward},
        shell::xdg::SurfaceCachedState,
    },
};
use wayland_server::protocol::wl_surface;

use super::{App, ManagedWindowKind, masonry_titlebar, windows::toplevel_title};
use crate::config::{
    DEFAULT_BACKGROUND_MOVES_WITH_CANVAS, DEFAULT_BACKGROUND_PATTERN_MARK_SIZE,
    DEFAULT_BACKGROUND_PATTERN_SPACING,
};

type GlesTexture = <GlesRenderer as RendererSuper>::TextureId;

const DEFAULT_BACKGROUND_TILE_WIDTH: i32 = 1_920;
const DEFAULT_BACKGROUND_TILE_HEIGHT: i32 = 1_080;
const MAX_BACKGROUND_TILE_ELEMENTS: usize = 512;

pub(super) struct BackgroundImageTile {
    context_id: ContextId<GlesTexture>,
    texture: GlesTexture,
    size: Size<i32, Logical>,
}

struct BackgroundImageData {
    pixels: Vec<u8>,
    format: Fourcc,
    size: Size<i32, Logical>,
}

render_elements! {
    pub(super) HearthspaceRenderElement<=GlesRenderer>;
    Surface = RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Memory = MemoryRenderBufferRenderElement<GlesRenderer>,
    Texture = TextureRenderElement<GlesTexture>,
}

render_elements! {
    pub(super) TranslatedHearthspaceRenderElement<=GlesRenderer>;
    Translated = RelocateRenderElement<HearthspaceRenderElement>,
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
        )
    }

    pub(super) fn render_frame_at(
        &mut self,
        renderer: &mut GlesRenderer,
        framebuffer: &mut <GlesRenderer as RendererSuper>::Framebuffer<'_>,
        damage_tracker: &mut OutputDamageTracker,
        age: usize,
        output_rect: Rectangle<i32, Logical>,
    ) -> RenderFrameResult {
        let output_location = output_rect.loc;
        let output_offset = Point::<i32, Physical>::from((-output_location.x, -output_location.y));
        let elements = self.collect_render_elements(renderer, output_rect);
        let elements = elements
            .into_iter()
            .map(|element| {
                TranslatedHearthspaceRenderElement::from(RelocateRenderElement::from_element(
                    element,
                    output_offset,
                    Relocate::Relative,
                ))
            })
            .collect::<Vec<_>>();
        let result = damage_tracker.render_output(
            renderer,
            framebuffer,
            age,
            &elements,
            // White canvas; the repeated background image is drawn on top of it.
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
        output_rect: Rectangle<i32, Logical>,
    ) -> Vec<HearthspaceRenderElement> {
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
                ManagedWindowKind::ShellBar => self.shell_bar_screen_origins(),
                ManagedWindowKind::Launcher => self.launcher_screen_origins(),
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

        // The repeated background image sits behind every window, directly on
        // top of the white clear color, so it is appended last in this
        // front-to-back ordering.
        for tile in self.background_image_elements(renderer, output_rect) {
            elements.push(HearthspaceRenderElement::from(tile));
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

    /// Build repeated background image render elements for the current viewport.
    ///
    /// The tile is laid out in *canvas* space, so it pans and zooms with the
    /// viewport by default while keeping the render-element count proportional
    /// to visible image tiles, not to details inside the image. This is also the
    /// shape future user-defined repeating backgrounds can reuse.
    fn background_image_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output_rect: Rectangle<i32, Logical>,
    ) -> Vec<TextureRenderElement<GlesTexture>> {
        if self.viewport_scale <= 0.0 || !self.viewport_scale.is_finite() {
            return Vec::new();
        }
        let moves_with_canvas = DEFAULT_BACKGROUND_MOVES_WITH_CANVAS;

        let (context_id, texture, tile_size) = match self.background_image_tile(renderer) {
            Ok(tile) => (tile.context_id.clone(), tile.texture.clone(), tile.size),
            Err(_) => return Vec::new(),
        };

        let (min_x, max_x, min_y, max_y) =
            self.background_image_visible_bounds(output_rect, moves_with_canvas);

        let Some((first_x, last_x, x_count)) =
            background_tile_axis_bounds(min_x, max_x, f64::from(tile_size.w))
        else {
            return Vec::new();
        };
        let Some((first_y, last_y, y_count)) =
            background_tile_axis_bounds(min_y, max_y, f64::from(tile_size.h))
        else {
            return Vec::new();
        };
        let tile_count = x_count.saturating_mul(y_count);
        if tile_count > MAX_BACKGROUND_TILE_ELEMENTS {
            return Vec::new();
        }

        while self.background_tile_ids.len() < tile_count {
            self.background_tile_ids.push(Id::new());
        }

        let mut elements = Vec::with_capacity(tile_count);
        let src = Rectangle::from_size(tile_size.to_f64());
        for gy in first_y..=last_y {
            for gx in first_x..=last_x {
                let tile_origin = Point::from((
                    gx as f64 * f64::from(tile_size.w),
                    gy as f64 * f64::from(tile_size.h),
                ));
                let next_tile_origin = Point::from((
                    (gx + 1) as f64 * f64::from(tile_size.w),
                    (gy + 1) as f64 * f64::from(tile_size.h),
                ));
                let (screen, next_screen) = if moves_with_canvas {
                    (
                        self.canvas_to_screen(tile_origin),
                        self.canvas_to_screen(next_tile_origin),
                    )
                } else {
                    (tile_origin, next_tile_origin)
                };
                let left = screen.x.round() as i32;
                let top = screen.y.round() as i32;
                let right = next_screen.x.round() as i32;
                let bottom = next_screen.y.round() as i32;
                let location = Point::<f64, Physical>::from((f64::from(left), f64::from(top)));
                let size = Size::from(((right - left).max(1), (bottom - top).max(1)));
                let slot = elements.len();
                elements.push(TextureRenderElement::from_static_texture(
                    self.background_tile_ids[slot].clone(),
                    context_id.clone(),
                    location,
                    texture.clone(),
                    1,
                    Transform::Normal,
                    None,
                    Some(src),
                    Some(size),
                    None,
                    Kind::Unspecified,
                ));
            }
        }
        elements
    }

    fn background_image_tile(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Result<&BackgroundImageTile, <GlesRenderer as RendererSuper>::Error> {
        let context_id = renderer.context_id();
        let needs_rebuild = self
            .background_image_tile
            .as_ref()
            .map(|tile| tile.context_id != context_id)
            .unwrap_or(true);

        if needs_rebuild {
            let data = default_background_image_data();
            let texture = renderer.import_memory(
                &data.pixels,
                data.format,
                (data.size.w, data.size.h).into(),
                false,
            )?;
            self.background_image_tile = Some(BackgroundImageTile {
                context_id,
                texture,
                size: data.size,
            });
        }

        Ok(self.background_image_tile.as_ref().unwrap())
    }

    fn background_image_visible_bounds(
        &self,
        output_rect: Rectangle<i32, Logical>,
        moves_with_canvas: bool,
    ) -> (f64, f64, f64, f64) {
        let top_left = Point::from((f64::from(output_rect.loc.x), f64::from(output_rect.loc.y)));
        let bottom_right = Point::from((
            f64::from(output_rect.loc.x) + f64::from(output_rect.size.w),
            f64::from(output_rect.loc.y) + f64::from(output_rect.size.h),
        ));

        let (top_left, bottom_right) = if moves_with_canvas {
            (
                self.screen_to_canvas(top_left),
                self.screen_to_canvas(bottom_right),
            )
        } else {
            (top_left, bottom_right)
        };

        (
            top_left.x.min(bottom_right.x),
            top_left.x.max(bottom_right.x),
            top_left.y.min(bottom_right.y),
            top_left.y.max(bottom_right.y),
        )
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

fn background_tile_axis_bounds(min: f64, max: f64, tile_size: f64) -> Option<(i64, i64, usize)> {
    if !min.is_finite() || !max.is_finite() || tile_size <= 0.0 || !tile_size.is_finite() {
        return None;
    }

    let first = (min / tile_size).floor() as i64;
    let last = (max / tile_size).ceil() as i64 - 1;
    let count = usize::try_from(last.checked_sub(first)?.checked_add(1)?).ok()?;
    Some((first, last, count))
}

fn default_background_image_data() -> BackgroundImageData {
    let size = Size::<i32, Logical>::from((
        DEFAULT_BACKGROUND_TILE_WIDTH,
        DEFAULT_BACKGROUND_TILE_HEIGHT,
    ));
    BackgroundImageData {
        pixels: rasterize_default_background_image(size),
        format: Fourcc::Abgr8888,
        size,
    }
}

fn rasterize_default_background_image(size: Size<i32, Logical>) -> Vec<u8> {
    let len = size
        .w
        .checked_mul(size.h)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .unwrap_or_default();
    let mut pixels = vec![0; len];
    let spacing = DEFAULT_BACKGROUND_PATTERN_SPACING.max(1) as usize;
    let mark_size = DEFAULT_BACKGROUND_PATTERN_MARK_SIZE.max(1);

    for cy in (0..=size.h).step_by(spacing) {
        for cx in (0..=size.w).step_by(spacing) {
            rasterize_default_background_mark(&mut pixels, size, cx, cy, mark_size);
        }
    }

    pixels
}

fn rasterize_default_background_mark(
    pixels: &mut [u8],
    size: Size<i32, Logical>,
    center_x: i32,
    center_y: i32,
    mark_size: i32,
) {
    let half = mark_size / 2;
    let x0 = (center_x - half).max(0);
    let y0 = (center_y - half).max(0);
    let x1 = (center_x - half + mark_size).min(size.w);
    let y1 = (center_y - half + mark_size).min(size.h);

    for y in y0..y1 {
        for x in x0..x1 {
            let Some(offset) = y
                .checked_mul(size.w)
                .and_then(|row| row.checked_add(x))
                .and_then(|pixel| pixel.checked_mul(4))
                .and_then(|byte| usize::try_from(byte).ok())
            else {
                continue;
            };
            if offset + 3 >= pixels.len() {
                continue;
            }
            pixels[offset] = 199;
            pixels[offset + 1] = 199;
            pixels[offset + 2] = 204;
            pixels[offset + 3] = 255;
        }
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

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle, Size};

    use super::{
        background_tile_axis_bounds, point_in_logical_rect, rasterize_default_background_image,
    };

    #[test]
    fn point_in_logical_rect_excludes_other_outputs() {
        let rect = Rectangle::<i32, Logical>::new((1280, 0).into(), (1024, 768).into());

        assert!(point_in_logical_rect(Point::from((1280.0, 12.0)), rect));
        assert!(point_in_logical_rect(Point::from((2303.0, 767.0)), rect));
        assert!(!point_in_logical_rect(Point::from((1279.0, 12.0)), rect));
        assert!(!point_in_logical_rect(Point::from((2304.0, 12.0)), rect));
    }

    #[test]
    fn background_tile_axis_bounds_include_partially_visible_tiles() {
        assert_eq!(
            background_tile_axis_bounds(0.0, 240.0, 120.0),
            Some((0, 1, 2))
        );
        assert_eq!(
            background_tile_axis_bounds(-240.0, 0.0, 120.0),
            Some((-2, -1, 2))
        );
        assert_eq!(
            background_tile_axis_bounds(10.0, 240.1, 120.0),
            Some((0, 2, 3))
        );
    }

    #[test]
    fn default_background_image_rasterizes_tile_edges() {
        let size = Size::<i32, Logical>::from((240, 120));
        let pixels = rasterize_default_background_image(size);

        assert_eq!(pixel_alpha(&pixels, size, 0, 0), 255);
        assert_eq!(pixel_alpha(&pixels, size, 239, 0), 255);
        assert_eq!(pixel_alpha(&pixels, size, 60, 60), 0);
    }

    fn pixel_alpha(pixels: &[u8], size: Size<i32, Logical>, x: i32, y: i32) -> u8 {
        let offset = ((y * size.w + x) * 4 + 3) as usize;
        pixels[offset]
    }
}
