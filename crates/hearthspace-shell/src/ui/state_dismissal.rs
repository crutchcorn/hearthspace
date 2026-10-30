//! Dismissal cancels queued openings even before a panel becomes visible.
use super::ShellState;
use crate::ui::popup::PopupKind;

impl ShellState {
    pub(in crate::ui) fn dismiss_controls(&mut self, output_key: &str) {
        self.frame_layout
            .intents
            .cancel_kind(output_key, PopupKind::Controls);
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.show_controls = false;
            output.tool_error = None;
            output.clock_formats.open = false;
        }
    }

    pub(in crate::ui) fn dismiss_launcher(&mut self, output_key: &str) {
        self.frame_layout.intents.cancel_output(output_key);
        crate::ui::favorite_actions::cancel(self, output_key);
        self.canvas_dragging = false;
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.dismiss_panels();
        }
    }

    pub(in crate::ui) fn dismiss_search(&mut self, output_key: &str) {
        self.frame_layout
            .intents
            .cancel_kind(output_key, PopupKind::Search);
        crate::ui::favorite_actions::cancel(self, output_key);
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key && output.show_search)
        {
            output.launch_pending = None;
            output.show_search = false;
            output.search_request_token = None;
            output.query.clear();
            output.selected_result = None;
            output.selected_result_index = 0;
            output.launch_error = None;
        }
    }

    pub(in crate::ui) fn dismiss_notifications(&mut self, output_key: &str) {
        self.frame_layout
            .intents
            .cancel_kind(output_key, PopupKind::Notifications);
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.show_notifications = false;
        }
    }

    pub(in crate::ui) fn dismiss_power(&mut self, output_key: &str) {
        self.frame_layout
            .intents
            .cancel_kind(output_key, PopupKind::Power);
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key && output.show_power)
        {
            output.show_power = false;
            output.power_confirmation = None;
            output.launch_error = None;
        }
    }

    pub(in crate::ui) fn dismiss_map(&mut self, output_key: &str) {
        self.frame_layout
            .intents
            .cancel_kind(output_key, PopupKind::Map);
        self.canvas_dragging = false;
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.show_map = false;
        }
    }
}
