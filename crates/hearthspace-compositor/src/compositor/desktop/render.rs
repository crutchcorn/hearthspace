//! One cached GPU primitive keeps a large marquee cheap while it follows the pointer.
use super::{App, Gesture, geometry};
use crate::compositor::rendering::HearthspaceRenderElement;
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

#[derive(Default)]
pub(super) struct Cache {
    renderers: HashMap<ContextId<GlesTexture>, Option<Cached>>,
}

struct Cached {
    element: PixelShaderElement,
    rectangle: Rectangle<i32, Logical>,
    ink: [f32; 4],
}

impl App {
    pub(in crate::compositor) fn desktop_selection_elements(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Vec<HearthspaceRenderElement> {
        let Some(Gesture::Marquee { start, current, .. }) = &self.desktop.gesture else {
            return Vec::new();
        };
        let rectangle = geometry::rectangle(
            self.canvas_to_screen(*start),
            self.canvas_to_screen(*current),
        )
        .to_i32_up();
        if rectangle.is_empty() {
            return Vec::new();
        }
        let ink = self.palette().accent.components;
        self.desktop
            .selection_cache
            .element(renderer, rectangle, ink)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}

impl Cache {
    fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        rectangle: Rectangle<i32, Logical>,
        ink: [f32; 4],
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
                        uniforms(rectangle, ink),
                        Kind::Unspecified,
                    ),
                    rectangle,
                    ink,
                })
            })
            .as_mut()?;
        if cached.rectangle != rectangle || cached.ink != ink {
            cached.element.resize(rectangle, None);
            cached.element.update_uniforms(uniforms(rectangle, ink));
            cached.rectangle = rectangle;
            cached.ink = ink;
        }
        Some(cached.element.clone())
    }
}

fn uniforms(rectangle: Rectangle<i32, Logical>, ink: [f32; 4]) -> Vec<Uniform<'static>> {
    vec![
        Uniform::new(
            "area_size",
            (rectangle.size.w as f32, rectangle.size.h as f32),
        ),
        Uniform::new("ink", (ink[0], ink[1], ink[2], ink[3])),
    ]
}
