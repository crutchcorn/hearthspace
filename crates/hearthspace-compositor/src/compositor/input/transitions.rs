//! Named mode transitions retain their distinct gesture/focus ownership rules.
use crate::compositor::{App, switcher::SwitcherSource};

impl App {
    /// Search consumes any pointer/touch selection and saves the prior app focus.
    pub(in crate::compositor) fn prepare_canvas_search_entry(&mut self) -> (bool, Option<u64>) {
        self.cancel_input_gestures();
        let desktop_release = self.clear_desktop_for_canvas_search();
        self.clear_window_group();
        self.advance_viewport_animation();
        self.viewport_animation = None;
        self.cancel_pending_search();
        let previous_focus = self.focused_normal_window_id;
        self.clear_keyboard_focus();
        (desktop_release, previous_focus)
    }

    /// Gesture switching keeps its live recognizer; keyboard switching cancels it.
    pub(in crate::compositor) fn prepare_window_switcher_entry(&mut self, source: SwitcherSource) {
        self.cancel_canvas_pan();
        if source == SwitcherSource::Keyboard {
            self.interrupt_pinch_zoom();
        }
        self.cancel_pending_search();
        self.cancel_desktop_gesture();
        self.clear_window_group();
        if self.active_shell_panel_index().is_some() {
            self.dismiss_active_shell_panel();
            self.restore_focus_after_shell_panel();
        }
        self.advance_viewport_animation();
        self.viewport_animation = None;
    }

    /// Revoke modal feature state before the seat reset drains held input.
    /// Never unset Smithay grabs here: recovery must deliver their releases first.
    pub(super) fn reset_interaction_modes(&mut self) {
        self.disarm_canvas_pan();
        self.cancel_presentation_reveals();
        self.gtk_shell.reset_input();
        self.clear_canvas_peek_motion();
        // A complete lock/VT transition can occur between layout polls. Cancel
        // shell-side pending/queued openings as well as compositor input state.
        self.invalidate_shell_popups();
        self.reset_power_confirmation();
        self.cancel_pending_shell_click();
        self.cancel_titlebar_pointer();
        self.reset_focused_shell_peek();
        self.reset_window_group_input();
        self.reset_desktop_input();
        self.reset_layout_editor();
        self.reset_layout_pointer_captures();
        self.reset_canvas_search();
        self.reset_favorite_input();
        // Drain queued search modifiers before releasing held keys; otherwise
        // recovery can buffer its own releases or replay a chord on return.
        self.cancel_pending_search();
        self.control_events.clear();
        self.shortcuts.cancel_device_repeat();
        self.dismiss_control_osds();
        self.reset_window_switcher();
        self.switcher_input.clear();
        self.cancel_input_gestures();
        self.viewport_animation = None;
        self.drag = None;
        self.finish_resize();
    }
}
