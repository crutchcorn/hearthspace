use std::collections::BTreeMap;

use smithay::{
    backend::renderer::{
        element::{
            Kind,
            memory::MemoryRenderBufferRenderElement,
            solid::{SolidColorBuffer, SolidColorRenderElement},
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            utils::{CropRenderElement, RescaleRenderElement},
        },
        gles::GlesRenderer,
    },
    utils::{Logical, Physical, Point, Rectangle},
};

use crate::compositor::appearance::color;
use hearthspace_ui_components::Palette;

use super::{APPEAR_DURATION, labels::LabelBuffer, layout::Item};
use crate::compositor::{App, rendering::HearthspaceRenderElement, windows::window_title};

#[derive(Default)]
pub(in crate::compositor) struct Cache {
    palette: Palette,
    backgrounds: BTreeMap<(i32, i32), SolidColorBuffer>,
    cards: BTreeMap<u64, (SolidColorBuffer, SolidColorBuffer)>,
    labels: BTreeMap<u64, LabelBuffer>,
    headings: BTreeMap<u8, LabelBuffer>,
}

impl App {
    pub(in crate::compositor) fn collect_overview_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
    ) -> Vec<HearthspaceRenderElement> {
        let mut elements = self.software_cursor_elements(renderer, output);
        let Some(session) = self.overview.session.as_ref() else {
            return elements;
        };
        let progress = (session.opened_at.elapsed().as_secs_f32() / APPEAR_DURATION.as_secs_f32())
            .clamp(0.0, 1.0);
        let alpha = 1.0 - (1.0 - progress).powi(3);
        let selected = session.selected;
        let items = self.overview_items();
        let mut cache = std::mem::take(&mut self.overview.cache);
        cache.palette = self.palette();
        if output == self.overview_output() {
            self.collect_window_cards(renderer, &mut elements, &mut cache, &items, selected, alpha);
            let left = output.loc.x + 32.min(output.size.w / 10);
            let width = (output.size.w - (left - output.loc.x) * 2).max(1);
            push_label(
                renderer,
                &mut elements,
                &mut cache.headings,
                0,
                "Canvas overview".into(),
                Rectangle::new((left, output.loc.y + 18).into(), (width, 34).into()),
                24,
                1.0,
                cache.palette.text,
            );
            push_label(
                renderer,
                &mut elements,
                &mut cache.headings,
                1,
                "Select a window · Esc or pinch outward to return".into(),
                Rectangle::new((left, output.loc.y + 54).into(), (width, 24).into()),
                14,
                1.0,
                cache.palette.text,
            );
            if items.is_empty() {
                push_label(
                    renderer,
                    &mut elements,
                    &mut cache.headings,
                    2,
                    "No open windows".into(),
                    Rectangle::new(
                        (left, output.loc.y + output.size.h / 2).into(),
                        (width, 40).into(),
                    ),
                    22,
                    1.0,
                    cache.palette.text,
                );
            }
        }
        let background = cache
            .backgrounds
            .entry((output.loc.x, output.loc.y))
            .or_default();
        background.update(output.size, color(cache.palette.surface.with_alpha(0.96)));
        elements.push(
            SolidColorRenderElement::from_buffer(
                background,
                output.loc.to_physical(1),
                1.0,
                1.0,
                Kind::Unspecified,
            )
            .into(),
        );
        self.overview.cache = cache;
        elements
    }

    pub(in crate::compositor) fn collect_window_cards(
        &self,
        renderer: &mut GlesRenderer,
        elements: &mut Vec<HearthspaceRenderElement>,
        cache: &mut Cache,
        items: &[Item],
        selected: Option<u64>,
        alpha: f32,
    ) {
        cache.palette = self.palette();
        cache
            .cards
            .retain(|id, _| items.iter().any(|item| item.id == *id));
        cache
            .labels
            .retain(|id, _| items.iter().any(|item| item.id == *id));
        for item in items {
            let Some(index) = self.windows.iter().position(|window| window.id == item.id) else {
                continue;
            };
            let Some(surface) = self.windows[index].surface.wl_surface() else {
                continue;
            };
            let title = window_title(&self.windows[index].surface)
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| "Untitled window".into());
            push_label(
                renderer,
                elements,
                &mut cache.labels,
                item.id,
                title,
                item.label,
                14,
                alpha,
                cache.palette.text,
            );

            let geometry_loc = self.window_surface_geometry_loc(index);
            let origin = Point::<i32, Physical>::from((
                item.preview.loc.x - (f64::from(geometry_loc.x) * item.scale).round() as i32,
                item.preview.loc.y - (f64::from(geometry_loc.y) * item.scale).round() as i32,
            ));
            let surfaces: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    renderer,
                    &surface,
                    origin,
                    1.0,
                    alpha,
                    Kind::Unspecified,
                );
            for surface in surfaces {
                let scaled = RescaleRenderElement::from_element(surface, origin, item.scale);
                // Client shadows and subsurfaces cannot cover a neighboring card.
                if let Some(cropped) =
                    CropRenderElement::from_element(scaled, 1.0, item.preview.to_physical(1))
                {
                    elements.push(HearthspaceRenderElement::from(cropped));
                }
            }
            let (border, body) = cache.cards.entry(item.id).or_default();
            let inner = Rectangle::new(
                item.rect.loc + Point::from((2, 2)),
                ((item.rect.size.w - 4).max(1), (item.rect.size.h - 4).max(1)).into(),
            );
            body.update(inner.size, color(cache.palette.surface_raised));
            elements.push(
                SolidColorRenderElement::from_buffer(
                    body,
                    inner.loc.to_physical(1),
                    1.0,
                    alpha,
                    Kind::Unspecified,
                )
                .into(),
            );
            let border_color = if selected == Some(item.id) {
                color(cache.palette.accent)
            } else {
                color(cache.palette.border)
            };
            border.update(item.rect.size, border_color);
            elements.push(
                SolidColorRenderElement::from_buffer(
                    border,
                    item.rect.loc.to_physical(1),
                    1.0,
                    alpha,
                    Kind::Unspecified,
                )
                .into(),
            );
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "shared label cache helper keeps card and heading rendering identical"
)]
fn push_label<K: Ord + Copy>(
    renderer: &mut GlesRenderer,
    elements: &mut Vec<HearthspaceRenderElement>,
    cache: &mut BTreeMap<K, LabelBuffer>,
    key: K,
    text: String,
    rect: Rectangle<i32, Logical>,
    font_size: u32,
    alpha: f32,
    color: xilem::Color,
) {
    if !cache
        .get(&key)
        .is_some_and(|label| label.matches(&text, rect.size, font_size, color))
    {
        cache.insert(key, LabelBuffer::new(text, rect.size, font_size, color));
    }
    let buffer = &cache.get(&key).expect("cached label").buffer;
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        rect.loc.to_physical(1).to_f64(),
        buffer,
        Some(alpha),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(element.into());
    }
}
