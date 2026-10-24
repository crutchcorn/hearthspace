//! Only an armed, still-available affirmative control can end the session.
use crate::ui::{
    command_client::send_command,
    power_task::{self, Action},
    state::ShellState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Confirmation {
    LogOff,
    Reboot,
    PowerOff,
}

impl Confirmation {
    pub(super) const ALL: [Self; 3] = [Self::LogOff, Self::Reboot, Self::PowerOff];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::LogOff => "Log off",
            Self::Reboot => "Reboot",
            Self::PowerOff => "Turn off",
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::LogOff => "Log off?",
            Self::Reboot => "Reboot?",
            Self::PowerOff => "Turn off?",
        }
    }

    pub(super) fn body(self) -> &'static str {
        match self {
            Self::LogOff => "Save your work before ending this session.",
            Self::Reboot => "Save your work before restarting your computer.",
            Self::PowerOff => "Save your work before turning off your computer.",
        }
    }

    pub(super) fn action(self) -> Option<Action> {
        match self {
            Self::LogOff => None,
            Self::Reboot => Some(Action::Reboot),
            Self::PowerOff => Some(Action::PowerOff),
        }
    }

    pub(super) fn enabled(self, state: &ShellState) -> bool {
        self.action().map_or_else(
            || !state.power.busy(),
            |action| state.power.can_action(action),
        )
    }

    pub(super) fn pending(self, state: &ShellState) -> bool {
        self.action()
            .is_some_and(|action| state.power.pending_action == Some(action))
    }

    pub(super) fn unavailable(self, state: &ShellState) -> Option<String> {
        if self.enabled(state) {
            return None;
        }
        Some(if state.power.busy() {
            "A power action is already in progress.".into()
        } else {
            match self {
                Self::LogOff => "Log off is unavailable in this session.".into(),
                Self::Reboot => state
                    .power
                    .status
                    .reboot_error
                    .clone()
                    .unwrap_or_else(|| "Reboot is unavailable in this session.".into()),
                Self::PowerOff => state
                    .power
                    .status
                    .power_off_error
                    .clone()
                    .unwrap_or_else(|| "Turn off is unavailable in this session.".into()),
            }
        })
    }
}

pub(super) fn open(state: &mut ShellState, output_key: &str, action: Confirmation) {
    if !action.enabled(state) {
        return;
    }
    state.canvas_dragging = false;
    if let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == output_key && output.show_power)
    {
        output.power_confirmation = Some(action);
        output.launch_error = None;
    }
}

pub(super) fn confirm(state: &mut ShellState, output_key: &str, expected: Confirmation) {
    if !expected.enabled(state)
        || !state.outputs.iter().any(|output| {
            output.key() == output_key
                && output.show_power
                && output.power_confirmation == Some(expected)
        })
    {
        return;
    }
    if let Some(action) = expected.action() {
        power_task::request_action(state, output_key, action);
    } else if let Err(error) = send_command(&state.command_socket, "quit") {
        state.finish_launch(output_key, Err(error));
    }
}
