//! Damage-tracked GPU chrome around the application geometry, not its shadows.
use hearthspace_ipc::Appearance;
use hearthspace_ui_components::Palette;
use std::collections::{HashMap, HashSet};

use smithay::{
    backend::renderer::{
        ContextId, Renderer,
        gles::{
            GlesError, GlesPixelProgram, GlesRenderer, GlesTexture, UniformName, UniformType,
            element::PixelShaderElement,
        },
    },
    utils::{Logical, Rectangle},
};

mod clip;
mod style;
use style::{appearance, make_element};
#[cfg(test)]
mod tests;

pub(super) use clip::{RoundedElement, WindowClip, solid_color};

#[derive(Default)]
pub(super) struct WindowFrameCache {
    renderers: HashMap<ContextId<GlesTexture>, RendererFrames>,
    palette: Palette,
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
    appearance: Appearance,
    window: Rectangle<i32, Logical>,
    zoom: f64,
    active: bool,
    hovered: bool,
    server_side: bool,
    photo: bool,
    panel: bool,
    opacity: f32,
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
    pub fn set_appearance(&mut self, appearance: Appearance) {
        if self.palette.appearance != appearance {
            self.palette = Palette::new(appearance);
        }
    }

    /// Keep IDs across output passes; only the caller knows all mapped windows.
    pub fn retain(&mut self, live: &HashSet<u64>) {
        for renderer in self.renderers.values_mut() {
            renderer.windows.retain(|id, _| live.contains(id));
        }
    }

    pub fn clear(&mut self) {
        self.renderers.clear();
    }

    #[cfg(test)]
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
        self.elements_with_opacity(
            renderer,
            id,
            window,
            zoom,
            active,
            hovered,
            server_side,
            photo,
            1.,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn elements_with_opacity(
        &mut self,
        renderer: &mut GlesRenderer,
        id: u64,
        window: Rectangle<i32, Logical>,
        zoom: f64,
        active: bool,
        hovered: bool,
        server_side: bool,
        photo: bool,
        opacity: f32,
    ) -> Option<WindowFrameElements> {
        let style = FrameStyle {
            appearance: self.palette.appearance,
            window,
            zoom,
            active,
            // Active material overrides hover in the reference. Avoid damage
            // when the pointer crosses an already active window.
            hovered: hovered && !active,
            server_side,
            photo: photo && server_side,
            panel: false,
            opacity,
        };
        self.frame(renderer, id, style)
    }

    /// The compositor owns the halo outside a transparent native panel's buffer.
    /// Its rounded interior remains empty so glass never sees its own shadow.
    #[allow(clippy::too_many_arguments)]
    pub fn panel_shadow(
        &mut self,
        renderer: &mut GlesRenderer,
        id: u64,
        window: Rectangle<i32, Logical>,
        zoom: f64,
        emphasized: bool,
        opacity: f32,
    ) -> Option<PixelShaderElement> {
        let style = FrameStyle {
            appearance: self.palette.appearance,
            window,
            zoom,
            active: emphasized,
            hovered: false,
            server_side: false,
            photo: false,
            panel: true,
            opacity,
        };
        self.frame(renderer, id, style).map(|frame| frame.shadow)
    }

    fn frame(
        &mut self,
        renderer: &mut GlesRenderer,
        id: u64,
        style: FrameStyle,
    ) -> Option<WindowFrameElements> {
        if style.window.is_empty() || !style.zoom.is_finite() || style.zoom <= 0.0 {
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
        let cached = cache.windows.entry(id).or_insert_with(|| CachedFrame {
            style,
            rim: make_element(&shaders.frame, style, FrameLayer::Rim, &self.palette),
            shadow: make_element(&shaders.frame, style, FrameLayer::Shadow, &self.palette),
            header: make_element(&shaders.frame, style, FrameLayer::Header, &self.palette),
        });
        if cached.style != style {
            let geometry_changed = cached.style.window != style.window
                || cached.style.zoom != style.zoom
                || cached.style.server_side != style.server_side
                || cached.style.panel != style.panel;
            let material_changed = cached.style.appearance != style.appearance
                || cached.style.opacity != style.opacity;
            for (element, layer, changed) in [
                (
                    &mut cached.rim,
                    FrameLayer::Rim,
                    geometry_changed
                        || material_changed
                        || cached.style.active != style.active
                        || cached.style.hovered != style.hovered,
                ),
                (
                    &mut cached.shadow,
                    FrameLayer::Shadow,
                    geometry_changed
                        || material_changed
                        || cached.style.active != style.active
                        || cached.style.hovered != style.hovered,
                ),
                (
                    &mut cached.header,
                    FrameLayer::Header,
                    geometry_changed || material_changed || cached.style.photo != style.photo,
                ),
            ] {
                if !changed {
                    continue;
                }
                let (area, uniforms) = appearance(style, layer, &self.palette);
                element.resize(area, None);
                element.update_uniforms(uniforms);
            }
            cached.style = style;
        }
        Some(WindowFrameElements {
            rim: cached.rim.clone(),
            shadow: cached.shadow.clone(),
            header: style.server_side.then(|| cached.header.clone()),
            clip: style.server_side.then(|| {
                WindowClip::new(
                    shaders.clip.clone(),
                    style.window,
                    // The border occupies the pixel outside the client geometry.
                    19.0 * style.zoom,
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
                    UniformName::new("scene_opacity", UniformType::_1f),
                    UniformName::new("area_size", UniformType::_2f),
                    UniformName::new("window_size", UniformType::_2f),
                    UniformName::new("window_offset", UniformType::_2f),
                    UniformName::new("zoom", UniformType::_1f),
                    UniformName::new("radius", UniformType::_1f),
                    UniformName::new("active", UniformType::_1f),
                    UniformName::new("hovered", UniformType::_1f),
                    UniformName::new("mode", UniformType::_1f),
                    UniformName::new("panel", UniformType::_1f),
                    UniformName::new("chrome_color", UniformType::_4f),
                    UniformName::new("wash_color", UniformType::_4f),
                    UniformName::new("control_color", UniformType::_4f),
                    UniformName::new("line_color", UniformType::_4f),
                    UniformName::new("muted_color", UniformType::_4f),
                    UniformName::new("border_color", UniformType::_4f),
                    UniformName::new("edge_color", UniformType::_4f),
                    UniformName::new("edge_all_color", UniformType::_4f),
                    UniformName::new("broad_shadow_color", UniformType::_4f),
                    UniformName::new("near_shadow_color", UniformType::_4f),
                    UniformName::new("inset_color", UniformType::_4f),
                ],
            )?,
            clip: clip::ClipPrograms::new(renderer)?,
        })
    }
}
