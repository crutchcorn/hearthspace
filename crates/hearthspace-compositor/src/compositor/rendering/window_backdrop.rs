//! Backdrops for Settings and compositor-owned glass titlebars.
//! Each source slice starts below its owner in the real front-to-back stack.
use std::collections::{HashMap, HashSet};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Color32F, ContextId, Offscreen, Renderer,
            damage::OutputDamageTracker,
            element::{
                Element, Kind,
                texture::{TextureRenderBuffer, TextureRenderElement},
                utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
            },
            gles::GlesRenderer,
        },
    },
    utils::{Buffer, Logical, Physical, Point, Rectangle, Size, Transform},
};

use super::{
    GlesTexture, HearthspaceRenderElement, TranslatedHearthspaceRenderElement,
    window_backdrop_shader::{DOWNSAMPLE, PADDING, WindowBackdropShader},
};
use crate::compositor::{App, windows::window_app_id};

pub(super) const SETTINGS_APP_ID: &str = "dev.hearthspace.Settings";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type OutputKey = (i32, i32, i32, i32);
type CacheKey = (u64, OutputKey);

#[derive(Clone, Copy)]
pub(super) struct BackdropRequest {
    pub id: u64,
    pub front: usize,
    pub after: usize,
    pub window: Rectangle<i32, Logical>,
    /// Titlebars sample only their own strip, while retaining the whole
    /// window's corner mask. Settings samples its entire translucent window.
    pub region: Option<Rectangle<i32, Logical>>,
    pub radius: f64,
    pub modal: bool,
    pub zoom: f64,
    pub opacity: f32,
}

#[derive(Default)]
pub(in crate::compositor) struct WindowBackdropCache {
    renderers: HashMap<ContextId<GlesTexture>, RendererCache>,
}

struct RendererCache {
    shader: Option<WindowBackdropShader>,
    modal_shader: Option<WindowBackdropShader>,
    windows: HashMap<CacheKey, Backdrop>,
}

struct Backdrop {
    size: Size<i32, Physical>,
    sample_scale: f64,
    scene: GlesTexture,
    intermediate: GlesTexture,
    low_blur: GlesTexture,
    resolved: TextureRenderBuffer<GlesTexture>,
    damage: OutputDamageTracker,
    mask: Option<(Rectangle<i32, Physical>, Rectangle<i32, Physical>)>,
    saturation: f32,
    radius: f64,
    #[cfg(test)]
    blur_passes: usize,
}

fn output_key(output: Rectangle<i32, Logical>) -> OutputKey {
    (output.loc.x, output.loc.y, output.size.w, output.size.h)
}

impl App {
    pub(super) fn apply_window_backdrops(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
        scale: i32,
        requests: &[BackdropRequest],
        elements: &mut Vec<TranslatedHearthspaceRenderElement>,
    ) -> Vec<usize> {
        if self.session_locked() {
            self.window_backdrop.renderers.clear();
            return (0..=elements.len()).collect();
        }
        let mut live: HashSet<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, window)| {
                window.mapped
                    && (self.has_compositor_chrome(*index)
                        || window_app_id(&window.surface).as_deref() == Some(SETTINGS_APP_ID)
                        || self.layout_glass_rect(*index).is_some())
            })
            .map(|(_, window)| window.id)
            .collect();
        if self.modal_confirmation_active() {
            live.insert(u64::MAX);
        }
        let outputs: HashSet<_> = self
            .output_logical_rects()
            .into_iter()
            .map(output_key)
            .collect();
        for cache in self.window_backdrop.renderers.values_mut() {
            cache
                .windows
                .retain(|(id, output), _| live.contains(id) && outputs.contains(output));
        }
        if !requests.is_empty() {
            self.window_backdrop.render(
                renderer,
                output,
                scale,
                if self.background_image_path.is_some() {
                    1.15
                } else {
                    1.35
                },
                requests,
                elements,
            )
        } else {
            (0..=elements.len()).collect()
        }
    }
}

