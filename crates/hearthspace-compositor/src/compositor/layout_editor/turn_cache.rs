//! Small, temporary GPU surfaces compose native group members during a toolbar turn.
use super::turn_element::{self, TurnElement};
use crate::compositor::rendering::TranslatedHearthspaceRenderElement;
use hearthspace_common::frame_layout::{Point, Rect};
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
            gles::{GlesRenderer, GlesTexProgram, GlesTexture},
        },
    },
    utils::{Physical, Rectangle, Transform},
};
use std::collections::{HashMap, HashSet};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
pub(super) struct TurnCache {
    renderers: HashMap<(ContextId<GlesTexture>, i32), RendererCache>,
}

struct RendererCache {
    shader: Option<GlesTexProgram>,
    groups: HashMap<String, Capture>,
}
struct Capture {
    region: Rectangle<i32, Physical>,
    buffer: TextureRenderBuffer<GlesTexture>,
    damage: OutputDamageTracker,
    initialized: bool,
    pivot: Point,
    angle: f64,
}

impl TurnCache {
    pub(super) fn retain<'a>(&mut self, keys: impl Iterator<Item = &'a String>) {
        let keys: HashSet<_> = keys.map(String::as_str).collect();
        for cache in self.renderers.values_mut() {
            cache.groups.retain(|key, _| keys.contains(key.as_str()));
        }
    }

    pub(super) fn move_held(
        &mut self,
        renderer: &GlesRenderer,
        key: &str,
        scale: i32,
        delta: Point,
    ) {
        if let Some(capture) = self
            .renderers
            .get_mut(&(renderer.context_id(), scale))
            .and_then(|cache| cache.groups.get_mut(key))
        {
            capture.region.loc.x += delta.x.round() as i32;
            capture.region.loc.y += delta.y.round() as i32;
            capture.pivot.x += delta.x.round();
            capture.pivot.y += delta.y.round();
        }
    }

    /// Keep the last presented composition intact while clients reflow independently.
    pub(super) fn held(
        &self,
        renderer: &GlesRenderer,
        key: &str,
        scale: i32,
    ) -> Option<TurnElement> {
        let cache = self.renderers.get(&(renderer.context_id(), scale))?;
        let capture = cache.groups.get(key)?;
        let region = capture.region;
        let inner = TextureRenderElement::from_texture_render_buffer(
            region.loc.to_f64(),
            &capture.buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        );
        Some(TurnElement::new(
            inner,
            cache.shader.as_ref()?.clone(),
            Rect {
                x: f64::from(region.loc.x),
                y: f64::from(region.loc.y),
                width: f64::from(region.size.w),
                height: f64::from(region.size.h),
            },
            capture.pivot,
            capture.angle,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        key: &str,
        elements: &[&TranslatedHearthspaceRenderElement],
        bounds: Rect,
        pivot: Point,
        angle: f64,
        scale: i32,
    ) -> Result<Option<TurnElement>> {
        let cache = self
            .renderers
            .entry((renderer.context_id(), scale))
            .or_insert_with(|| RendererCache {
                shader: turn_element::compile(renderer)
                    .map_err(|error| tracing::warn!(%error,"toolbar turn shader unavailable"))
                    .ok(),
                groups: HashMap::new(),
            });
        let Some(shader) = cache.shader.as_ref() else {
            return Ok(None);
        };
        // Include the group shadow and its edit outline, keeping a bounded transient target.
        let mut region = Rectangle::<i32, Physical>::new(
            (
                (bounds.x - 32. * f64::from(scale)).floor() as i32,
                (bounds.y - 32. * f64::from(scale)).floor() as i32,
            )
                .into(),
            (
                (bounds.width + 64. * f64::from(scale)).ceil() as i32,
                (bounds.height + 64. * f64::from(scale)).ceil() as i32,
            )
                .into(),
        );
        // The caller only captures coherent layouts. Include shadows/overlays owned
        // by this group, without ever combining old native buffers with a new layout.
        for element in elements {
            region = region.merge(element.geometry(1.0.into()));
        }
        if region.size.w <= 0 || region.size.h <= 0 || region.size.w > 8192 || region.size.h > 8192
        {
            return Ok(None);
        }
        let size = region.size;
        if cache
            .groups
            .get(key)
            .is_none_or(|capture| capture.region.size != region.size)
        {
            let texture = renderer.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())?;
            cache.groups.insert(
                key.into(),
                Capture {
                    region,
                    buffer: TextureRenderBuffer::from_texture(
                        renderer,
                        texture,
                        1,
                        Transform::Normal,
                        None,
                    ),
                    damage: OutputDamageTracker::new(size, 1.0, Transform::Normal),
                    initialized: false,
                    pivot,
                    angle,
                },
            );
        }
        let capture = cache.groups.get_mut(key).unwrap();
        capture.region = region;
        capture.pivot = pivot;
        capture.angle = angle;
        let offset = (-region.loc.x, -region.loc.y);
        let scene: Vec<_> = elements
            .iter()
            .map(|element| {
                RescaleRenderElement::from_element(
                    RelocateRenderElement::from_element(*element, offset, Relocate::Relative),
                    (0, 0).into(),
                    1.0,
                )
            })
            .collect();
        capture.buffer.render().draw(|texture| {
            let mut target = renderer.bind(texture)?;
            let changed = capture.damage.render_output(
                renderer,
                &mut target,
                usize::from(capture.initialized),
                &scene,
                Color32F::new(0., 0., 0., 0.),
            )?;
            Ok::<_, Box<dyn std::error::Error>>(if changed.damage.is_some() {
                vec![Rectangle::from_size((size.w, size.h).into())]
            } else {
                Vec::new()
            })
        })?;
        capture.initialized = true;
        let inner = TextureRenderElement::from_texture_render_buffer(
            region.loc.to_f64(),
            &capture.buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        );
        let bounds = Rect {
            x: f64::from(region.loc.x),
            y: f64::from(region.loc.y),
            width: f64::from(region.size.w),
            height: f64::from(region.size.h),
        };
        Ok(Some(TurnElement::new(
            inner,
            shader.clone(),
            bounds,
            pivot,
            angle,
        )))
    }
}
