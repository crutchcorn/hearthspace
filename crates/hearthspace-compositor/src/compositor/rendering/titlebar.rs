//! Cached, scale-aware server decorations. Geometry and input remain owned by
//! the compositor; Masonry only supplies the titlebar's premultiplied pixels.
use smithay::{
    backend::renderer::{
        element::{Kind, memory::MemoryRenderBufferRenderElement},
        gles::GlesRenderer,
    },
    utils::{Logical, Rectangle, Size},
};

use crate::compositor::{
    App, masonry_titlebar,
    windows::{window_app_id, window_title},
};

impl App {
    pub(super) fn titlebar_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        index: usize,
    ) -> Vec<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if !self.has_compositor_chrome(index) {
            return Vec::new();
        }
        let screen = self.title_bar_rect(index);
        if screen.size.w <= 0 || screen.size.h <= 0 {
            return Vec::new();
        }
        // Pinned geometry may differ from the canvas minimum or surface-tree
        // bounds while a client is still handling its configure. Match the
        // current frame and hit rectangles instead of stretching old controls.
        let pinned =
            self.window_uses_output_coordinates(index) && self.canvas_search_pose(index).is_none();
        let native = if pinned {
            screen.size
        } else {
            self.title_bar_canvas_rect(index).size
        };
        let width = native.w.max(1);
        let window = &self.windows[index];
        let (hovered, pressed) = self.titlebar_button_state(window.id);
        let state = masonry_titlebar::TitlebarState {
            appearance: self.appearance,
            title: window_title(&window.surface)
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| "Hearthspace".to_string()),
            app_id: window_app_id(&window.surface),
            active: !self.canvas_search_active()
                && self.focused_normal_window_id == Some(window.id),
            maximized: window.presentation.maximized,
            hovered,
            pressed,
            photo: self.background_image_path.is_some(),
        };
        // Rasterize for physical pixels instead of stretching a 1× text image
        // on high-density outputs. Zoom is still applied by the scene transform.
        // One density for all intersecting outputs prevents cache churn when a
        // window straddles monitors with different scales.
        let raster_scale = (f64::from(self.titlebar_output_scale(screen))
            * self.window_render_scale(index))
        .ceil()
        .clamp(1.0, 8.0) as i32;
        if let Some(cache) = &mut self.windows[index].titlebar {
            cache.update(state, raster_scale);
        } else {
            self.windows[index].titlebar =
                Some(masonry_titlebar::TitlebarBuffer::new(state, raster_scale));
        }
        let scale = f64::from(screen.size.w) / f64::from(width);
        self.windows[index]
            .titlebar
            .as_ref()
            .unwrap()
            .placements(f64::from(width))
            .into_iter()
            .filter_map(|part| {
                let source = Rectangle::from_size(Size::<f64, Logical>::from((
                    part.width,
                    f64::from(native.h),
                )));
                MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    (
                        f64::from(screen.loc.x) + part.x * scale,
                        f64::from(screen.loc.y),
                    ),
                    &part.raster.buffer,
                    Some(
                        self.canvas_search_pose(index)
                            .map_or(1., |(_, alpha)| alpha as f32),
                    ),
                    Some(source),
                    Some(Size::from((
                        (part.width * scale).round() as i32,
                        screen.size.h,
                    ))),
                    Kind::Unspecified,
                )
                .ok()
            })
            .collect()
    }
}
