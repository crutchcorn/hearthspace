//! Small, temporary GPU surfaces compose native group members during a toolbar turn.
use super::{
    App,
    turn_element::{self, TurnElement},
};
use crate::compositor::rendering::{HearthspaceRenderElement, TranslatedHearthspaceRenderElement};
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
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

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
}

impl TurnCache {
    pub(super) fn retain<'a>(&mut self, keys: impl Iterator<Item = &'a String>) {
        let keys: HashSet<_> = keys.map(String::as_str).collect();
        for cache in self.renderers.values_mut() {
            cache.groups.retain(|key, _| keys.contains(key.as_str()));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn element(
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
        // A client can commit its orientation a frame after the placement changes.
        // Preserve those live pixels as well as the final-layout glass while it catches up.
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
                },
            );
        }
        let capture = cache.groups.get_mut(key).unwrap();
        capture.region = region;
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

impl App {
    /// Compose after backdrop sampling so glass, shadows, and outlines rotate together.
    pub(in crate::compositor) fn apply_layout_turns(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &mut Vec<TranslatedHearthspaceRenderElement>,
        spans: &[(u64, Range<usize>)],
        output: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
    ) {
        let mut replaced = HashSet::new();
        let mut replacements = HashMap::new();
        let now = self.layout_editor.motion.sampled_at;
        let turns: Vec<_> = self
            .layout_editor
            .turn
            .turns
            .iter()
            .filter(|(_, turn)| {
                self.output_logical_rect_for_name(Some(&turn.target.output)) == Some(output)
            })
            .map(|(key, turn)| {
                // Placement translation and toolbar rotation share the same current pose.
                // Capturing only the destination would clip members while they travel there.
                let bounds = self
                    .target_window_index(&turn.target)
                    .and_then(|index| self.layout_motion_rect(index))
                    .unwrap_or(turn.bounds);
                let pivot = Point {
                    x: bounds.x
                        + bounds.width * (turn.pivot.x - turn.bounds.x) / turn.bounds.width.max(1.),
                    y: bounds.y
                        + bounds.height * (turn.pivot.y - turn.bounds.y)
                            / turn.bounds.height.max(1.),
                };
                (
                    key.clone(),
                    turn.members.clone(),
                    bounds,
                    pivot,
                    turn.angle(now),
                )
            })
            .collect();
        let scale = self
            .capture_outputs()
            .iter()
            .find(|candidate| {
                candidate.logical_x == output.loc.x
                    && candidate.logical_y == output.loc.y
                    && candidate.logical_width == output.size.w
                    && candidate.logical_height == output.size.h
            })
            .map_or(1, |candidate| candidate.scale);
        for (key, members, bounds, pivot, angle) in turns {
            let bounds = Rect {
                x: (bounds.x - f64::from(output.loc.x)) * f64::from(scale),
                y: (bounds.y - f64::from(output.loc.y)) * f64::from(scale),
                width: bounds.width * f64::from(scale),
                height: bounds.height * f64::from(scale),
            };
            let pivot = Point {
                x: (pivot.x - f64::from(output.loc.x)) * f64::from(scale),
                y: (pivot.y - f64::from(output.loc.y)) * f64::from(scale),
            };
            let ranges: Vec<_> = spans
                .iter()
                .filter(|(id, _)| members.contains(id))
                .map(|(_, range)| range.clone())
                .collect();
            let mut indices: Vec<_> = ranges
                .into_iter()
                .flatten()
                .filter(|index| !replaced.contains(index))
                .collect();
            indices.sort_unstable();
            indices.dedup();
            let Some(first) = indices.iter().copied().min() else {
                continue;
            };
            let scene: Vec<_> = indices.iter().map(|index| &elements[*index]).collect();
            match self
                .layout_editor
                .turn
                .cache
                .element(renderer, &key, &scene, bounds, pivot, angle, scale)
            {
                Ok(Some(element)) => {
                    replacements.insert(
                        first,
                        TranslatedHearthspaceRenderElement::from(
                            RelocateRenderElement::from_element(
                                HearthspaceRenderElement::from(element),
                                (0, 0),
                                Relocate::Relative,
                            ),
                        ),
                    );
                    replaced.extend(indices);
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(%error,"toolbar turn capture failed"),
            }
        }
        let mut result = Vec::with_capacity(elements.len());
        for (index, element) in std::mem::take(elements).into_iter().enumerate() {
            if let Some(replacement) = replacements.remove(&index) {
                result.push(replacement);
            }
            if !replaced.contains(&index) {
                result.push(element);
            }
        }
        *elements = result;
    }
}

#[cfg(all(test, feature = "e2e"))]
#[path = "render_turn_tests.rs"]
mod tests;