impl WindowBackdropCache {
    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
        scale: i32,
        saturation: f32,
        requests: &[BackdropRequest],
        elements: &mut Vec<TranslatedHearthspaceRenderElement>,
    ) -> Vec<usize> {
        let original_len = elements.len();
        let mut insertions = Vec::new();
        let cache = self
            .renderers
            .entry(renderer.context_id())
            .or_insert_with(|| RendererCache {
                shader: WindowBackdropShader::new(renderer)
                    .map_err(|error| {
                        tracing::warn!(%error, "window backdrop blur is unavailable");
                    })
                    .ok(),
                modal_shader: None,
                windows: HashMap::new(),
            });
        if requests.iter().any(|request| request.modal) && cache.modal_shader.is_none() {
            cache.modal_shader = WindowBackdropShader::with_sigma(renderer, 2.0)
                .map_err(|error| tracing::warn!(%error, "dialog backdrop blur unavailable"))
                .ok();
        }
        let output_bounds = Rectangle::from_size(output.size.to_physical(scale));
        // Insertion below a lower owner leaves every earlier recorded index
        // unchanged. A higher glass window then sees the resolved lower glass.
        for request in requests.iter().rev() {
            let Some(shader) = (if request.modal {
                &cache.modal_shader
            } else {
                &cache.shader
            }) else {
                continue;
            };
            let window = Rectangle::new(request.window.loc - output.loc, request.window.size)
                .to_physical(scale);
            let region = request.region.map_or(window, |region| {
                Rectangle::new(region.loc - output.loc, region.size).to_physical(scale)
            });
            let Some(visible) = region
                .intersection(window)
                .and_then(|region| region.intersection(output_bounds))
            else {
                continue;
            };
            let key = (request.id, output_key(output));
            if occluded(visible, &elements[..request.front]) {
                continue;
            }
            let sample_scale = DOWNSAMPLE * f64::from(scale) * request.zoom;
            let result: Result<_> = (|| {
                if cache.windows.get(&key).is_none_or(|backdrop| {
                    backdrop.size != visible.size || backdrop.sample_scale != sample_scale
                }) {
                    cache
                        .windows
                        .insert(key, Backdrop::new(renderer, visible.size, sample_scale)?);
                }
                let backdrop = cache.windows.get_mut(&key).unwrap();
                backdrop.update(
                    renderer,
                    shader,
                    output_bounds,
                    window,
                    visible,
                    request.radius * f64::from(scale) * request.zoom,
                    saturation,
                    &elements[request.after..],
                )?;
                Ok(TextureRenderElement::from_texture_render_buffer(
                    visible.loc.to_f64(),
                    &backdrop.resolved,
                    Some(request.opacity),
                    None,
                    None,
                    Kind::Unspecified,
                ))
            })();
            match result {
                Ok(element) => {
                    insertions.push(request.after);
                    elements.insert(
                        request.after,
                        RelocateRenderElement::from_element(
                            HearthspaceRenderElement::from(element),
                            (0, 0),
                            Relocate::Relative,
                        )
                        .into(),
                    );
                }
                Err(error) => {
                    cache.windows.remove(&key);
                    tracing::warn!(%error, "failed to update window backdrop");
                }
            }
        }
        (0..=original_len)
            .map(|boundary| boundary + insertions.iter().filter(|&&at| at <= boundary).count())
            .collect()
    }
}

fn occluded(rect: Rectangle<i32, Physical>, above: &[TranslatedHearthspaceRenderElement]) -> bool {
    let opaque = above
        .iter()
        .filter(|element| element.alpha() >= 1.0)
        .flat_map(|element| {
            let origin = element.geometry(1.0.into()).loc;
            element
                .opaque_regions(1.0.into())
                .iter()
                .map(|region| Rectangle::new(region.loc + origin, region.size))
                .collect::<Vec<_>>()
        });
    rect.subtract_rects(opaque).is_empty()
}

#[path = "window_backdrop_surface.rs"]
mod surface;

#[cfg(all(test, feature = "e2e"))]
#[path = "window_backdrop_tests.rs"]
mod tests;
