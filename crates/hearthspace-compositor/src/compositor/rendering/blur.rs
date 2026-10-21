//! GPU backdrops for the shell's four corners. Only the padded corner regions
//! are rendered offscreen; unchanged desktop content reuses the previous blur.
use std::collections::{HashMap, HashSet};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Color32F, ContextId, Offscreen, Renderer,
            damage::OutputDamageTracker,
            element::{
                Kind,
                texture::{TextureRenderBuffer, TextureRenderElement},
                utils::{Relocate, RelocateRenderElement},
            },
            gles::GlesRenderer,
        },
    },
    utils::{Logical, Physical, Rectangle, Size, Transform},
};

use super::{
    GlesTexture, TranslatedHearthspaceRenderElement,
    blur_shader::{BLUR_PADDING, CornerBlurShader},
};
use crate::compositor::{App, ManagedWindowKind};

type BlurResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
pub(in crate::compositor) struct CornerBlurCache {
    renderers: HashMap<(ContextId<GlesTexture>, i32), RendererBlur>,
}

struct RendererBlur {
    // A failed shader leaves readable translucent controls and logs once.
    shader: Option<CornerBlurShader>,
    corners: HashMap<u64, CornerBackdrop>,
}

#[derive(Clone, Copy)]
struct CornerRegion {
    id: u64,
    rect: Rectangle<i32, Physical>,
    radius: f32,
    alpha: f32,
}

struct CornerBackdrop {
    size: Size<i32, Physical>,
    scale: i32,
    scene: GlesTexture,
    intermediate: GlesTexture,
    blurred: TextureRenderBuffer<GlesTexture>,
    damage: OutputDamageTracker,
    initialized: bool,
    radius: f32,
}

impl App {
    pub(super) fn corner_blur_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
        scale: i32,
        desktop: &[TranslatedHearthspaceRenderElement],
    ) -> Vec<TextureRenderElement<GlesTexture>> {
        // A locked desktop must never retain or sample the unlocked scene.
        if self.session_locked() {
            self.corner_blur.renderers.clear();
            return Vec::new();
        }
        let mut live = HashSet::new();
        let mut regions = Vec::new();
        for (index, window) in self.windows.iter().enumerate() {
            let corner = self.shell_corner(index).is_some();
            let panel = window.kind == ManagedWindowKind::Launcher;
            if !window.mapped || (!corner && !panel) {
                continue;
            }
            if self.canvas_search_result_identity(index).is_some() {
                live.insert(window.id);
                if let Some((pose, alpha)) = self.canvas_search_pose(index)
                    && self.window_visible_on_output(index, output)
                    && alpha > 0.0
                {
                    let logical = Rectangle::new(
                        (pose.x.round() as i32, pose.y.round() as i32).into(),
                        (pose.width.round() as i32, pose.height.round() as i32).into(),
                    );
                    if logical.overlaps(output) && logical.size.w > 0 && logical.size.h > 0 {
                        regions.push(CornerRegion {
                            id: window.id,
                            rect: Rectangle::new(logical.loc - output.loc, logical.size)
                                .to_physical(scale),
                            radius: (hearthspace_common::SHELL_CORNER_RADIUS * pose.width / 240.0
                                * f64::from(scale)) as f32,
                            alpha: alpha as f32,
                        });
                    }
                }
                continue;
            }
            let Some(window_output) =
                self.output_logical_rect_for_name(window.shell_output_name.as_deref())
            else {
                continue;
            };
            live.insert(window.id);
            if window_output != output
                || self.overview_active()
                || !self.window_visible_on_output(index, output)
            {
                continue;
            }
            let Some(origin) = (if corner {
                self.shell_bar_logical_origin(index)
            } else {
                self.launcher_logical_origin(index)
            }) else {
                continue;
            };
            let rect = Rectangle::new(
                origin + self.window_surface_geometry_loc(index) - output.loc,
                self.window_geometry_size(index),
            )
            .to_physical(scale);
            if rect.size.w > 0 && rect.size.h > 0 {
                regions.push(CornerRegion {
                    id: window.id,
                    rect,
                    radius: hearthspace_common::SHELL_CORNER_RADIUS as f32 * scale as f32,
                    alpha: 1.0,
                });
            }
        }
        for cache in self.corner_blur.renderers.values_mut() {
            cache.corners.retain(|id, _| live.contains(id));
        }
        if regions.is_empty() {
            return Vec::new();
        }
        self.corner_blur.render(renderer, &regions, scale, desktop)
    }
}

