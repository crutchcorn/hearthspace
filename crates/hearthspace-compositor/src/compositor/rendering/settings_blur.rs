//! Backdrops opt in our own Settings window without changing other applications.
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
    settings_blur_shader::{DOWNSAMPLE, PADDING, SettingsBlurShader},
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
    pub zoom: f64,
}

#[derive(Default)]
pub(in crate::compositor) struct SettingsBlurCache {
    renderers: HashMap<ContextId<GlesTexture>, RendererCache>,
}

struct RendererCache {
    shader: Option<SettingsBlurShader>,
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
}

fn output_key(output: Rectangle<i32, Logical>) -> OutputKey {
    (output.loc.x, output.loc.y, output.size.w, output.size.h)
}

impl App {
    pub(super) fn apply_settings_backdrops(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
        scale: i32,
        requests: &[BackdropRequest],
        elements: &mut Vec<TranslatedHearthspaceRenderElement>,
    ) {
        if self.session_locked() {
            self.settings_blur.renderers.clear();
            return;
        }
        let live: HashSet<_> = self
            .windows
            .iter()
            .filter(|window| {
                window.mapped && window_app_id(&window.surface).as_deref() == Some(SETTINGS_APP_ID)
            })
            .map(|window| window.id)
            .collect();
        let outputs: HashSet<_> = self
            .output_logical_rects()
            .into_iter()
            .map(output_key)
            .collect();
        for cache in self.settings_blur.renderers.values_mut() {
            cache
                .windows
                .retain(|(id, output), _| live.contains(id) && outputs.contains(output));
        }
        if !requests.is_empty() {
            self.settings_blur.render(
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
            );
        }
    }
}

impl SettingsBlurCache {
    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
        scale: i32,
        saturation: f32,
        requests: &[BackdropRequest],
        elements: &mut Vec<TranslatedHearthspaceRenderElement>,
    ) {
        let cache = self
            .renderers
            .entry(renderer.context_id())
            .or_insert_with(|| RendererCache {
                shader: SettingsBlurShader::new(renderer)
                    .map_err(|error| {
                        tracing::warn!(%error, "Settings backdrop blur is unavailable");
                    })
                    .ok(),
                windows: HashMap::new(),
            });
        let Some(shader) = &cache.shader else {
            return;
        };
        let output_bounds = Rectangle::from_size(output.size.to_physical(scale));
        // Insertion below a lower owner leaves every earlier recorded index
        // unchanged. A higher glass window then sees the resolved lower glass.
        for request in requests.iter().rev() {
            let window = Rectangle::new(request.window.loc - output.loc, request.window.size)
                .to_physical(scale);
            let Some(visible) = window.intersection(output_bounds) else {
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
                    saturation,
                    &elements[request.after..],
                )?;
                Ok(TextureRenderElement::from_texture_render_buffer(
                    visible.loc.to_f64(),
                    &backdrop.resolved,
                    None,
                    None,
                    None,
                    Kind::Unspecified,
                ))
            })();
            match result {
                Ok(element) => elements.insert(
                    request.after,
                    RelocateRenderElement::from_element(
                        HearthspaceRenderElement::from(element),
                        (0, 0),
                        Relocate::Relative,
                    )
                    .into(),
                ),
                Err(error) => {
                    cache.windows.remove(&key);
                    tracing::warn!(%error, "failed to update Settings backdrop");
                }
            }
        }
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

