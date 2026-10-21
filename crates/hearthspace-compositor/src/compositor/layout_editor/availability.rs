//! Device absence is live availability, not a destructive edit of the saved frame layout.
use super::App;
impl App {
    pub(super) fn set_frame_widget_available(
        &mut self,
        output: String,
        id: String,
        available: bool,
    ) -> Result<(), String> {
        if !self.layout_editor.layouts.contains_key(&output)
            || !hearthspace_common::frame_layout::WIDGETS.contains(&id.as_str())
        {
            return Err("unknown frame widget".into());
        }
        let key = (output, id);
        let changed = if available {
            self.layout_editor.unavailable.remove(&key)
        } else {
            self.layout_editor.unavailable.insert(key)
        };
        if changed {
            self.reconcile_layout_editor();
            self.layout_changed();
        }
        Ok(())
    }
    pub(super) fn frame_widget_available(&self, output: &str, id: &str) -> bool {
        !self
            .layout_editor
            .unavailable
            .contains(&(output.into(), id.into()))
    }
}
