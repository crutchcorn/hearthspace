//! Masonry-rendered window title-bar chrome.
//!
//! The compositor draws server-side window decorations — the draggable title
//! bar, its title text, and the close button — by asking **Masonry** to lay out
//! and rasterize the whole bar with its CPU `vello_cpu` backend, then handing
//! the resulting pixels to the GLES renderer as a memory render element (see
//! [`super::rendering`]).
//!
//! The shared light materials and SVG close glyph are rendered by Masonry's
//! CPU backend. Pixels are cached per window and rebuilt only when the width,
//! title, or active state changes. Input and the close hit target stay owned by
//! the compositor.

use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};

use masonry::{
    core::{PropertySet, Widget},
    layout::AsUnit,
    parley::StyleProperty,
    properties::{
        Background, ContentColor, Gap,
        types::{CrossAxisAlignment, Gradient},
    },
    testing::{TestHarness, TestHarnessParams},
    theme::default_property_set,
    widgets::{Flex, Label, SizedBox},
};
use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::Transform,
};

use crate::config::{CLOSE_BUTTON_MARGIN, CLOSE_BUTTON_SIZE, TITLE_BAR_HEIGHT};

const TITLE_FONT_SIZE: f32 = 13.0;
/// Inset before the title text, in native (unscaled) pixels.
const TITLE_LEFT_PAD: i32 = 10;
/// Gap between the title text and the close X, in native pixels.
const TITLE_RIGHT_GAP: i32 = 8;
/// Radius of the rounded top corners of the title bar, in native pixels.
const TITLE_BAR_CORNER_RADIUS: f32 = components::RADIUS_PANEL as f32;

/// A Masonry-rendered title-bar image cached on a window.
///
/// The rasterized [`MemoryRenderBuffer`] is reused across frames and only
/// rebuilt when the cache key (`width`, `title`, `active`) changes, since
/// re-running Masonry's layout/raster every frame would be wasteful.
pub(super) struct TitlebarBuffer {
    pub(super) width: i32,
    pub(super) title: String,
    pub(super) active: bool,
    pub(super) buffer: MemoryRenderBuffer,
}

/// Rasterize the full title bar (`width` × [`TITLE_BAR_HEIGHT`]) with Masonry and
/// wrap the pixels in a [`MemoryRenderBuffer`].
///
/// The bar is opaque apart from its rounded top corners, which are masked to
/// transparent with premultiplied edges so the warm bar and close glyph
/// composite correctly against whatever sits behind the window chrome.
pub(super) fn render_titlebar(width: i32, title: &str, active: bool) -> MemoryRenderBuffer {
    let image = rasterize_titlebar(width, title, active);
    let img_w = image.width() as i32;
    let img_h = image.height() as i32;
    let data = image.into_raw();

    // `image::RgbaImage` stores bytes in R, G, B, A order, which matches the
    // little-endian `Abgr8888` DRM fourcc the GLES renderer can import.
    MemoryRenderBuffer::from_slice(
        &data,
        Fourcc::Abgr8888,
        (img_w, img_h),
        1,
        Transform::Normal,
        None,
    )
}

/// Run Masonry's layout + CPU raster for the whole title bar, returning the raw
/// RGBA image. Split out from [`render_titlebar`] so the Masonry path can be
/// validated without depending on the GLES renderer.
fn rasterize_titlebar(width: i32, title: &str, active: bool) -> image::RgbaImage {
    let (bar_top, bar_bottom, text_color) = if active {
        (
            components::SURFACE_RAISED,
            components::SURFACE,
            components::TEXT,
        )
    } else {
        (
            components::SURFACE,
            components::SURFACE,
            components::TEXT_MUTED,
        )
    };

    let title_label = Label::new(title.to_string())
        .with_style(StyleProperty::FontSize(TITLE_FONT_SIZE))
        .prepare()
        .with_props(PropertySet::new().with(ContentColor::new(text_color)));

    let row = Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .with_fixed_spacer(TITLE_LEFT_PAD.px())
        .with(title_label, 1.0)
        .with_fixed_spacer(TITLE_RIGHT_GAP.px())
        .with(
            SizedBox::new(
                icon(Icon::X, f64::from(CLOSE_BUTTON_SIZE))
                    .color(text_color)
                    .into_widget()
                    .prepare(),
            )
            .width(CLOSE_BUTTON_SIZE.px())
            .height(CLOSE_BUTTON_SIZE.px())
            .prepare(),
            0.0,
        )
        .with_fixed_spacer(CLOSE_BUTTON_MARGIN.px())
        .prepare()
        // Explicit spacers define the compositor's close hit target. Masonry's
        // default inter-child gap would shift the glyph ten pixels left.
        .with_props(Gap::ZERO);

    let mut bar_props = PropertySet::new();
    bar_props.insert(Background::Gradient(
        // Angle π (180°) puts the first stop at the top and runs straight down,
        // per Masonry's CSS-style gradient convention.
        Gradient::new_linear(std::f64::consts::PI).with_stops([bar_top, bar_bottom]),
    ));

    let bar = SizedBox::new(row)
        .width(width.px())
        .height(TITLE_BAR_HEIGHT.px())
        .prepare()
        .with_props(bar_props);

    let params = TestHarnessParams::default()
        .with_size((width as u32, TITLE_BAR_HEIGHT as u32))
        .with_background(bar_bottom);
    let mut harness = TestHarness::create_with(default_property_set(), bar, params);

    let mut image = harness.render();
    finish_top_edge(&mut image, width, active);
    image
}

