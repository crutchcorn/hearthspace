//! Masonry-rendered window title-bar chrome.
//!
//! The compositor draws server-side window decorations — the draggable title
//! bar, its title text, and the close button — by asking **Masonry** to lay out
//! and rasterize the whole bar with its CPU `vello_cpu` backend, then handing
//! the resulting pixels to the GLES renderer as a memory render element (see
//! [`super::rendering`]).
//!
//! Rendering the chrome as a real widget tree — a gradient-filled bar and a
//! padded title [`Label`] — replaces the earlier hand-drawn solid rectangles,
//! while the close button (a red circle with a white X) is drawn directly onto
//! the rasterized bar so the circle and X stay concentric and crisp without
//! depending on a font shipping a “✕” glyph. `vello_cpu` is pure CPU, so the
//! only cost is a rasterization that we cache per window in [`TitlebarBuffer`]
//! and only repeat when the bar width, title, or active state changes.

use masonry::{
    core::{PropertySet, Widget},
    layout::AsUnit,
    parley::StyleProperty,
    peniko::Color,
    properties::{
        Background, ContentColor,
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

const TITLE_FONT_SIZE: f32 = 14.0;
/// Inset before the title text, in native (unscaled) pixels.
const TITLE_LEFT_PAD: i32 = 10;
/// Gap between the title text and the close X, in native pixels.
const TITLE_RIGHT_GAP: i32 = 8;
/// Radius of the rounded top corners of the title bar, in native pixels.
const TITLE_BAR_CORNER_RADIUS: f32 = 9.0;

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
/// transparent with premultiplied edges so the gradient bar and the close
/// button (circle + X) composite correctly against whatever sits behind the
/// window chrome.
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
            Color::from_rgb8(60, 96, 156),
            Color::from_rgb8(40, 70, 120),
            Color::from_rgb8(236, 244, 255),
        )
    } else {
        (
            Color::from_rgb8(52, 60, 74),
            Color::from_rgb8(38, 45, 58),
            Color::from_rgb8(196, 205, 220),
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
        .with_fixed_spacer(CLOSE_BUTTON_SIZE.px())
        .with_fixed_spacer(CLOSE_BUTTON_MARGIN.px())
        .prepare();

    let mut bar_props = PropertySet::new();
    bar_props.insert(Background::Gradient(
        Gradient::new_linear(std::f64::consts::FRAC_PI_2).with_stops([bar_top, bar_bottom]),
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
    draw_close_button(&mut image, width);
    round_top_corners(&mut image, width);
    image
}

/// Draw the round red close button and its white X directly onto the rasterized
/// bar, occupying the same rect the compositor hit-tests for closing the window
/// (right-aligned, inset by [`CLOSE_BUTTON_MARGIN`], vertically centered,
/// [`CLOSE_BUTTON_SIZE`] square).
///
/// Both the filled circle and the X are drawn here (rather than as Masonry
/// widgets) so they stay perfectly concentric and the X stays crisp without
/// depending on a particular font shipping a “✕” glyph.
fn draw_close_button(image: &mut image::RgbaImage, width: i32) {
    let rect_x = (width - CLOSE_BUTTON_MARGIN - CLOSE_BUTTON_SIZE).max(0);
    let rect_y = ((TITLE_BAR_HEIGHT - CLOSE_BUTTON_SIZE) / 2).max(0);
    let size = CLOSE_BUTTON_SIZE as f32;

    let max_x = (rect_x + CLOSE_BUTTON_SIZE).min(width);
    let max_y = (rect_y + CLOSE_BUTTON_SIZE).min(TITLE_BAR_HEIGHT);

    // Filled red circle inscribed in the hit rect.
    let center_x = rect_x as f32 + size / 2.0;
    let center_y = rect_y as f32 + size / 2.0;
    let radius = size / 2.0;
    let [br, bg, bb, _] = Color::from_rgb8(202, 62, 66).to_rgba8().to_u8_array();
    for py in rect_y..max_y {
        for px in rect_x..max_x {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let dist = ((fx - center_x).powi(2) + (fy - center_y).powi(2)).sqrt();
            let coverage = (radius + 0.5 - dist).clamp(0.0, 1.0);
            if coverage > 0.0 {
                blend_pixel(image, px, py, br, bg, bb, coverage);
            }
        }
    }

    // White X stroked on top, inset from the circle edge so it stays contained.
    let inset = size * 0.36;
    let half_thickness = (size * 0.08).max(0.9);
    let x0 = rect_x as f32 + inset;
    let y0 = rect_y as f32 + inset;
    let x1 = (rect_x + CLOSE_BUTTON_SIZE) as f32 - inset;
    let y1 = (rect_y + CLOSE_BUTTON_SIZE) as f32 - inset;
    let [xr, xg, xb, _] = Color::from_rgb8(255, 240, 240).to_rgba8().to_u8_array();
    for py in rect_y..max_y {
        for px in rect_x..max_x {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let dist = distance_to_segment(fx, fy, x0, y0, x1, y1)
                .min(distance_to_segment(fx, fy, x0, y1, x1, y0));
            let coverage = (half_thickness + 0.5 - dist).clamp(0.0, 1.0);
            if coverage > 0.0 {
                blend_pixel(image, px, py, xr, xg, xb, coverage);
            }
        }
    }
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

/// Round off the top-left and top-right corners of the rasterized bar with an
/// anti-aliased quarter-circle mask of radius [`TITLE_BAR_CORNER_RADIUS`].
///
/// Corner pixels outside the rounded shape become fully transparent; edge pixels
/// are premultiplied (color × coverage, alpha = coverage) so the bar blends
/// cleanly over whatever is behind the window chrome. Only the top corners are
/// rounded; the bottom edge stays square where it meets the window body.
fn round_top_corners(image: &mut image::RgbaImage, width: i32) {
    let radius = TITLE_BAR_CORNER_RADIUS;
    if radius <= 0.0 {
        return;
    }
    let span = (radius.ceil() as i32).min(TITLE_BAR_HEIGHT).min(width);

    // (arc center x, first column) for the top-left and top-right corner boxes.
    // Every pixel in each box lies in that corner's outer quadrant, so it is
    // enough to fade by distance from the arc center at height `radius`.
    let corners = [(radius, 0), (width as f32 - radius, width - span)];
    for (center_x, start_x) in corners {
        for py in 0..span {
            for px in start_x..(start_x + span).min(width) {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;
                let dist = ((fx - center_x).powi(2) + (fy - radius).powi(2)).sqrt();
                let coverage = (radius + 0.5 - dist).clamp(0.0, 1.0);
                if coverage < 1.0 {
                    premultiply_pixel(image, px, py, coverage);
                }
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

/// Euclidean distance from point `(px, py)` to the segment `(ax, ay)-(bx, by)`.
fn distance_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masonry_strokes_a_close_x_onto_the_bar() {
        let width = 200;
        let image = rasterize_titlebar(width, "Hello", true);

        assert_eq!(image.width() as i32, width);
        assert_eq!(image.height() as i32, TITLE_BAR_HEIGHT);

        // The close X is stroked in the light title color over the darker bar
        // gradient, within the right-aligned hit rect. Counting near-white
        // pixels there proves the X (and the wider widget tree) rasterized.
        let x_start = (width - CLOSE_BUTTON_MARGIN - CLOSE_BUTTON_SIZE) as u32;
        let x_end = (width - CLOSE_BUTTON_MARGIN) as u32;
        let y_start = ((TITLE_BAR_HEIGHT - CLOSE_BUTTON_SIZE) / 2) as u32;
        let y_end = y_start + CLOSE_BUTTON_SIZE as u32;

        let mut bright = 0;
        for y in y_start..y_end {
            for x in x_start..x_end {
                let px = image.get_pixel(x, y);
                if px.0[0] > 200 && px.0[1] > 200 && px.0[2] > 200 {
                    bright += 1;
                }
            }
        }
        assert!(
            bright > 8,
            "expected a stroked close X, found {bright} bright pixels"
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
}
