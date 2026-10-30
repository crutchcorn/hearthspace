//! Fixed canvas chords reuse tool transitions and the shell's owned map action.
use hearthspace_ipc::{
    command::CanvasTool,
    shortcuts::{CANVAS_SHORTCUTS, CanvasAction},
};
use smithay::{input::keyboard::ModifiersState, utils::Rectangle};

use crate::compositor::App;

pub(super) fn shortcut(modifiers: &ModifiersState, symbol: u32) -> Option<CanvasAction> {
    if !modifiers.logo || modifiers.ctrl || modifiers.alt || modifiers.shift {
        return None;
    }
    let key = char::from_u32(symbol)?.to_ascii_lowercase();
    CANVAS_SHORTCUTS
        .iter()
        .find(|binding| binding.key == key)
        .map(|binding| binding.action)
}

impl App {
    pub(super) fn canvas_shortcuts_available(&self) -> bool {
        !self.session_locked()
            && !self.modal_confirmation_active()
            && !self.window_switcher_active()
            && !self.keyboard.is_grabbed()
            && (!self.pointer.is_grabbed() || self.canvas_pan_captured())
            && !self.touch.is_grabbed()
            && self.drag.is_none()
            && self.resize.is_none()
            && !self.group_interaction_active()
            && (self.canvas_pan_captured()
                || self.canvas_search_active()
                || self.layout_editing()
                || self.navigation_available())
    }

    /// Execute only after keyboard filtering, where focus and grabs may change.
    pub(super) fn run_canvas_shortcut(&mut self, action: CanvasAction) -> Result<(), String> {
        if !self.canvas_shortcuts_available() {
            return Err(
                "canvas shortcut unavailable during an input grab or modal interaction".into(),
            );
        }
        match action {
            CanvasAction::Tool(tool) => {
                self.select_canvas_tool(tool)?;
                if tool == CanvasTool::Search && !self.canvas_search_active() {
                    self.open_canvas_search(None, None)?;
                }
                Ok(())
            }
            CanvasAction::Map => {
                if self.layout_editing() {
                    self.layout_escape();
                }
                self.disarm_canvas_search();
                // Keep ownership until every held pointer button drains.
                self.cancel_canvas_pan();
                let outputs = self.capture_outputs();
                let output = outputs
                    .iter()
                    .find(|output| {
                        Rectangle::new(
                            (output.logical_x, output.logical_y).into(),
                            (output.logical_width, output.logical_height).into(),
                        )
                        .contains(self.pointer_location.to_i32_round())
                    })
                    .or_else(|| outputs.first())
                    .ok_or("no active output")?
                    .name
                    .clone();
                self.issue_canvas_search_action("tool:map".into(), Some(output))
            }
        }
    }
}
