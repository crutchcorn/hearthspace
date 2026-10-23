//! Website focused view retains native chrome and layers over canvas maximization.
use crate::compositor::App;

impl App {
    /// The titlebar button restores floating geometry on its second activation.
    /// The keyboard focused-view toggle retains its separate layered behavior.
    pub(in crate::compositor) fn toggle_window_maximize_button(
        &mut self,
        id: u64,
    ) -> Result<(), String> {
        let window = self
            .windows
            .iter()
            .find(|window| window.id == id)
            .ok_or_else(|| format!("unknown window {id}"))?;
        if window.presentation.focused_view {
            self.user_window_presentation(id, Some(false), Some(false))
        } else {
            self.toggle_window_focus_view(id)
        }
    }

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
        self.toggle_canvas_maximize_for_input(id, false)
    }

    /// The GTK protocol has validated the seat, serial and input ownership.
    pub(in crate::compositor) fn toggle_window_canvas_maximize_from_titlebar(
        &mut self,
        id: u64,
    ) -> Result<(), String> {
        self.toggle_canvas_maximize_for_input(id, true)
    }

    fn toggle_canvas_maximize_for_input(
        &mut self,
        id: u64,
        preserve_pointer: bool,
    ) -> Result<(), String> {
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
        self.focus_presentation_window_for_input(id, preserve_pointer)
    }

    pub(in crate::compositor) fn focus_presentation_window(
        &mut self,
        id: u64,
    ) -> Result<(), String> {
        self.focus_presentation_window_for_input(id, false)
    }

    fn focus_presentation_window_for_input(
        &mut self,
        id: u64,
        preserve_pointer: bool,
    ) -> Result<(), String> {
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or_else(|| format!("unknown window {id}"))?;
        if !self.windows[index].presentation.pinned() {
            return if preserve_pointer {
                self.focus_window_with_existing_grab(id)
            } else {
                self.focus_window(id)
            };
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

    pub(in crate::compositor) fn window_focus_view_pending_for_focus(
        &self,
        focused: usize,
    ) -> bool {
        self.is_navigable_window(focused)
            && self.windows.iter().enumerate().any(|(index, window)| {
                self.is_navigable_window(index)
                    && window.presentation.focused_view_requested
                    && !window.presentation.focused_view
                    && self.window_focus_view_contains_focus(index, focused)
            })
    }

    fn window_focus_view_contains_focus(&self, index: usize, focused: usize) -> bool {
        index == focused || self.transient_descendant_of(focused, self.windows[index].id)
    }

    pub(in crate::compositor) fn suspend_window_focus_view(&mut self, index: usize) {
        if self.windows[index].presentation.focused_view {
            // Remapping may happen during another client's grab. Retain intent
            // but require actual activation again before taking over the output.
            self.windows[index].presentation.focused_view = false;
            self.windows[index].presentation.focused_output_bounds = None;
            self.configure_window_presentation(index);
        }
    }

    pub(in crate::compositor) fn leave_window_focus_views(&mut self) {
        for index in 0..self.windows.len() {
            self.windows[index].presentation.focused_view_requested = false;
            self.leave_window_focus_view(index);
        }
    }

    pub(super) fn window_owns_focused_presentation(&self, index: usize) -> bool {
        self.is_navigable_window(index)
            && (self.windows[index].presentation.focused_view
                || self.focused_normal_window_id.is_some_and(|id| {
                    self.windows[index].id == id
                        || self
                            .windows
                            .iter()
                            .position(|window| window.id == id)
                            .is_some_and(|focused| {
                                self.transient_descendant_of(focused, self.windows[index].id)
                            })
                }))
    }

    // Also called under Smithay's keyboard mutex. Freeze at the last presented
    // viewport rather than advancing animation or dispatching synthetic input:
    // both can query keyboard focus recursively through pointer hit-testing.
    pub(super) fn activate_window_focus_view(&mut self, index: usize) -> bool {
        let state = &mut self.windows[index].presentation;
        if !state.focused_view_requested || state.focused_view {
            return false;
        }
        state.focused_view = true;
        self.pinch_zoom = Default::default();
        self.viewport_animation = None;
        self.clear_window_group();
        self.clear_desktop_selection();
        self.reset_focused_shell_peek();
        true
    }

    // Called under Smithay's keyboard mutex: use the supplied focus, never query
    // the keyboard or set focus recursively. Dialogs and shell controls keep the view.
    pub(in crate::compositor) fn window_focus_view_changed(&mut self, focused: Option<usize>) {
        let Some(focused) = focused.filter(|index| self.is_navigable_window(*index)) else {
            return;
        };
        for index in 0..self.windows.len() {
            if !self.window_focus_view_contains_focus(index, focused) {
                self.leave_window_focus_view(index);
            } else if self.is_navigable_window(index) && self.activate_window_focus_view(index) {
                self.refresh_focused_output_bounds(index);
                self.configure_window_presentation(index);
                self.request_redraw();
            }
        }
    }

    fn leave_window_focus_view(&mut self, index: usize) {
        if !self.windows[index].presentation.focused_view {
            return;
        }
        self.windows[index].presentation.focused_view = false;
        self.windows[index].presentation.focused_view_requested = false;
        self.windows[index].presentation.focused_output_bounds = None;
        self.configure_window_presentation(index);
        self.request_redraw();
    }
}
