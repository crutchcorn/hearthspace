//! Masonry-rendered text for compositor window title bars.
//!
//! Second stage of the Xilem/Masonry integration spike. The compositor normally
//! draws title-bar chrome with primitive solid-color rectangles (see
//! [`super::rendering`]). Here we instead ask **Masonry** to lay out and
//! rasterize a real text label using its CPU `vello_cpu` backend, then hand the
//! resulting pixels to the GLES renderer as an ordinary memory render element.
//!
//! This proves a Masonry render pipeline can produce content the compositor
//! composites directly, without pulling a GPU/wgpu stack into the render loop:
//! `vello_cpu` is pure CPU, so the only cost is a one-time rasterization that we
//! cache per window in [`TitleTextBuffer`].

use masonry::{
    core::{PropertySet, Widget},
    parley::StyleProperty,
    peniko::Color,
    properties::ContentColor,
    testing::{TestHarness, TestHarnessParams},
    theme::default_property_set,
    widgets::Label,
};
use smithay::{
    backend::{
        allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer,
    },
    utils::Transform,
};

/// Native (unscaled) height of the rasterized title-text image, in pixels.
pub(super) const TITLE_TEXT_HEIGHT: i32 = 22;
/// Native (unscaled) width of the rasterized title-text image, in pixels.
pub(super) const TITLE_TEXT_WIDTH: i32 = 240;
const TITLE_TEXT_FONT_SIZE: f32 = 15.0;

/// A Masonry-rendered title-text image cached on a window.
///
/// The rasterized [`MemoryRenderBuffer`] is reused across frames and only
/// rebuilt when the cache key (`title`, `active`) changes, since re-running
/// Masonry's layout/raster every frame would be wasteful.
pub(super) struct TitleTextBuffer {
    pub(super) title: String,
    pub(super) active: bool,
    pub(super) buffer: MemoryRenderBuffer,
}

/// Rasterize `title` with Masonry and wrap the pixels in a [`MemoryRenderBuffer`].
///
/// The image is rendered opaque over `background` so it can be composited
/// straight on top of the matching solid title bar with no alpha blending
/// (and therefore no premultiplied-alpha concerns) for this spike.
pub(super) fn render_title_text(title: &str, background: Color, text: Color) -> MemoryRenderBuffer {
    let image = rasterize_title_text(title, background, text);
    let width = image.width() as i32;
    let height = image.height() as i32;
    let data = image.into_raw();

    // `image::RgbaImage` stores bytes in R, G, B, A order, which matches the
    // little-endian `Abgr8888` DRM fourcc the GLES renderer can import.
    MemoryRenderBuffer::from_slice(
        &data,
        Fourcc::Abgr8888,
        (width, height),
        1,
        Transform::Normal,
        None,
    )
}

/// Run Masonry's layout + CPU raster for `title`, returning the raw RGBA image.
///
/// Split out from [`render_title_text`] so the Masonry rendering path can be
/// validated without depending on the GLES renderer.
fn rasterize_title_text(title: &str, background: Color, text: Color) -> image::RgbaImage {
    let label = Label::new(title.to_string())
        .with_style(StyleProperty::FontSize(TITLE_TEXT_FONT_SIZE))
        .prepare()
        .with_props(PropertySet::new().with(ContentColor::new(text)));

    let params = TestHarnessParams::default()
        .with_size((TITLE_TEXT_WIDTH as u32, TITLE_TEXT_HEIGHT as u32))
        .with_background(background);
    let mut harness = TestHarness::create_with(default_property_set(), label, params);

    harness.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masonry_rasterizes_visible_title_text() {
        let background = Color::from_rgb8(38, 46, 61);
        let text = Color::from_rgb8(234, 242, 255);
        let image = rasterize_title_text("Hello", background, text);

        assert_eq!(image.width() as i32, TITLE_TEXT_WIDTH);
        assert_eq!(image.height() as i32, TITLE_TEXT_HEIGHT);

        let [bg_r, bg_g, bg_b, _] = background.to_rgba8().to_u8_array();
        let text_pixels = image
            .pixels()
            .filter(|px| px.0[0] != bg_r || px.0[1] != bg_g || px.0[2] != bg_b)
            .count();

        // Real glyphs must have been drawn: a non-trivial number of pixels
        // differ from the solid background fill.
        assert!(
            text_pixels > 20,
            "expected Masonry to draw glyph pixels, found {text_pixels} non-background pixels"
        );
    }
}
