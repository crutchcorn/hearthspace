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

use super::{App, Gesture};

mod lasso;
mod shapes;
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
    action: Option<shapes::ActionBuffer>,
    lassos: BTreeMap<(i32, i32), lasso::LassoBuffer>,
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
        if output == self.group_output()
            && !self.window_group.selecting
            && !self.window_group.selected.is_empty()
        {
            let rect = self.group_action_rect();
            let hover = rect_contains(rect, self.pointer_location);
            let gathering = self.window_group.pending.is_some();
            let phase = if gathering {
                let epoch = cache.animation_epoch.get_or_insert_with(Instant::now);
                Some(((epoch.elapsed().as_secs_f64() % 1.2) / 1.2 * 40.0) as u8)
            } else {
                cache.animation_epoch = None;
                None
            };
            cache.action(renderer, &mut elements, rect, hover, phase);
            if let Some(error) = &self.window_group.error {
                cache.message(renderer, &mut elements, rect, usable, error, true);
            } else if hover {
                let help = if self.window_group.deferred_gather.is_some() {
                    "Release to gather"
                } else if gathering {
                    "Gathering similar windows…"
                } else {
                    "Gather similar windows (AI)"
                };
                cache.message(renderer, &mut elements, rect, usable, help, false);
            }
        }
        if let Some(Gesture::Select { lasso }) = &self.window_group.gesture {
            cache.lasso(renderer, &mut elements, lasso.points(), usable);
        } else {
            cache.lassos.clear();
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

impl Cache {
    fn action(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &mut Vec<HearthspaceRenderElement>,
        rect: Rectangle<i32, Logical>,
        hover: bool,
        phase: Option<u8>,
    ) {
        if !self
            .action
            .as_ref()
            .is_some_and(|action| action.matches(rect.size, hover, phase))
        {
            self.action = Some(shapes::ActionBuffer::new(rect.size, hover, phase));
        }
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            rect.loc.to_physical(1).to_f64(),
            &self.action.as_ref().unwrap().buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(element.into());
        }
    }

    fn lasso(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &mut Vec<HearthspaceRenderElement>,
        points: &[smithay::utils::Point<f64, Logical>],
        clip: Rectangle<i32, Logical>,
    ) {
        let key = (clip.loc.x, clip.loc.y);
        if !self
            .lassos
            .get(&key)
            .is_some_and(|lasso| lasso.matches(points, clip))
        {
            let Some(buffer) = lasso::LassoBuffer::new(points, clip) else {
                self.lassos.remove(&key);
                return;
            };
            self.lassos.insert(key, buffer);
        }
        let lasso = &self.lassos[&key];
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            lasso.rect.loc.to_physical(1).to_f64(),
            &lasso.buffer,
            None,
            Some(Rectangle::from_size(lasso.pixel_size.to_f64())),
            Some(lasso.rect.size),
            Kind::Unspecified,
        ) {
            elements.push(element.into());
        }
    }

    fn message(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &mut Vec<HearthspaceRenderElement>,
        anchor: Rectangle<i32, Logical>,
        output: Rectangle<i32, Logical>,
        text: &str,
        error: bool,
    ) {
        let width = (if error { 360 } else { 240 }).min(output.size.w.max(1));
        let padding = 8.min(width / 4);
        let lines = wrap_message(text, ((width - padding * 2).max(7) / 7) as usize);
        let height = (lines.len() as i32 * 18 + 12).min(output.size.h.max(1));
        let x = anchor
            .loc
            .x
            .clamp(output.loc.x, output.loc.x + output.size.w - width);
        let below = anchor.loc.y + anchor.size.h + 8;
        let y = if below + height <= output.loc.y + output.size.h {
            below
        } else {
            anchor.loc.y - height - 8
        }
        .clamp(output.loc.y, output.loc.y + output.size.h - height);
        let message = Rectangle::new((x, y).into(), (width, height).into());
        for (index, line) in lines.into_iter().enumerate() {
            let row_y = y + 6 + index as i32 * 18;
            if row_y + 18 > y + height {
                break;
            }
            self.label(
                renderer,
                elements,
                16 + index as u8,
                line,
                Rectangle::new(
                    (x + padding, row_y).into(),
                    ((width - padding * 2).max(1), 18).into(),
                ),
                12,
            );
        }
        self.solid(
            elements,
            (0, 36),
            message,
            if error {
                Color32F::new(0.20, 0.07, 0.08, 0.98)
            } else {
                Color32F::new(0.04, 0.06, 0.10, 0.98)
            },
        );
    }
}

fn wrap_message(text: &str, columns: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        for chunk in word.chars().collect::<Vec<_>>().chunks(columns.max(1)) {
            let line = lines.last_mut().unwrap();
            if line.chars().count() + chunk.len() + usize::from(!line.is_empty()) > columns {
                if lines.len() >= 8 {
                    let last = lines.last_mut().unwrap();
                    if last.chars().count() >= columns {
                        last.pop();
                    }
                    last.push('…');
                    return lines;
                }
                lines.push(String::new());
            }
            let line = lines.last_mut().unwrap();
            if !line.is_empty() {
                line.push(' ');
            }
            line.extend(chunk);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::wrap_message;

    #[test]
    fn model_errors_wrap_without_overflowing_or_unbounded_layout() {
        let lines = wrap_message(
            "Ollama unavailable. Start Ollama, then retry Gather similar windows.",
            20,
        );
        assert!(lines.len() > 1 && lines.iter().all(|line| line.chars().count() <= 20));
        assert_eq!(
            lines.join(" "),
            "Ollama unavailable. Start Ollama, then retry Gather similar windows."
        );
        let long = wrap_message(&"x".repeat(1000), 20);
        assert_eq!(long.len(), 8);
        assert!(long.last().unwrap().ends_with('…'));
    }
}
