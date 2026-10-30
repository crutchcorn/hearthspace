//! Explicit tool selection delegates to the existing feature state owners.
use crate::compositor::App;
use hearthspace_ipc::command::CanvasTool;
use smithay::backend::input::InputTime;

impl App {
    pub(in crate::compositor) fn check_canvas_tool_available(&self) -> Result<(), String> {
        if self.canvas_pan_captured()
            && !self.session_locked()
            && !self.modal_confirmation_active()
            && !self.window_switcher_active()
            && !self.keyboard.is_grabbed()
        {
            Ok(())
        } else {
            self.check_navigation_available()
        }
    }

    pub(in crate::compositor) fn selected_canvas_tool(&self) -> Option<CanvasTool> {
        if self.layout_editing() {
            Some(CanvasTool::Layout)
        } else if self.group_selection_active() {
            Some(CanvasTool::Group)
        } else if self.canvas_search_mode() {
            Some(CanvasTool::Search)
        } else if self.canvas_pan.selected {
            Some(CanvasTool::Pan)
        } else if self.desktop_selection_mode() {
            Some(CanvasTool::Select)
        } else {
            None
        }
    }

    pub(in crate::compositor) fn select_canvas_tool(
        &mut self,
        tool: CanvasTool,
    ) -> Result<(), String> {
        if self.session_locked()
            || self.modal_confirmation_active()
            || self.window_switcher_active()
        {
            return Err("canvas tool unavailable during a modal interaction".into());
        }
        if self.selected_canvas_tool() == Some(tool) {
            return Ok(());
        }
        // A tool change revokes movement, never the old button sequence. Only
        // a new press after all releases drain may start the selected tool.
        if self.canvas_pan_captured() {
            if self.keyboard.is_grabbed() {
                return Err("canvas tool unavailable during a keyboard grab".into());
            }
            self.disarm_canvas_pan();
        } else if self.canvas_search_active() {
            self.disarm_canvas_search();
        } else if self.layout_editing() {
            self.layout_escape();
        } else {
            self.check_navigation_available()?;
        }
        self.disarm_canvas_search();
        self.disarm_canvas_pan();
        match tool {
            CanvasTool::Select => self.select_desktop_tool(),
            CanvasTool::Pan => {
                self.leave_window_focus_views();
                self.disable_desktop_selection();
                self.disarm_group_selection();
                self.canvas_pan.selected = true;
            }
            CanvasTool::Search => self.select_canvas_search_tool(),
            CanvasTool::Group => self.select_group_tool(),
            CanvasTool::Layout => {
                self.open_layout_editor()?;
            }
        }
        self.layout_changed();
        self.apply_pointer_motion(self.pointer_location, InputTime::now());
        Ok(())
    }
}
