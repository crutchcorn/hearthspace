//! Website focused view retains native chrome and layers over canvas maximization.
use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn toggle_window_focus_view(
        &mut self,
        id: u64,
    ) -> Result<(), String> {
        self.check_navigation_available()?;
        if self.toggle_frame_window_reveal(id)? {
            return Ok(());
        }
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or_else(|| format!("unknown window {id}"))?;
        if !self.is_navigable_window(index) {
            return Err("window is not a mapped application window".into());
        }
        let focused = !self.windows[index].presentation.focused_view;
        self.update_window_presentation_with_focus(
            id,
            Some(false),
            Some(true),
            Some(focused),
            None,
        )?;
        self.focus_presentation_window(id)
    }

    pub(in crate::compositor) fn toggle_window_canvas_maximize(
        &mut self,
        id: u64,
    ) -> Result<(), String> {
        self.check_navigation_available()?;
        if self.toggle_frame_window_reveal(id)? {
            return Ok(());
        }
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or_else(|| format!("unknown window {id}"))?;
        if !self.is_navigable_window(index) {
            return Err("window is not a mapped application window".into());
        }
        let state = &self.windows[index].presentation;
        let maximize = state.focused_view || !state.maximized;
        self.update_window_presentation_with_focus(
            id,
            Some(false),
            Some(maximize),
            Some(false),
            None,
        )?;
        self.focus_presentation_window(id)
    }

    pub(super) fn focus_presentation_window(&mut self, id: u64) -> Result<(), String> {
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or_else(|| format!("unknown window {id}"))?;
        if !self.windows[index].presentation.pinned() {
            return self.focus_window(id);
        }
        // A client may still have its old full-size buffer here. Revealing that
        // buffer would pan the canvas before its bounded configure is committed.
        let index = self.raise_window(index);
        if let Some(surface) = self.windows[index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(index, surface);
        }
        self.request_redraw();
        Ok(())
    }

    pub(super) fn refresh_focused_output_bounds(&mut self, index: usize) {
        if !self.windows[index].presentation.focused_view {
            self.windows[index].presentation.focused_output_bounds = None;
            return;
        }
        let bounds = self.presentation_output_rect(index);
        if bounds == self.windows[index].presentation.focused_output_bounds {
            return;
        }
        let output = self.windows[index]
            .presentation
            .output
            .clone()
            .unwrap_or_default();
        let maximized = self.maximized_canvas_bounds(index, &output);
        let state = &mut self.windows[index].presentation;
        state.focused_output_bounds = bounds;
        state.maximized_bounds = Some(maximized);
    }

    pub(in crate::compositor) fn window_focus_view_active(&self) -> bool {
        self.windows
            .iter()
            .any(|window| window.mapped && window.presentation.focused_view)
    }

    pub(in crate::compositor) fn leave_window_focus_views(&mut self) {
        for index in 0..self.windows.len() {
            self.leave_window_focus_view(index);
        }
    }

    // Called under Smithay's keyboard mutex: use the supplied focus, never query
    // the keyboard or set focus recursively. Dialogs and shell controls keep the view.
    pub(in crate::compositor) fn window_focus_view_changed(&mut self, focused: Option<usize>) {
        let Some(focused) = focused.filter(|index| self.is_navigable_window(*index)) else {
            return;
        };
        for index in 0..self.windows.len() {
            if index != focused && !self.transient_descendant_of(focused, self.windows[index].id) {
                self.leave_window_focus_view(index);
            }
        }
    }

    fn leave_window_focus_view(&mut self, index: usize) {
        if !self.windows[index].presentation.focused_view {
            return;
        }
        self.windows[index].presentation.focused_view = false;
        self.windows[index].presentation.focused_output_bounds = None;
        self.configure_window_presentation(index);
        self.request_redraw();
    }
}
