use smithay::{
    backend::renderer::gles::GlesRenderer,
    utils::{Logical, Rectangle},
};

use super::{SwitcherSource, layout};
use crate::compositor::{App, rendering::HearthspaceRenderElement};

impl App {
    pub(in crate::compositor) fn collect_window_switcher_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output: Rectangle<i32, Logical>,
    ) -> Vec<HearthspaceRenderElement> {
        if !self.window_switcher_active() || output != self.window_switcher_output() {
            return Vec::new();
        }
        let session = self
            .window_switcher
            .session
            .as_ref()
            .expect("active switcher");
        let strip = self.window_switcher_strip();
        let selected = session.selected;
        let position = session
            .ids
            .iter()
            .position(|id| *id == selected)
            .unwrap_or(0)
            + 1;
        let release = match session.source {
            SwitcherSource::Keyboard => "Release Alt to switch",
            SwitcherSource::Gesture => "Lift fingers to switch",
        };
        let caption = format!(
            "{position} of {} windows · {release} · Esc to cancel",
            session.ids.len()
        );
        let mut cache = std::mem::take(&mut self.window_switcher.cache);
        let mut elements = Vec::new();
        self.collect_window_cards(
            renderer,
            &mut elements,
            &mut cache,
            &strip.items,
            Some(selected),
            1.0,
        );
        cache.caption(renderer, &mut elements, caption, strip.caption);
        cache.panel(&mut elements, strip.panel);
        self.window_switcher.cache = cache;
        elements
    }

    pub(in crate::compositor) fn window_switcher_output(&self) -> Rectangle<i32, Logical> {
        self.window_switcher
            .session
            .as_ref()
            .and_then(|session| self.output_logical_rect_for_name(session.output.as_deref()))
            .unwrap_or_else(|| self.primary_output_logical_rect())
    }

    pub(super) fn window_switcher_strip(&self) -> layout::Strip {
        let session = self
            .window_switcher
            .session
            .as_ref()
            .expect("active switcher");
        let windows: Vec<_> = session
            .ids
            .iter()
            .filter_map(|id| {
                self.windows
                    .iter()
                    .position(|window| window.id == *id)
                    .map(|index| (*id, self.window_geometry_size(index)))
            })
            .collect();
        layout::arrange(self.window_switcher_output(), &windows, session.selected)
    }
}
