use std::{collections::BTreeMap, time::Instant};

use smithay::{
    backend::renderer::{
        Color32F,
        element::{
            Kind,
            memory::MemoryRenderBufferRenderElement,
            solid::{SolidColorBuffer, SolidColorRenderElement},
        },
        gles::GlesRenderer,
    },
    utils::{Logical, Rectangle},
};

use super::{
    App, Gesture,
    model::{ACTION_WIDTH, selection_rect},
};
use crate::{
    compositor::{
        navigation::usable_output, overview::labels::LabelBuffer,
        rendering::HearthspaceRenderElement,
    },
    geometry::rect_contains,
};

#[derive(Default)]
pub(super) struct Cache {
    solids: BTreeMap<(u64, u8), SolidColorBuffer>,
    labels: BTreeMap<u8, LabelBuffer>,
    animation_epoch: Option<Instant>,
}

impl App {
    pub(in crate::compositor) fn collect_group_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
    ) -> Vec<HearthspaceRenderElement> {
        let mut elements = Vec::new();
        if !self.group_visible_on_output(output) {
            return elements;
        }
        let mut cache = std::mem::take(&mut self.window_group.cache);
        cache.solids.retain(|(id, _), _| {
            *id == 0
                || self.window_group.selected.contains(id)
                || self.window_group.keyboard_target == Some(*id)
        });
        let usable = usable_output(output);
        let accent = Color32F::new(0.46, 0.67, 1.0, 1.0);
        if output == self.group_output() {
            let rect = self.group_toolbar_rect();
            let selected = self.window_group.selected.len();
            let noun = if selected == 1 { "window" } else { "windows" };
            let gathering = self.window_group.pending.is_some();
            let label = if selected == 0 {
                "Click or drag to select windows".into()
            } else if self.window_group.deferred_gather.is_some() {
                format!("{selected} {noun} · Release to gather")
            } else if gathering {
                format!("{selected} {noun} · Gathering…")
            } else {
                format!("{selected} {noun} · Drag to move")
            };
            let body_width = if selected == 0 {
                rect.size.w
            } else {
                (rect.size.w - ACTION_WIDTH * 2).max(1)
            };
            cache.label(
                renderer,
                &mut elements,
                0,
                label,
                Rectangle::new(
                    (rect.loc.x + 12, rect.loc.y + 6).into(),
                    ((body_width - 20).max(1), 30).into(),
                ),
                14,
            );
            if selected > 0 {
                let gather = Rectangle::new(
                    (rect.loc.x + rect.size.w - ACTION_WIDTH * 2, rect.loc.y).into(),
                    (ACTION_WIDTH, rect.size.h).into(),
                );
                let clear = Rectangle::new(
                    (rect.loc.x + rect.size.w - ACTION_WIDTH, rect.loc.y).into(),
                    (ACTION_WIDTH, rect.size.h).into(),
                );
                // Two offset window outlines are the gather action's icon.
                cache.outline(
                    &mut elements,
                    0,
                    20,
                    Rectangle::new(
                        (gather.loc.x + 12, gather.loc.y + 11).into(),
                        (16, 16).into(),
                    ),
                    2,
                    accent,
                    usable,
                );
                cache.outline(
                    &mut elements,
                    0,
                    24,
                    Rectangle::new(
                        (gather.loc.x + 18, gather.loc.y + 17).into(),
                        (16, 16).into(),
                    ),
                    2,
                    accent,
                    usable,
                );
                cache.label(
                    renderer,
                    &mut elements,
                    1,
                    "×".into(),
                    Rectangle::new((clear.loc.x + 13, clear.loc.y + 6).into(), (24, 30).into()),
                    24,
                );
                for (key, button) in [(30, gather), (31, clear)] {
                    if rect_contains(button, self.pointer_location) {
                        cache.solid(
                            &mut elements,
                            (0, key),
                            button,
                            Color32F::new(0.14, 0.21, 0.34, 1.0),
                        );
                    }
                }
                if rect_contains(gather, self.pointer_location)
                    || rect_contains(clear, self.pointer_location)
                {
                    let text = if rect_contains(gather, self.pointer_location) {
                        "Gather similar windows (AI)"
                    } else {
                        "Clear window group (Esc)"
                    };
                    let tooltip = Rectangle::new(
                        (
                            rect.loc.x,
                            (rect.loc.y + rect.size.h + 6).min(usable.loc.y + usable.size.h - 28),
                        )
                            .into(),
                        (rect.size.w, 28).into(),
                    );
                    cache.label(renderer, &mut elements, 2, text.into(), tooltip, 13);
                    cache.solid(
                        &mut elements,
                        (0, 32),
                        tooltip,
                        Color32F::new(0.04, 0.06, 0.10, 0.96),
                    );
                }
            }
            if gathering {
                let epoch = cache.animation_epoch.get_or_insert_with(Instant::now);
                let phase = epoch.elapsed().as_secs_f64() % 1.2 / 1.2;
                let length = (body_width / 3).max(1);
                let x = rect.loc.x + ((body_width - length) as f64 * phase) as i32;
                cache.solid(
                    &mut elements,
                    (0, 35),
                    Rectangle::new((x, rect.loc.y + rect.size.h - 4).into(), (length, 3).into()),
                    accent,
                );
            } else {
                cache.animation_epoch = None;
            }
            if let Some(error) = &self.window_group.error {
                let message = Rectangle::new(
                    (
                        rect.loc.x,
                        (rect.loc.y + rect.size.h + 8).min(usable.loc.y + usable.size.h - 30),
                    )
                        .into(),
                    (rect.size.w, 30).into(),
                );
                cache.label(renderer, &mut elements, 3, error.clone(), message, 12);
                cache.solid(
                    &mut elements,
                    (0, 36),
                    message,
                    Color32F::new(0.20, 0.07, 0.08, 0.97),
                );
            }
            cache.outline(
                &mut elements,
                0,
                40,
                rect,
                1,
                Color32F::new(0.28, 0.39, 0.56, 1.0),
                usable,
            );
            cache.solid(
                &mut elements,
                (0, 44),
                rect,
                Color32F::new(0.045, 0.065, 0.105, 0.98),
            );
        }
        if let Some(Gesture::Select {
            origin, current, ..
        }) = &self.window_group.gesture
        {
            let rect = selection_rect(*origin, *current);
            cache.outline(&mut elements, 0, 50, rect, 2, accent, usable);
            if let Some(rect) = rect.intersection(usable) {
                cache.solid(
                    &mut elements,
                    (0, 54),
                    rect,
                    Color32F::new(0.06, 0.10, 0.19, 0.18),
                );
            }
        }
        for (index, window) in self.windows.iter().enumerate() {
            if !self.group_window_eligible(index) {
                continue;
            }
            let selected = self.window_group.selected.contains(&window.id);
            let focused =
                self.window_group.selecting && self.window_group.keyboard_target == Some(window.id);
            if selected || focused {
                let color = if focused {
                    Color32F::new(1.0, 0.80, 0.38, 1.0)
                } else {
                    accent
                };
                cache.outline(
                    &mut elements,
                    window.id,
                    0,
                    self.window_screen_bounds(index),
                    3,
                    color,
                    usable,
                );
            }
        }
        self.window_group.cache = cache;
        elements
    }
}

