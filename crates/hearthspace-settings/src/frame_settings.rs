//! The compositor owns frame placement and preferences; this page is a live client.
use std::time::Duration;

use xilem::{
    ViewCtx,
    core::{MessageProxy, NoElement, View},
    tokio::{self, sync::mpsc::UnboundedSender},
    view::task_raw,
};

use crate::state::SettingsApp;

#[cfg(test)]
mod tests;
mod transport;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Refresh,
    OpenEditor,
    AllowHideTools(bool),
    RestoreTools,
}

impl Operation {
    fn command(self) -> Option<String> {
        match self {
            Self::Refresh => None,
            Self::OpenEditor => Some("layout-tool".into()),
            Self::AllowHideTools(allow) => {
                Some(format!("frame-settings {{\"allow_hide_tools\":{allow}}}"))
            }
            Self::RestoreTools => Some("frame-settings {\"restore_tools\":true}".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Snapshot {
    pub(crate) allow_hide_tools: bool,
}

#[derive(Debug)]
pub(crate) struct Response {
    operation: Operation,
    snapshot: Result<Snapshot, String>,
    action_error: Option<String>,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) sender: Option<UnboundedSender<Operation>>,
    pub(crate) snapshot: Option<Snapshot>,
    pub(crate) available: bool,
    pending: Option<Operation>,
    connection_error: Option<String>,
    action_error: Option<String>,
}

impl State {
    pub(crate) fn enabled(&self) -> bool {
        self.available
            && self
                .sender
                .as_ref()
                .is_some_and(|sender| !sender.is_closed())
    }

    pub(crate) fn request(&mut self, operation: Operation) {
        if operation != Operation::Refresh && (!self.enabled() || self.pending.is_some()) {
            return;
        }
        if let Some(sender) = &self.sender {
            if sender.send(operation).is_ok() {
                if operation != Operation::Refresh {
                    self.pending = Some(operation);
                    self.action_error = None;
                }
            } else {
                self.available = false;
                self.connection_error = Some("Layout controls are unavailable.".into());
            }
        }
    }

    pub(crate) fn message(&self) -> Option<&str> {
        self.connection_error
            .as_deref()
            .or(self.action_error.as_deref())
            .or_else(|| {
                self.pending.map(|operation| match operation {
                    Operation::OpenEditor => "Opening the layout editor…",
                    Operation::RestoreTools => "Restoring canvas tools…",
                    _ => "Saving canvas tools…",
                })
            })
    }

    fn receive(&mut self, response: Response) {
        if response.operation != Operation::Refresh {
            self.pending = None;
            self.action_error = response.action_error;
        }
        match response.snapshot {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.available = true;
                self.connection_error = None;
            }
            Err(error) => {
                // Keep the last confirmed value through a transient disconnect.
                self.available = false;
                self.connection_error = Some(format!("Layout controls are unavailable: {error}"));
            }
        }
    }
}

/// One serial worker prevents a late refresh from overwriting an acknowledged
/// change. Socket reads never run on the UI thread, even while entering System.
pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Response>, state: &mut SettingsApp| {
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            state.frame_settings.sender = Some(sender);
            async move {
                let mut timer = tokio::time::interval(Duration::from_secs(1));
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut previous = None;
                loop {
                    let operation = tokio::select! {
                        operation = receiver.recv() => {
                            let Some(operation) = operation else { break };
                            operation
                        }
                        _ = timer.tick() => Operation::Refresh,
                    };
                    let response = tokio::task::spawn_blocking(move || {
                        let path = crate::settings_backend::command_socket_path();
                        match path {
                            Ok(path) => transport::execute(&path, operation),
                            Err(_) => Response {
                                operation,
                                snapshot: Err(
                                    "connect to Hearthspace to change the frame layout".into()
                                ),
                                action_error: None,
                            },
                        }
                    })
                    .await
                    .unwrap_or_else(|error| Response {
                        operation,
                        snapshot: Err(format!("Layout worker failed: {error}")),
                        action_error: None,
                    });
                    let changed = previous.as_ref() != Some(&response.snapshot);
                    previous = Some(response.snapshot.clone());
                    if (changed || operation != Operation::Refresh)
                        && proxy.message(response).is_err()
                    {
                        break;
                    }
                }
            }
        },
        |state: &mut SettingsApp, response| state.frame_settings.receive(response),
    )
}
