//! Capability queries and power requests stay off the shell's UI thread.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::state::ShellState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Suspend,
    Reboot,
    PowerOff,
}

impl Action {
    fn command(self) -> &'static str {
        match self {
            Self::Suspend => "suspend",
            Self::Reboot => "reboot",
            Self::PowerOff => "power_off",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Suspend => "Suspend",
            Self::Reboot => "Reboot",
            Self::PowerOff => "Turn off",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub(super) struct SuspendStatus {
    pub available: bool,
    pub busy: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub reboot_available: bool,
    #[serde(default)]
    pub power_off_available: bool,
    #[serde(default)]
    pub reboot_error: Option<String>,
    #[serde(default)]
    pub power_off_error: Option<String>,
}

#[derive(Default)]
pub(super) struct PowerState {
    pub status: SuspendStatus,
    pub pending: bool,
    pub pending_action: Option<Action>,
    sender: Option<tokio::sync::mpsc::Sender<(String, Action)>>,
}

impl PowerState {
    pub fn can_suspend(&self) -> bool {
        self.can_action(Action::Suspend)
    }

    pub fn can_action(&self, action: Action) -> bool {
        let available = match action {
            Action::Suspend => self.status.available,
            Action::Reboot => self.status.reboot_available,
            Action::PowerOff => self.status.power_off_available,
        };
        available && !self.busy() && self.sender.is_some()
    }

    pub fn busy(&self) -> bool {
        self.pending || self.status.busy
    }
}

#[derive(Deserialize)]
struct Reply {
    ok: bool,
    #[serde(flatten)]
    status: SuspendStatus,
}

#[derive(Debug)]
enum Update {
    Status(SuspendStatus),
    Finished(String, Result<(), String>),
}

fn request(socket: &Path, command: &str) -> Result<SuspendStatus, String> {
    let action = match command {
        "suspend" => Some(Action::Suspend),
        "reboot" => Some(Action::Reboot),
        "power_off" => Some(Action::PowerOff),
        _ => None,
    };
    let timeout = if action.is_some() {
        Duration::from_secs(20)
    } else {
        Duration::from_millis(750)
    };
    let response = hearthspace_ipc::transport::request_line(
        socket,
        &format!("{{\"command\":\"{command}\"}}\n"),
        timeout,
    )
    .map_err(|_| {
        if let Some(action) = action {
            format!(
                "{} could not be confirmed. Check the session before trying again.",
                action.label()
            )
        } else {
            "Power controls are unavailable in this session.".to_string()
        }
    })?;
    let reply: Reply = serde_json::from_str(&response)
        .map_err(|_| "Power controls returned an invalid response.".to_string())?;
    if !reply.ok {
        return Err(reply
            .status
            .error
            .unwrap_or_else(|| "This power action is unavailable in this session.".into()));
    }
    Ok(reply.status)
}

async fn query(socket: PathBuf, command: &'static str) -> Result<SuspendStatus, String> {
    tokio::task::spawn_blocking(move || request(&socket, command))
        .await
        .unwrap_or_else(|_| Err("Power controls are unavailable.".into()))
}

pub(super) fn suspend(state: &mut ShellState, output_key: &str) {
    request_action(state, output_key, Action::Suspend);
}

/// Called by the UI only after any required confirmation has been accepted.
pub(super) fn request_action(state: &mut ShellState, output_key: &str, action: Action) {
    if !state.power.can_action(action) {
        return;
    }
    let result = state
        .power
        .sender
        .as_ref()
        .expect("can_action checked the sender")
        .try_send((output_key.to_string(), action));
    if result.is_ok() {
        state.power.pending = true;
        state.power.pending_action = Some(action);
        if let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.launch_error = None;
        }
    } else {
        state.finish_launch(
            output_key,
            Err("Power controls are busy. Try again shortly.".into()),
        );
    }
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut ShellState| {
            let socket = state.command_socket.with_file_name("power-control.sock");
            let (sender, mut commands) = tokio::sync::mpsc::channel::<(String, Action)>(1);
            state.power.sender = Some(sender);
            async move {
                loop {
                    let status = query(socket.clone(), "status")
                        .await
                        .unwrap_or_else(|error| SuspendStatus {
                            error: Some(error),
                            ..Default::default()
                        });
                    if proxy.message(Update::Status(status)).is_err() {
                        break;
                    }
                    tokio::select! {
                        key = commands.recv() => {
                            let Some((key, action)) = key else { break; };
                            let result = query(socket.clone(), action.command()).await.map(|_| ());
                            if proxy.message(Update::Finished(key, result)).is_err() {
                                break;
                            }
                        }
                        () = tokio::time::sleep(Duration::from_secs(1)) => {}
                    }
                }
            }
        },
        |state: &mut ShellState, update| match update {
            Update::Status(status) => state.power.status = status,
            Update::Finished(key, result) => {
                state.power.pending = false;
                state.power.pending_action = None;
                state.finish_launch(&key, result);
            }
        },
    )
}

#[cfg(test)]
#[path = "power_task_tests.rs"]
mod tests;

#[cfg(test)]
pub(super) fn test_channel(
    state: &mut ShellState,
) -> tokio::sync::mpsc::Receiver<(String, Action)> {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    state.power.sender = Some(sender);
    receiver
}

#[cfg(test)]
#[path = "power_actions_tests.rs"]
mod action_tests;
