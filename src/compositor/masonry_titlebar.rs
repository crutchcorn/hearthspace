//! Masonry-rendered window title-bar chrome.
//!
//! The compositor draws server-side window decorations — the draggable title
//! bar, its title text, and the close button — by asking **Masonry** to lay out
//! and rasterize the whole bar with its CPU `vello_cpu` backend, then handing
//! the resulting pixels to the GLES renderer as a memory render element (see
//! [`super::rendering`]).
//!
//! Rendering the chrome as a real widget tree — a gradient-filled bar, a padded
//! title [`Label`], and a rounded close button with a crisp ✕ glyph — replaces
//! the earlier hand-drawn solid rectangles and blocky X marks. `vello_cpu` is
//! pure CPU, so the only cost is a rasterization that we cache per window in
//! [`TitlebarBuffer`] and only repeat when the bar width, title, or active
//! state changes.

use masonry::{
    core::{NewWidget, PropertySet, Widget},
    layout::AsUnit,
    parley::StyleProperty,
    peniko::Color,
    properties::{
        Background, ContentColor, CornerRadius,
        types::{CrossAxisAlignment, Gradient, MainAxisAlignment},
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
const CLOSE_GLYPH_FONT_SIZE: f32 = 12.0;
/// Inset before the title text, in native (unscaled) pixels.
const TITLE_LEFT_PAD: i32 = 10;
/// Gap between the title text and the close button, in native pixels.
const TITLE_RIGHT_GAP: i32 = 8;

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
/// The bar is opaque — the gradient background fills the whole area and the
/// close button's rounded corners reveal that same background — so it composites
/// with no premultiplied-alpha concerns.
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
        .with_fixed(close_button_widget())
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

    harness.render()
}

/// Build the rounded red close button hosting a centered ✕ glyph.
fn close_button_widget() -> NewWidget<SizedBox> {
    let glyph = Label::new("\u{2715}".to_string())
        .with_style(StyleProperty::FontSize(CLOSE_GLYPH_FONT_SIZE))
        .prepare()
        .with_props(PropertySet::new().with(ContentColor::new(Color::from_rgb8(255, 240, 240))));

    let centered = Flex::row()
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .with_fixed(glyph)
        .prepare();

    let mut close_props = PropertySet::new();
    close_props.insert(Background::Color(Color::from_rgb8(202, 62, 66)));
    close_props.insert(CornerRadius::all((CLOSE_BUTTON_SIZE / 2).px()));

    SizedBox::new(centered)
        .width(CLOSE_BUTTON_SIZE.px())
        .height(CLOSE_BUTTON_SIZE.px())
        .prepare()
        .with_props(close_props)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masonry_renders_titlebar_with_close_button() {
        let width = 200;
        let image = rasterize_titlebar(width, "Hello", true);

        assert_eq!(image.width() as i32, width);
        assert_eq!(image.height() as i32, TITLE_BAR_HEIGHT);

        // The close button is a saturated red while the bar gradient is
        // blue/grey, so a run of strongly-red pixels proves the button (and
        // therefore the wider widget tree) rasterized.
        let red_pixels = image
            .pixels()
            .filter(|px| px.0[0] > 150 && px.0[1] < 120 && px.0[2] < 120)
            .count();
        assert!(
            red_pixels > 40,
            "expected a red close button, found {red_pixels} red pixels"
        );
    }
}
