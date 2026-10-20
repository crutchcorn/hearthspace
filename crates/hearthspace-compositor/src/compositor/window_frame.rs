//! Damage-tracked GPU chrome around the application geometry, not its shadows.
use std::collections::{HashMap, HashSet};

use smithay::{
    backend::renderer::{
        ContextId, Renderer,
        element::Kind,
        gles::{
            GlesError, GlesPixelProgram, GlesRenderer, GlesTexture, Uniform, UniformName,
            UniformType, element::PixelShaderElement,
        },
    },
    utils::{Logical, Rectangle},
};

mod clip;
#[cfg(test)]
mod tests;

pub(super) use clip::{RoundedElement, WindowClip, solid_color};

#[derive(Default)]
pub(super) struct WindowFrameCache {
    renderers: HashMap<ContextId<GlesTexture>, RendererFrames>,
}

struct RendererFrames {
    shaders: Option<Shaders>,
    windows: HashMap<u64, CachedFrame>,
}

struct Shaders {
    frame: GlesPixelProgram,
    clip: clip::ClipPrograms,
}

#[derive(Clone, Copy, PartialEq)]
struct FrameStyle {
    window: Rectangle<i32, Logical>,
    zoom: f64,
    active: bool,
    hovered: bool,
    server_side: bool,
    photo: bool,
}

struct CachedFrame {
    style: FrameStyle,
    rim: PixelShaderElement,
    shadow: PixelShaderElement,
    header: PixelShaderElement,
}

pub(super) struct WindowFrameElements {
    pub rim: PixelShaderElement,
    pub shadow: PixelShaderElement,
    /// SSD background, divider and grip; text and buttons remain above it.
    pub header: Option<PixelShaderElement>,
    pub clip: Option<WindowClip>,
}

#[derive(Clone, Copy)]
enum FrameLayer {
    Rim,
    Shadow,
    Header,
}

impl WindowFrameCache {
    /// Keep IDs across output passes; only the caller knows all mapped windows.
    pub fn retain(&mut self, live: &HashSet<u64>) {
        for renderer in self.renderers.values_mut() {
            renderer.windows.retain(|id, _| live.contains(id));
        }
    }

    pub fn clear(&mut self) {
        self.renderers.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn elements(
        &mut self,
        renderer: &mut GlesRenderer,
        id: u64,
        window: Rectangle<i32, Logical>,
        zoom: f64,
        active: bool,
        hovered: bool,
        server_side: bool,
        photo: bool,
    ) -> Option<WindowFrameElements> {
        if window.is_empty() || !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }
        let cache = self
            .renderers
            .entry(renderer.context_id())
            .or_insert_with(|| RendererFrames {
                shaders: Shaders::new(renderer)
                    .map_err(|error| tracing::warn!(%error, "window frame shaders are unavailable"))
                    .ok(),
                windows: HashMap::new(),
            });
        let shaders = cache.shaders.as_ref()?;
        let style = FrameStyle {
            window,
            zoom,
            active,
            // Active material overrides hover in the reference. Avoid damage
            // when the pointer crosses an already active window.
            hovered: hovered && !active,
            server_side,
            photo: photo && server_side,
        };
        let cached = cache.windows.entry(id).or_insert_with(|| CachedFrame {
            style,
            rim: make_element(&shaders.frame, style, FrameLayer::Rim),
            shadow: make_element(&shaders.frame, style, FrameLayer::Shadow),
            header: make_element(&shaders.frame, style, FrameLayer::Header),
        });
        if cached.style != style {
            let geometry_changed = cached.style.window != style.window
                || cached.style.zoom != style.zoom
                || cached.style.server_side != style.server_side;
            for (element, layer, changed) in [
                (
                    &mut cached.rim,
                    FrameLayer::Rim,
                    geometry_changed
                        || cached.style.active != style.active
                        || cached.style.hovered != style.hovered,
                ),
                (
                    &mut cached.shadow,
                    FrameLayer::Shadow,
                    geometry_changed
                        || cached.style.active != style.active
                        || cached.style.hovered != style.hovered,
                ),
                (
                    &mut cached.header,
                    FrameLayer::Header,
                    geometry_changed || cached.style.photo != style.photo,
                ),
            ] {
                if !changed {
                    continue;
                }
                let (area, uniforms) = appearance(style, layer);
                element.resize(area, None);
                element.update_uniforms(uniforms);
            }
            cached.style = style;
        }
        Some(WindowFrameElements {
            rim: cached.rim.clone(),
            shadow: cached.shadow.clone(),
            header: server_side.then(|| cached.header.clone()),
            clip: server_side.then(|| {
                WindowClip::new(
                    shaders.clip.clone(),
                    window,
                    // The border occupies the pixel outside the client geometry.
                    19.0 * zoom,
                )
            }),
        })
    }
}

