//! Cached native foreground for server decorations.
//!
//! Masonry shapes text and shared SVG glyphs only when their content changes.
//! Smithay places/crops independent buffers during resize; the GPU owns the
//! header material, center grip, divider, rounded mask, and outer frame.
use std::sync::Arc;

use masonry::{
    core::Widget,
    kurbo::Affine,
    testing::{TestHarness, TestHarnessParams},
    theme::default_property_set,
    widgets::SizedBox,
};
use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::Transform,
};
use xilem::Color;

use super::windows::TitlebarButton;
use crate::config::TITLE_BAR_HEIGHT;

mod fonts;
pub(super) mod icons;
mod paint;
#[cfg(test)]
mod tests;
mod widget;

const MAX_TITLE_WIDTH: f64 = 2048.0;
const ACTIONS_WIDTH: i32 = 128;

/// Content and interaction state only: width and height never invalidate pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TitlebarState {
    pub appearance: hearthspace_ipc::Appearance,
    pub title: String,
    pub app_id: Option<String>,
    pub icon: Option<Arc<hearthspace_desktop::artwork::RasterImage>>,
    pub active: bool,
    pub maximized: bool,
    pub hovered: Option<TitlebarButton>,
    pub pressed: Option<TitlebarButton>,
    pub photo: bool,
}

impl TitlebarState {
    fn palette(&self) -> hearthspace_ui_components::Palette {
        hearthspace_ui_components::Palette::new(self.appearance).with_photo(self.photo)
    }
}

pub(super) struct TitlebarBuffer {
    state: TitlebarState,
    raster_scale: i32,
    title: Arc<Raster>,
    ellipsis: Arc<Raster>,
    app: Arc<Raster>,
    actions: Arc<Raster>,
}

pub(super) struct Raster {
    pub buffer: MemoryRenderBuffer,
    width: f64,
    natural_width: f64,
    #[cfg(test)]
    pixels: image::RgbaImage,
}

pub(super) struct Placement<'a> {
    pub raster: &'a Raster,
    pub x: f64,
    pub width: f64,
}

impl TitlebarBuffer {
    pub fn new(state: TitlebarState, raster_scale: i32) -> Self {
        let raster_scale = raster_scale.max(1);
        let text = text_color(&state);
        Self {
            title: rasterize(
                widget::Strip::text(state.title.clone(), MAX_TITLE_WIDTH, text),
                MAX_TITLE_WIDTH,
                raster_scale,
            ),
            ellipsis: rasterize(
                widget::Strip::text("…".into(), 32.0, text),
                32.0,
                raster_scale,
            ),
            app: rasterize(widget::Strip::app(&state), 44.0, raster_scale),
            actions: rasterize(
                widget::Strip::actions(&state),
                f64::from(ACTIONS_WIDTH),
                raster_scale,
            ),
            state,
            raster_scale,
        }
    }

    pub fn update(&mut self, state: TitlebarState, raster_scale: i32) {
        if self.raster_scale != raster_scale.max(1) {
            *self = Self::new(state, raster_scale);
            return;
        }
        if self.state == state {
            return;
        }
        let text_changed = text_color(&self.state) != text_color(&state);
        if self.state.title != state.title || text_changed {
            self.title = rasterize(
                widget::Strip::text(state.title.clone(), MAX_TITLE_WIDTH, text_color(&state)),
                MAX_TITLE_WIDTH,
                self.raster_scale,
            );
        }
        if text_changed {
            self.ellipsis = rasterize(
                widget::Strip::text("…".into(), 32.0, text_color(&state)),
                32.0,
                self.raster_scale,
            );
        }
        let palette_changed = self.state.appearance != state.appearance;
        if self.state.icon != state.icon
            || self.state.app_id != state.app_id
            || self.state.active != state.active
            || text_changed
            || palette_changed
        {
            self.app = rasterize(widget::Strip::app(&state), 44.0, self.raster_scale);
        }
        if self.state.maximized != state.maximized
            || self.state.hovered != state.hovered
            || self.state.pressed != state.pressed
            || text_changed
            || palette_changed
        {
            self.actions = rasterize(
                widget::Strip::actions(&state),
                f64::from(ACTIONS_WIDTH),
                self.raster_scale,
            );
        }
        self.state = state;
    }

    pub fn placements(&self, width: f64) -> Vec<Placement<'_>> {
        let budget = (width / 2.0 - 78.0).clamp(0.0, MAX_TITLE_WIDTH);
        let truncated = self.title.natural_width > budget;
        let suffix = if truncated {
            self.ellipsis.natural_width.min(budget)
        } else {
            0.0
        };
        let visible = budget - suffix;
        let mut parts = vec![Placement {
            raster: &self.app,
            x: 0.0,
            width: self.app.width,
        }];
        // Whole logical pixels keep the source and destination at identical
        // density. Cropping must never stretch every glyph to fit a fraction.
        let title_width = visible.floor().min(self.title.width);
        if title_width > 0.0 && self.title.natural_width > 0.0 {
            parts.push(Placement {
                raster: &self.title,
                x: 44.0,
                width: title_width,
            });
        }
        if suffix > 0.0 {
            parts.push(Placement {
                raster: &self.ellipsis,
                x: 44.0 + visible,
                width: suffix.ceil().min(self.ellipsis.width),
            });
        }
        parts.push(Placement {
            raster: &self.actions,
            x: width - f64::from(ACTIONS_WIDTH),
            width: f64::from(ACTIONS_WIDTH),
        });
        parts
    }
}

fn text_color(state: &TitlebarState) -> Color {
    if state.active {
        state.palette().text
    } else {
        state.palette().muted
    }
}

fn rasterize(strip: widget::Strip, width: f64, scale: i32) -> Arc<Raster> {
    let params = TestHarnessParams::default()
        .with_size((
            (width * f64::from(scale)) as u32,
            (TITLE_BAR_HEIGHT * scale) as u32,
        ))
        .with_scale(f64::from(scale))
        .with_background(Color::TRANSPARENT);
    // The harness strips its base layer transform. Scale the vector child
    // beneath an untransformed root, preserving native high-density glyphs.
    let strip = strip
        .prepare()
        .with_transform(Affine::scale(f64::from(scale)));
    let id = strip.id();
    let mut harness = TestHarness::create_with(
        default_property_set(),
        SizedBox::new(strip).prepare(),
        params,
    );
    let natural_width = harness
        .get_widget_with_id(id)
        .downcast::<widget::Strip>()
        .unwrap()
        .inner()
        .natural_width;
    // Keep a small ink overhang after the advance width (for example a final
    // italic fallback glyph); placement still clips at the available text box.
    let width = (natural_width.ceil() + 2.0).clamp(1.0, width);
    let image = harness.render();
    // Retain only the measured title/ellipsis width; the limit bounds hostile
    // client titles without allocating a bitmap proportional to their length.
    let mut image = image::imageops::crop_imm(
        &image,
        0,
        0,
        (width * f64::from(scale)) as u32,
        image.height(),
    )
    .to_image();
    harness.edit_root_widget(|mut root| fonts::preserve(root.ctx.text_contexts().0));
    for pixel in image.pixels_mut() {
        let alpha = u16::from(pixel.0[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let buffer = MemoryRenderBuffer::from_slice(
        image.as_raw(),
        Fourcc::Abgr8888,
        (image.width() as i32, image.height() as i32),
        scale,
        Transform::Normal,
        None,
    );
    Arc::new(Raster {
        buffer,
        width,
        natural_width,
        #[cfg(test)]
        pixels: image,
    })
}
