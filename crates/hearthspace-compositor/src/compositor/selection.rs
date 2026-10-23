//! Cached rounded selection shape shared by desktop marquees and search previews.
use smithay::{
    backend::renderer::{
        ContextId, Renderer,
        element::Kind,
        gles::{
            GlesRenderer, GlesTexture, Uniform, UniformName, UniformType,
            element::PixelShaderElement,
        },
    },
    utils::{Logical, Rectangle},
};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub(in crate::compositor) struct Style {
    pub ink: [f32; 4],
    pub radius: f32,
    pub stroke: f32,
    pub fill: f32,
}

#[derive(Default)]
pub(in crate::compositor) struct Cache {
    renderers: HashMap<ContextId<GlesTexture>, Option<Cached>>,
}

struct Cached {
    element: PixelShaderElement,
    rectangle: Rectangle<i32, Logical>,
    style: Style,
}

impl Cache {
    pub fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        rectangle: Rectangle<i32, Logical>,
        style: Style,
    ) -> Option<PixelShaderElement> {
        let cached = self
            .renderers
            .entry(renderer.context_id())
            .or_insert_with(|| {
                let shader = renderer
                    .compile_custom_pixel_shader(
                        include_str!("selection.frag"),
                        &[
                            UniformName::new("area_size", UniformType::_2f),
                            UniformName::new("ink", UniformType::_4f),
                            UniformName::new("shape", UniformType::_3f),
                        ],
                    )
                    .map_err(|error| tracing::warn!(%error, "desktop selection shader unavailable"))
                    .ok()?;
                Some(Cached {
                    element: PixelShaderElement::new(
                        shader,
                        rectangle,
                        None,
                        1.0,
                        uniforms(rectangle, style),
                        Kind::Unspecified,
                    ),
                    rectangle,
                    style,
                })
            })
            .as_mut()?;
        if cached.rectangle != rectangle || cached.style != style {
            cached.element.resize(rectangle, None);
            cached.element.update_uniforms(uniforms(rectangle, style));
            cached.rectangle = rectangle;
            cached.style = style;
        }
        Some(cached.element.clone())
    }
}

fn uniforms(rectangle: Rectangle<i32, Logical>, style: Style) -> Vec<Uniform<'static>> {
    vec![
        Uniform::new(
            "area_size",
            (rectangle.size.w as f32, rectangle.size.h as f32),
        ),
        Uniform::new(
            "ink",
            (style.ink[0], style.ink[1], style.ink[2], style.ink[3]),
        ),
        Uniform::new("shape", (style.radius, style.stroke, style.fill)),
    ]
}
