use smithay::utils::{Logical, Point, Rectangle};

use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn window_surface_screen_origin(
        &self,
        index: usize,
    ) -> Point<f64, Logical> {
        self.window_projection(index).origin
    }

    pub(in crate::compositor) fn window_screen_bounds(
        &self,
        index: usize,
    ) -> Rectangle<i32, Logical> {
        self.window_projection(index).bounds
    }

    pub(in crate::compositor) fn base_window_screen_bounds(
        &self,
        index: usize,
    ) -> Rectangle<i32, Logical> {
        self.base_window_projection(index).bounds
    }

    pub(in crate::compositor) fn fullscreen_on_output(
        &self,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        self.windows.iter().enumerate().any(|(index, window)| {
            window.mapped
                && window.presentation.fullscreen
                && self.presentation_output_rect(index) == Some(output)
        })
    }

    pub(in crate::compositor) fn shell_visible_on_output(
        &self,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        if self.canvas_preview_active() {
            return false;
        }
        // Launcher activation always provides an escape from fullscreen.
        if self
            .keyboard
            .current_focus()
            .and_then(|focus| self.window_index_for_surface(focus.surface()))
            .is_some_and(|index| self.windows[index].kind.is_shell_chrome())
        {
            return true;
        }
        !self
            .windows
            .iter()
            .enumerate()
            .rev()
            .find(|(index, window)| {
                window.mapped
                    && self.is_navigable_window(*index)
                    && self
                        .normal_window_screen_rect(*index)
                        .is_some_and(|rect| rect.overlaps(output))
            })
            .is_some_and(|(index, _)| {
                self.presentation_anchor(index).is_some_and(|anchor| {
                    let state = &self.windows[anchor].presentation;
                    self.presentation_output_rect(anchor) == Some(output)
                        && (state.fullscreen
                            || (state.focused_view
                                && !self.focused_shell_visible_on_output(output)))
                })
            })
    }

    pub(in crate::compositor) fn window_visible_on_output(
        &self,
        index: usize,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        if !self.windows[index].mapped {
            return false;
        }
        if self.is_power_confirmation(index) {
            return self.power_confirmation_active()
                && self.power_confirmation_surface(index)
                && self.power_confirmation_on_output(output);
        }
        if self.is_layout_surface(index) {
            return self.layout_surface_visible(index);
        }
        if self.is_window_switcher_caption(index) {
            return self.window_switcher_caption_visible(index);
        }
        if self.is_canvas_search_panel(index) {
            return self.canvas_search_panel_allowed(index);
        }
        if self.canvas_search_result_identity(index).is_some() {
            return self.canvas_search_result_visible(index);
        }
        if self.canvas_preview_active() && !self.windows[index].kind.is_shell_chrome() {
            return true;
        }
        if self.windows[index].kind == super::super::ManagedWindowKind::ControlOsd {
            return !self.session_locked();
        }
        if self.windows[index].kind.is_shell_chrome() {
            return self.shell_visible_on_output(output);
        }
        self.window_presentation_output(index)
            .is_none_or(|target| target == output)
    }
}