impl Shaders {
    fn new(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        Ok(Self {
            frame: renderer.compile_custom_pixel_shader(
                include_str!("window_frame/frame.frag"),
                &[
                    UniformName::new("area_size", UniformType::_2f),
                    UniformName::new("window_size", UniformType::_2f),
                    UniformName::new("window_offset", UniformType::_2f),
                    UniformName::new("zoom", UniformType::_1f),
                    UniformName::new("radius", UniformType::_1f),
                    UniformName::new("active", UniformType::_1f),
                    UniformName::new("hovered", UniformType::_1f),
                    UniformName::new("mode", UniformType::_1f),
                    UniformName::new("photo", UniformType::_1f),
                ],
            )?,
            clip: clip::ClipPrograms::new(renderer)?,
        })
    }
}

fn make_element(
    shader: &GlesPixelProgram,
    style: FrameStyle,
    layer: FrameLayer,
) -> PixelShaderElement {
    let (area, uniforms) = appearance(style, layer);
    PixelShaderElement::new(shader.clone(), area, None, 1.0, uniforms, Kind::Unspecified)
}

fn appearance(
    style: FrameStyle,
    layer: FrameLayer,
) -> (Rectangle<i32, Logical>, Vec<Uniform<'static>>) {
    // Three sigma of the broadest reference shadow, including its displacement.
    // The transparent interior is discarded by the shader and opaque clients
    // let Smithay subtract their body from the damaged shadow regions.
    let padding = (match layer {
        FrameLayer::Rim => 1.0,
        FrameLayer::Shadow => 144.0,
        FrameLayer::Header => 0.0,
    } * style.zoom)
        .ceil() as i32;
    let mut area = Rectangle::new(
        (
            style.window.loc.x.saturating_sub(padding),
            style.window.loc.y.saturating_sub(padding),
        )
            .into(),
        (
            style
                .window
                .size
                .w
                .saturating_add(padding.saturating_mul(2)),
            style
                .window
                .size
                .h
                .saturating_add(padding.saturating_mul(2)),
        )
            .into(),
    );
    if matches!(layer, FrameLayer::Header) {
        area.size.h = ((f64::from(crate::config::TITLE_BAR_HEIGHT) * style.zoom).round() as i32)
            .min(style.window.size.h)
            .max(1);
    }
    let uniforms = vec![
        Uniform::new("area_size", (area.size.w as f32, area.size.h as f32)),
        Uniform::new(
            "window_size",
            (style.window.size.w as f32, style.window.size.h as f32),
        ),
        Uniform::new(
            "window_offset",
            (
                (style.window.loc.x - area.loc.x) as f32,
                (style.window.loc.y - area.loc.y) as f32,
            ),
        ),
        Uniform::new("zoom", style.zoom as f32),
        Uniform::new(
            "radius",
            if style.server_side {
                19.0 * style.zoom as f32
            } else {
                0.0
            },
        ),
        Uniform::new("active", if style.active { 1.0f32 } else { 0.0 }),
        Uniform::new("hovered", if style.hovered { 1.0f32 } else { 0.0 }),
        Uniform::new(
            "mode",
            match layer {
                FrameLayer::Rim => 0.0f32,
                FrameLayer::Shadow => 1.0,
                FrameLayer::Header => 2.0,
            },
        ),
        Uniform::new("photo", if style.photo { 1.0f32 } else { 0.0 }),
    ];
    (area, uniforms)
}
