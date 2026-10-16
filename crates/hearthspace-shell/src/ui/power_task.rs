//! Capability queries and suspend requests stay off the shell's UI thread.
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub(super) struct SuspendStatus {
    pub available: bool,
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub(super) struct PowerState {
    pub status: SuspendStatus,
    pub pending: bool,
    sender: Option<tokio::sync::mpsc::Sender<String>>,
}

impl PowerState {
    pub fn can_suspend(&self) -> bool {
        self.status.available && !self.status.busy && !self.pending && self.sender.is_some()
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
    let timeout = if command == "suspend" {
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
        if command == "suspend" {
            "Suspend could not be confirmed. Check the session before trying again.".to_string()
        } else {
            "Suspend is unavailable in this session.".to_string()
        }
    })?;
    let reply: Reply = serde_json::from_str(&response)
        .map_err(|_| "Power controls returned an invalid response.".to_string())?;
    if !reply.ok {
        return Err(reply
            .status
            .error
            .unwrap_or_else(|| "Suspend is unavailable in this session.".into()));
    }
    Ok(reply.status)
}

async fn query(socket: PathBuf, command: &'static str) -> Result<SuspendStatus, String> {
    tokio::task::spawn_blocking(move || request(&socket, command))
        .await
        .unwrap_or_else(|_| Err("Power controls are unavailable.".into()))
}

pub(super) fn suspend(state: &mut ShellState, output_key: &str) {
    if !state.power.can_suspend() {
        return;
    }
    let result = state
        .power
        .sender
        .as_ref()
        .expect("can_suspend checked the sender")
        .try_send(output_key.to_string());
    if result.is_ok() {
        state.power.pending = true;
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
            let (sender, mut commands) = tokio::sync::mpsc::channel::<String>(1);
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
                            let Some(key) = key else { break; };
                            let result = query(socket.clone(), "suspend").await.map(|_| ());
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
                state.finish_launch(&key, result);
            }
        },
    )
}

#[cfg(test)]
#[path = "power_task_tests.rs"]
mod tests;
