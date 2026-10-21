//! Cached guide fills and dashed editable-object outlines, in logical output coordinates.
use std::collections::{HashMap, HashSet};

use hearthspace_common::frame_layout::Rect;
use smithay::{
    backend::renderer::{
        ContextId, Renderer,
        element::Kind,
        gles::{
            GlesPixelProgram, GlesRenderer, GlesTexture, Uniform, UniformName, UniformType,
            element::PixelShaderElement,
        },
    },
    utils::{Logical, Rectangle},
};

use crate::compositor::rendering::HearthspaceRenderElement;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::compositor) struct OverlayShape {
    pub key: String,
    pub bounds: Rect,
    pub ink: [f32; 4],
    pub radius: f32,
    pub stroke: f32,
    /// Fill coverage relative to the outline ink, matching CSS translucent backgrounds.
    pub fill: f32,
    pub dashed: bool,
    pub opacity: f32,
}

#[derive(Default)]
pub(in crate::compositor) struct OverlayCache {
    renderers: HashMap<ContextId<GlesTexture>, Option<RendererCache>>,
}

struct RendererCache {
    shader: GlesPixelProgram,
    shapes: HashMap<String, Cached>,
}

struct Cached {
    element: PixelShaderElement,
    shape: OverlayShape,
    rectangle: Rectangle<i32, Logical>,
}

impl OverlayCache {
    pub(in crate::compositor) fn elements(
        &mut self,
        renderer: &mut GlesRenderer,
        shapes: &[OverlayShape],
    ) -> Vec<HearthspaceRenderElement> {
        let Some(cache) = self
            .renderers
            .entry(renderer.context_id())
            .or_insert_with(|| {
                let shader = renderer
                    .compile_custom_pixel_shader(
                        include_str!("overlay.frag"),
                        &[
                            UniformName::new("area_size", UniformType::_2f),
                            UniformName::new("shape_rect", UniformType::_4f),
                            UniformName::new("ink", UniformType::_4f),
                            UniformName::new("shape", UniformType::_4f),
                        ],
                    )
                    .map_err(|error| tracing::warn!(%error, "layout overlay shader unavailable"))
                    .ok()?;
                Some(RendererCache {
                    shader,
                    shapes: HashMap::new(),
                })
            })
            .as_mut()
        else {
            return Vec::new();
        };
        let keys: HashSet<&str> = shapes.iter().map(|shape| shape.key.as_str()).collect();
        cache.shapes.retain(|key, _| keys.contains(key.as_str()));
        shapes
            .iter()
            .filter_map(|shape| {
                if shape.opacity <= 0.0 || shape.bounds.width <= 0.0 || shape.bounds.height <= 0.0 {
                    return None;
                }
                let rectangle = enclosing_rect(shape.bounds);
                let cached = match cache.shapes.entry(shape.key.clone()) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => entry.insert(Cached {
                        element: PixelShaderElement::new(
                            cache.shader.clone(),
                            rectangle,
                            None,
                            1.0,
                            uniforms(shape, rectangle),
                            Kind::Unspecified,
                        ),
                        shape: shape.clone(),
                        rectangle,
                    }),
                };
                if cached.shape != *shape || cached.rectangle != rectangle {
                    cached.element.resize(rectangle, None);
                    cached.element.update_uniforms(uniforms(shape, rectangle));
                    cached.shape = shape.clone();
                    cached.rectangle = rectangle;
                }
                Some(cached.element.clone().into())
            })
            .collect()
    }
}

fn enclosing_rect(bounds: Rect) -> Rectangle<i32, Logical> {
    let x = bounds.x.floor() as i32;
    let y = bounds.y.floor() as i32;
    Rectangle::new(
        (x, y).into(),
        (
            (bounds.x + bounds.width).ceil() as i32 - x,
            (bounds.y + bounds.height).ceil() as i32 - y,
        )
            .into(),
    )
}

fn uniforms(shape: &OverlayShape, rectangle: Rectangle<i32, Logical>) -> Vec<Uniform<'static>> {
    vec![
        Uniform::new(
            "area_size",
            (rectangle.size.w as f32, rectangle.size.h as f32),
        ),
        Uniform::new(
            "shape_rect",
            (
                (shape.bounds.x - f64::from(rectangle.loc.x)) as f32,
                (shape.bounds.y - f64::from(rectangle.loc.y)) as f32,
                shape.bounds.width as f32,
                shape.bounds.height as f32,
            ),
        ),
        Uniform::new(
            "ink",
            (
                shape.ink[0],
                shape.ink[1],
                shape.ink[2],
                shape.ink[3] * shape.opacity,
            ),
        ),
        Uniform::new(
            "shape",
            (
                shape.radius,
                shape.stroke,
                shape.fill,
                if shape.dashed { 3.0_f32 } else { 0.0_f32 },
            ),
        ),
    ]
}