/// Alpha-blend an opaque `(r, g, b)` color into one image pixel by `coverage`.
fn blend_pixel(image: &mut image::RgbaImage, px: i32, py: i32, r: u8, g: u8, b: u8, coverage: f32) {
    let pixel = image.get_pixel_mut(px as u32, py as u32);
    pixel.0[0] = blend_channel(pixel.0[0], r, coverage);
    pixel.0[1] = blend_channel(pixel.0[1], g, coverage);
    pixel.0[2] = blend_channel(pixel.0[2], b, coverage);
    pixel.0[3] = 255;
}

/// Alpha-blend a single 8-bit channel of `foreground` over `background`.
fn blend_channel(background: u8, foreground: u8, coverage: f32) -> u8 {
    let blended = f32::from(background) * (1.0 - coverage) + f32::from(foreground) * coverage;
    blended.round().clamp(0.0, 255.0) as u8
}

/// Add the active rim and clip only the rounded top corners. The bottom stays
/// square where the title bar meets the client's body. Keeping this within the
/// existing buffer preserves both the draggable area and the close hit target.
fn finish_top_edge(image: &mut image::RgbaImage, width: i32, active: bool) {
    let radius = TITLE_BAR_CORNER_RADIUS.min(width as f32 / 2.0);
    let color = if active {
        components::ACCENT
    } else {
        components::BORDER
    };
    let [r, g, b, alpha] = color.to_rgba8().to_u8_array();
    for py in 0..TITLE_BAR_HEIGHT {
        for px in 0..width {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let edge = if fy < radius && (fx < radius || fx > width as f32 - radius) {
                let center_x = if fx < radius {
                    radius
                } else {
                    width as f32 - radius
                };
                radius - ((fx - center_x).powi(2) + (fy - radius).powi(2)).sqrt()
            } else {
                fy.min(fx).min(width as f32 - fx)
            };
            let coverage = (edge + 0.5).clamp(0.0, 1.0);
            let rim = (1.5 - edge).clamp(0.0, 1.0) * f32::from(alpha) / 255.0;
            if rim > 0.0 {
                blend_pixel(image, px, py, r, g, b, rim);
            }
            if coverage < 1.0 {
                premultiply_pixel(image, px, py, coverage);
            }
        }
    }
}

/// Premultiply one image pixel by `coverage` and set its alpha to match, used to
/// fade out the rounded corner edges.
fn premultiply_pixel(image: &mut image::RgbaImage, px: i32, py: i32, coverage: f32) {
    let pixel = image.get_pixel_mut(px as u32, py as u32);
    pixel.0[0] = (f32::from(pixel.0[0]) * coverage).round() as u8;
    pixel.0[1] = (f32::from(pixel.0[1]) * coverage).round() as u8;
    pixel.0[2] = (f32::from(pixel.0[2]) * coverage).round() as u8;
    pixel.0[3] = (255.0 * coverage).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masonry_renders_shared_close_glyph_inside_the_existing_hit_target() {
        let width = 200;
        let image = rasterize_titlebar(width, "Hello", true);

        assert_eq!(image.width() as i32, width);
        assert_eq!(image.height() as i32, TITLE_BAR_HEIGHT);

        // The shared X is dark over the warm title bar and remains inside
        // the same compositor-owned close target.
        let x_start = (width - CLOSE_BUTTON_MARGIN - CLOSE_BUTTON_SIZE) as u32;
        let x_end = (width - CLOSE_BUTTON_MARGIN) as u32;
        let y_start = ((TITLE_BAR_HEIGHT - CLOSE_BUTTON_SIZE) / 2) as u32;
        let y_end = y_start + CLOSE_BUTTON_SIZE as u32;

        let mut dark = 0;
        for y in y_start..y_end {
            for x in x_start..x_end {
                let px = image.get_pixel(x, y);
                // Optical SVG strokes are antialiased at this 18px size;
                // count half-covered ink pixels as well as solid ones.
                if px.0[0] < 160 && px.0[1] < 160 && px.0[2] < 160 {
                    dark += 1;
                }
            }
        }
        assert!(
            dark > 8,
            "expected the shared close X, found {dark} dark pixels"
        );
    }

    #[test]
    fn masonry_rounds_the_top_corners() {
        let width = 200;
        let image = rasterize_titlebar(width, "Hello", true);

        // The extreme top corners fall outside the rounded shape and must be
        // fully transparent, while the bar's interior stays opaque.
        assert_eq!(image.get_pixel(0, 0).0[3], 0, "top-left corner not cut");
        assert_eq!(
            image.get_pixel(width as u32 - 1, 0).0[3],
            0,
            "top-right corner not cut"
        );
        assert_eq!(
            image
                .get_pixel(width as u32 / 2, TITLE_BAR_HEIGHT as u32 / 2)
                .0[3],
            255,
            "bar interior should be opaque"
        );
        // The bottom corners stay square (the bar meets the window body there).
        assert_eq!(
            image.get_pixel(0, TITLE_BAR_HEIGHT as u32 - 1).0[3],
            255,
            "bottom-left corner should be square"
        );
    }

    #[test]
    fn active_titlebar_has_a_warm_surface_and_accent_rim_without_changing_opacity() {
        let active = rasterize_titlebar(200, "", true);
        let inactive = rasterize_titlebar(200, "", false);
        assert_eq!(
            active.get_pixel(100, 0).0,
            components::ACCENT.to_rgba8().to_u8_array()
        );
        assert_ne!(active.get_pixel(100, 0), inactive.get_pixel(100, 0));
        for image in [active, inactive] {
            let interior = image.get_pixel(100, 15).0;
            assert!(interior[0] > 230 && interior[1] > 230 && interior[2] > 230);
            assert_eq!(interior[3], 255);
            for pixel in image.pixels() {
                assert!(
                    pixel.0[..3].iter().all(|channel| *channel <= pixel.0[3]),
                    "corner edges must remain premultiplied"
                );
            }
        }
    }
}