impl Cache {
    fn solid(
        &mut self,
        elements: &mut Vec<HearthspaceRenderElement>,
        key: (u64, u8),
        rect: Rectangle<i32, Logical>,
        color: Color32F,
    ) {
        if rect.size.w <= 0 || rect.size.h <= 0 {
            return;
        }
        let buffer = self.solids.entry(key).or_default();
        buffer.update(rect.size, color);
        elements.push(
            SolidColorRenderElement::from_buffer(
                buffer,
                rect.loc.to_physical(1),
                1.0,
                1.0,
                Kind::Unspecified,
            )
            .into(),
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "shared primitive provides stable damage IDs and output clipping"
    )]
    fn outline(
        &mut self,
        elements: &mut Vec<HearthspaceRenderElement>,
        id: u64,
        key: u8,
        rect: Rectangle<i32, Logical>,
        thickness: i32,
        color: Color32F,
        clip: Rectangle<i32, Logical>,
    ) {
        for (part, (x, y, w, h)) in [
            (rect.loc.x, rect.loc.y, rect.size.w, thickness),
            (
                rect.loc.x,
                rect.loc.y + rect.size.h - thickness,
                rect.size.w,
                thickness,
            ),
            (rect.loc.x, rect.loc.y, thickness, rect.size.h),
            (
                rect.loc.x + rect.size.w - thickness,
                rect.loc.y,
                thickness,
                rect.size.h,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(rect) = Rectangle::new((x, y).into(), (w, h).into()).intersection(clip) {
                self.solid(elements, (id, key + part as u8), rect, color);
            }
        }
    }

    fn label(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &mut Vec<HearthspaceRenderElement>,
        key: u8,
        text: String,
        rect: Rectangle<i32, Logical>,
        font_size: u32,
    ) {
        if !self
            .labels
            .get(&key)
            .is_some_and(|label| label.matches(&text, rect.size, font_size))
        {
            self.labels
                .insert(key, LabelBuffer::new(text, rect.size, font_size));
        }
        let buffer = &self.labels[&key].buffer;
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            rect.loc.to_physical(1).to_f64(),
            buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(element.into());
        }
    }
}
