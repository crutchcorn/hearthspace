//! Explicit canvas zoom releases maximized windows while leaving fullscreen alone.
use smithay::utils::Rectangle;

use crate::compositor::{
    App,
    navigation::{reveal_offset, usable_output},
};

pub(in crate::compositor) struct RestoredMaximizedWindow {
    id: u64,
    output: Option<String>,
}

impl App {
    pub(in crate::compositor) fn restore_maximized_for_canvas_zoom(
        &mut self,
    ) -> Vec<RestoredMaximizedWindow> {
        if !self.navigation_available() {
            return Vec::new();
        }
        let restored: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, window)| {
                self.is_navigable_window(*index)
                    && window.presentation.maximized
                    && !window.presentation.fullscreen
            })
            .map(|(_, window)| RestoredMaximizedWindow {
                id: window.id,
                output: window.presentation.output.clone(),
            })
            .collect();
        // The saved floating position may be far outside the current view. Keep
        // the active app useful without moving its saved canvas position or
        // changing focus. Restoring other windows does not steal that view.
        let reveal = self.canvas_zoom_reveal_index(&restored).and_then(|index| {
            let anchor = self.presentation_anchor(index)?;
            restored
                .iter()
                .find(|window| window.id == self.windows[anchor].id)?;
            let mut canvas = self.window_canvas_rect(index);
            if let Some(size) = self.windows[index].presentation.restore_size {
                let chrome = canvas.size.h - self.window_geometry_size(index).h;
                canvas.size = (size.w, size.h + chrome).into();
            }
            let output = usable_output(self.presentation_output_rect(anchor)?);
            Some((canvas, output))
        });
        for window in &restored {
            if let Err(error) = self.update_window_presentation(window.id, None, Some(false), None)
            {
                tracing::warn!(%error, "failed to restore maximized window for canvas zoom");
                continue;
            }
            let index = self
                .windows
                .iter()
                .position(|candidate| candidate.id == window.id)
                .unwrap();
            // A later client commit must not refocus/reveal and cancel the zoom
            // gesture or animation which is already controlling this view.
            self.windows[index].presentation.reveal_on_restore = false;
        }
        if let Some((canvas, output)) = reveal {
            let screen = Rectangle::new(
                self.canvas_to_screen(canvas.loc.to_f64()),
                (
                    f64::from(canvas.size.w) * self.viewport_scale,
                    f64::from(canvas.size.h) * self.viewport_scale,
                )
                    .into(),
            );
            if !output.to_f64().contains_rect(screen) {
                self.viewport_offset = reveal_offset(canvas, output, self.viewport_scale);
            }
        }
        restored
    }

    fn canvas_zoom_reveal_index(&self, restored: &[RestoredMaximizedWindow]) -> Option<usize> {
        if let Some(id) = self.focused_normal_window_id {
            return self.windows.iter().position(|window| window.id == id);
        }
        let focus = self.keyboard.current_focus()?;
        let shell = &self.windows[self.window_index_for_surface(focus.surface())?];
        if !shell.kind.is_shell_chrome() {
            return None;
        }
        let output = self.output_logical_rect_for_name(shell.shell_output_name.as_deref())?;
        // A physical click on the bar correctly gives the shell keyboard
        // focus. Reveal the topmost maximized app on that bar's output without
        // transferring focus away from the shell's control.
        self.windows
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, window)| {
                (restored.iter().any(|restored| restored.id == window.id)
                    && self.presentation_output_rect(index) == Some(output))
                .then_some(index)
            })
    }

    pub(in crate::compositor) fn cancel_canvas_zoom_restore(
        &mut self,
        restored: Vec<RestoredMaximizedWindow>,
    ) {
        for window in restored {
            let Some(index) = self
                .windows
                .iter()
                .position(|candidate| candidate.id == window.id)
            else {
                continue;
            };
            if self.is_navigable_window(index)
                && !self.windows[index].presentation.pinned()
                && let Err(error) =
                    self.update_window_presentation(window.id, None, Some(true), window.output)
            {
                tracing::warn!(%error, "failed to restore maximization after cancelled canvas zoom");
            }
        }
    }
}
