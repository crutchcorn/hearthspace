//! Serial native-mode worker. The compositor owns the deadline and rollback.
use crate::state::SettingsApp;
use hearthspace_ipc::display_modes::{Request, Snapshot};
use std::time::Duration;
use xilem::{
    ViewCtx,
    core::{MessageProxy, NoElement, View},
    tokio::{self, sync::mpsc::UnboundedSender},
    view::task_raw,
};

#[derive(Default)]
pub(crate) struct State {
    pub snapshot: Snapshot,
    pub error: Option<String>,
    pub busy: bool,
    connection_failed: bool,
    sender: Option<UnboundedSender<Request>>,
    pub dialog: Option<(u64, xilem::WindowId)>,
    pub dismissed_token: Option<u64>,
    pub dialog_height: Option<f64>,
}

pub(crate) fn request(state: &mut SettingsApp, operation: Request) {
    let state = &mut state.display_layout.modes;
    if state.busy {
        return;
    }
    if state
        .sender
        .as_ref()
        .is_some_and(|s| s.send(operation).is_ok())
    {
        state.busy = true;
        state.error = None;
    } else {
        state.error = Some("Display controls are unavailable.".into());
    }
}

pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<(bool, Result<Snapshot, String>)>, state: &mut SettingsApp| {
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Request>();
            state.display_layout.modes.sender = Some(sender);
            async move {
                let mut timer = tokio::time::interval(Duration::from_millis(500));
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut previous = None;
                loop {
                    let operation = tokio::select! {
                        value=receiver.recv()=>{let Some(value)=value else {break};Some(value)},
                        _=timer.tick()=>None,
                    };
                    let action = operation.is_some();
                    let result = tokio::task::spawn_blocking(move || query(operation))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                    if (action || previous.as_ref() != Some(&result))
                        && proxy.message((action, result.clone())).is_err()
                    {
                        break;
                    }
                    previous = Some(result);
                }
            }
        },
        |state: &mut SettingsApp, (action, result)| {
            let state = &mut state.display_layout.modes;
            if action {
                state.busy = false;
            }
            match result {
                Ok(snapshot) => {
                    if state.connection_failed {
                        state.error = None;
                        state.connection_failed = false;
                    }
                    state.snapshot = snapshot;
                }
                Err(error) => {
                    state.connection_failed = !action;
                    state.snapshot.available = false;
                    state.error = Some(error);
                }
            }
        },
    )
}

fn query(operation: Option<Request>) -> Result<Snapshot, String> {
    let path = crate::settings_backend::command_socket_path().map_err(|e| e.to_string())?;
    let command = operation.map_or_else(
        || "display-modes".into(),
        |o| format!("display-mode {}", serde_json::to_string(&o).unwrap()),
    );
    let payload = hearthspace_ipc::transport::request_payload(
        &path,
        &command,
        Duration::from_secs(2),
        hearthspace_ipc::transport::MAX_RESPONSE_BYTES,
    )
    .map_err(|e| e.to_string())?;
    serde_json::from_slice(&payload).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) fn test_channel(
    state: &mut SettingsApp,
) -> tokio::sync::mpsc::UnboundedReceiver<Request> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    state.display_layout.modes.sender = Some(sender);
    receiver
}