impl Backdrop {
    fn new(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        sample_scale: f64,
    ) -> Result<Self> {
        let low = Size::<i32, Buffer>::from((
            (f64::from(size.w) / sample_scale).ceil() as i32,
            (f64::from(size.h) / sample_scale).ceil() as i32,
        ));
        let padded = (low.w + 2 * PADDING, low.h + 2 * PADDING);
        // Only the visible output intersection is allocated at full resolution.
        // The source and Gaussian passes stay small even at high output scale.
        let scene = renderer.create_buffer(Fourcc::Abgr8888, padded.into())?;
        let intermediate = renderer.create_buffer(Fourcc::Abgr8888, padded.into())?;
        let low_blur = renderer.create_buffer(Fourcc::Abgr8888, low)?;
        let resolved = renderer.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())?;
        Ok(Self {
            size,
            sample_scale,
            scene,
            intermediate,
            low_blur,
            resolved: TextureRenderBuffer::from_texture(
                renderer,
                resolved,
                1,
                Transform::Normal,
                None,
            ),
            damage: OutputDamageTracker::new(padded, 1.0, Transform::Normal),
            mask: None,
            saturation: 1.0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        renderer: &mut GlesRenderer,
        shader: &SettingsBlurShader,
        output: Rectangle<i32, Physical>,
        window: Rectangle<i32, Physical>,
        visible: Rectangle<i32, Physical>,
        saturation: f32,
        source: &[TranslatedHearthspaceRenderElement],
    ) -> Result<()> {
        let elements: Vec<_> = source
            .iter()
            .map(|element| {
                RelocateRenderElement::from_element(
                    RescaleRenderElement::from_element(
                        RelocateRenderElement::from_element(
                            element,
                            (-visible.loc.x, -visible.loc.y),
                            Relocate::Relative,
                        ),
                        Point::from((0, 0)),
                        1.0 / self.sample_scale,
                    ),
                    (PADDING, PADDING),
                    Relocate::Relative,
                )
            })
            .collect();
        let changed = {
            let mut target = renderer.bind(&mut self.scene)?;
            self.damage
                .render_output(
                    renderer,
                    &mut target,
                    usize::from(self.mask.is_some()),
                    &elements,
                    Color32F::new(0.0, 0.0, 0.0, 1.0),
                )?
                .damage
                .is_some()
        };
        let next_mask = (window, visible);
        if !changed && self.mask == Some(next_mask) && self.saturation == saturation {
            return Ok(());
        }
        let low = Size::from((
            (f64::from(self.size.w) / self.sample_scale).ceil() as i32,
            (f64::from(self.size.h) / self.sample_scale).ceil() as i32,
        ));
        // Clamp each blur tap at the physical output boundary. Unmapped space
        // outside a monitor must not produce black fringes around maximized UI.
        let padded_size = (low.w + PADDING * 2, low.h + PADDING * 2);
        let left = (f64::from(-visible.loc.x) / self.sample_scale + f64::from(PADDING)).max(0.0);
        let top = (f64::from(-visible.loc.y) / self.sample_scale + f64::from(PADDING)).max(0.0);
        let right = (f64::from(output.size.w - visible.loc.x) / self.sample_scale
            + f64::from(PADDING))
        .min(f64::from(padded_size.0));
        let bottom = (f64::from(output.size.h - visible.loc.y) / self.sample_scale
            + f64::from(PADDING))
        .min(f64::from(padded_size.1));
        shader.blur.render_region(
            renderer,
            &self.scene,
            &mut self.low_blur,
            &mut self.intermediate,
            low,
            PADDING,
            0.0,
            Some(Rectangle::new(
                (left, top).into(),
                ((right - left).max(1.0), (bottom - top).max(1.0)).into(),
            )),
        )?;
        self.resolved.render().draw(|target| {
            shader.resolve(
                renderer,
                &self.low_blur,
                target,
                self.size,
                window,
                visible,
                self.sample_scale,
                20.0 * self.sample_scale / DOWNSAMPLE,
                saturation,
            )?;
            Ok::<_, smithay::backend::renderer::gles::GlesError>(vec![Rectangle::from_size(
                (self.size.w, self.size.h).into(),
            )])
        })?;
        self.mask = Some(next_mask);
        self.saturation = saturation;
        Ok(())
    }
}

#[cfg(all(test, feature = "e2e"))]
#[path = "settings_blur_tests.rs"]
mod tests;