impl CornerBlurCache {
    fn render(
        &mut self,
        renderer: &mut GlesRenderer,
        regions: &[CornerRegion],
        scale: i32,
        desktop: &[TranslatedHearthspaceRenderElement],
    ) -> Vec<TextureRenderElement<GlesTexture>> {
        let cache = self
            .renderers
            .entry((renderer.context_id(), scale))
            .or_insert_with(|| RendererBlur {
                shader: match CornerBlurShader::new(renderer, scale) {
                    Ok(shader) => Some(shader),
                    Err(error) => {
                        tracing::warn!(%error, "corner backdrop blur is unavailable");
                        None
                    }
                },
                corners: HashMap::new(),
            });
        let Some(shader) = cache.shader.as_ref() else {
            return Vec::new();
        };
        let mut elements = Vec::with_capacity(regions.len());
        for region in regions {
            let result: BlurResult<_> =
                (|| {
                    if cache.corners.get(&region.id).is_none_or(|corner| {
                        corner.size != region.rect.size || corner.scale != scale
                    }) {
                        cache.corners.insert(
                            region.id,
                            CornerBackdrop::new(renderer, region.rect.size, scale)?,
                        );
                    }
                    let corner = cache.corners.get_mut(&region.id).unwrap();
                    corner.update(renderer, shader, region.rect, region.radius, desktop)?;
                    Ok(TextureRenderElement::from_texture_render_buffer(
                        region.rect.loc.to_f64(),
                        &corner.blurred,
                        Some(region.alpha),
                        None,
                        None,
                        Kind::Unspecified,
                    ))
                })();
            match result {
                Ok(element) => elements.push(element),
                Err(error) => {
                    // Do not present a stale backing after an incomplete pass.
                    cache.corners.remove(&region.id);
                    tracing::warn!(%error, "failed to update corner backdrop");
                }
            }
        }
        elements
    }
}

#[cfg(all(test, feature = "e2e"))]
#[path = "blur_tests.rs"]
mod tests;

impl CornerBackdrop {
    fn new(renderer: &mut GlesRenderer, size: Size<i32, Physical>, scale: i32) -> BlurResult<Self> {
        let padding = BLUR_PADDING * scale;
        let padded = (size.w + 2 * padding, size.h + 2 * padding);
        let scene = renderer.create_buffer(Fourcc::Abgr8888, padded.into())?;
        let intermediate = renderer.create_buffer(Fourcc::Abgr8888, padded.into())?;
        let target = renderer.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())?;
        Ok(Self {
            size,
            scale,
            scene,
            intermediate,
            // Coordinates here are already physical output pixels.
            blurred: TextureRenderBuffer::from_texture(
                renderer,
                target,
                1,
                Transform::Normal,
                None,
            ),
            damage: OutputDamageTracker::new(padded, 1.0, Transform::Normal),
            initialized: false,
            radius: 0.0,
        })
    }

    fn update(
        &mut self,
        renderer: &mut GlesRenderer,
        shader: &CornerBlurShader,
        region: Rectangle<i32, Physical>,
        radius: f32,
        desktop: &[TranslatedHearthspaceRenderElement],
    ) -> BlurResult<()> {
        let padding = BLUR_PADDING * self.scale;
        let offset = (padding - region.loc.x, padding - region.loc.y);
        let elements: Vec<_> = desktop
            .iter()
            .map(|element| RelocateRenderElement::from_element(element, offset, Relocate::Relative))
            .collect();
        let changed = {
            let mut target = renderer.bind(&mut self.scene)?;
            self.damage
                .render_output(
                    renderer,
                    &mut target,
                    usize::from(self.initialized),
                    &elements,
                    Color32F::new(0.0, 0.0, 0.0, 1.0),
                )?
                .damage
                .is_some()
        };
        if !changed && self.initialized && self.radius == radius {
            return Ok(());
        }
        self.blurred.render().draw(|target| {
            shader.render_region(
                renderer,
                &self.scene,
                target,
                &mut self.intermediate,
                self.size,
                padding,
                radius,
                None,
            )?;
            // Any source change can spread throughout this small backdrop.
            // Full-corner damage also covers changes just outside its bounds.
            Ok::<_, smithay::backend::renderer::gles::GlesError>(vec![Rectangle::from_size(
                (self.size.w, self.size.h).into(),
            )])
        })?;
        self.initialized = true;
        self.radius = radius;
        Ok(())
    }
}
