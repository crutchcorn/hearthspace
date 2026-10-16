//! Gathering explicitly returns a matched output-pinned app to canvas space.
use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn restore_window_for_group(
        &mut self,
        id: u64,
    ) -> Result<(), String> {
        self.update_window_presentation(id, Some(false), Some(false), None)?;
        if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
            // The group's fly-in owns placement; a delayed client configure
            // acknowledgment must not pan the viewport or reclaim focus.
            window.presentation.reveal_on_restore = false;
        }
        Ok(())
    }
}
