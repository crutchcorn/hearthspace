//! Input and rendering must agree when fullscreen content hides group chrome.
use smithay::utils::{Logical, Rectangle};

use super::App;

impl App {
    pub(super) fn group_visible_on_output(&self, output: Rectangle<i32, Logical>) -> bool {
        self.group_active()
            && !self.session_locked()
            && !self.window_switcher_active()
            // A raised floating app or focused shell makes the canvas controls
            // visible again even when a fullscreen app remains behind it.
            && self.shell_visible_on_output(output)
    }

    pub(in crate::compositor) fn group_pointer_visible(&self) -> bool {
        self.output_logical_rects()
            .into_iter()
            .find(|output| output.contains(self.pointer_location.to_i32_round()))
            .is_some_and(|output| self.group_visible_on_output(output))
    }

    /// Compute before Smithay takes its keyboard lock for shortcut filtering.
    pub(in crate::compositor) fn group_keyboard_visible(&self) -> bool {
        let output = self
            .keyboard
            .current_focus()
            .and_then(|focus| self.window_index_for_surface(focus.surface()))
            .and_then(|index| {
                self.window_presentation_output(index).or_else(|| {
                    let bounds = self.normal_window_screen_rect(index)?;
                    self.output_logical_rects()
                        .into_iter()
                        .find(|output| output.overlaps(bounds))
                })
            })
            .unwrap_or_else(|| self.group_output());
        self.group_visible_on_output(output)
    }
}
