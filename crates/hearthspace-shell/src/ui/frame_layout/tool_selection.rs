//! Keep an originating chooser available until its selection is confirmed.
use super::{ShellState, popup_intent::Intent, popups::Popup, task::Request};
use crate::ui::quick_panel::Kind;
use hearthspace_ipc::command::{CanvasTool, ShellCommand};

#[derive(Clone, Debug)]
pub(super) struct Selection {
    intent: Intent,
    pub tool: CanvasTool,
}

pub(super) fn choose(state: &mut ShellState, output: &str, tool: CanvasTool) {
    let Some(owner) = state
        .outputs
        .iter_mut()
        .find(|item| item.key() == output && item.show_controls && item.quick_panel == Kind::Tools)
    else {
        return;
    };
    owner.tool_error = None;
    let selection = Selection {
        intent: state.frame_layout.intents.begin(
            output.into(),
            Popup::Quick(Kind::Tools),
            state.frame_layout.popup_epoch,
        ),
        tool,
    };
    let request = Request {
        command: ShellCommand::CanvasTool(tool).wire_name(),
        open: None,
        selection: Some(selection.clone()),
    };
    let result = state
        .frame_layout
        .requests
        .as_ref()
        .ok_or_else(|| "Canvas tools are not ready.".to_owned())
        .and_then(|sender| sender.try_send(request).map_err(|error| error.to_string()));
    if let Err(error) = result {
        selection.complete(state, Err(error));
    }
}

impl Selection {
    pub fn complete(self, state: &mut ShellState, result: Result<(), String>) {
        // Intents already cancel on dismissal, another popup/attempt, output
        // removal, and compositor takeover. Completion never opens a surface.
        if !self.intent.is_current() {
            return;
        }
        self.intent.cancel();
        let Some(output) = state.outputs.iter_mut().find(|output| {
            output.key() == self.intent.key
                && output.show_controls
                && output.quick_panel == Kind::Tools
        }) else {
            return;
        };
        match result {
            Ok(()) if state.frame_layout.snapshot.canvas_tool == Some(self.tool) => {
                state.dismiss_controls(&self.intent.key);
            }
            _ => {
                output.tool_error =
                    Some("Could not change canvas tool. Choose it again to retry.".into());
            }
        }
    }
}
