//! Settings adapter: observations update connected rows, never persisted preferences.
use hearthspace_ipc::MonitorConfig;
use hearthspace_wayland_client::{OutputInfo, connect_wayland, watch_outputs};
use xilem::{
    core::{MessageProxy, NoElement, View},
    tokio,
    view::task_raw,
};

mod primary;

use crate::{
    monitors::{LiveOutput, monitor_logical_width, monitors_for_live_outputs},
    state::SettingsApp,
};

#[derive(Default)]
pub(crate) struct Observation {
    received: bool,
    primary: Option<String>,
    // Includes disconnected drafts until their edit is committed successfully.
    preview: Vec<MonitorConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) outputs: Vec<LiveOutput>,
    pub(crate) primary: Option<String>,
}

#[derive(Debug)]
enum Message {
    Changed(Snapshot),
    Unavailable(String),
}

pub(crate) fn task() -> impl View<SettingsApp, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Message>, _state: &mut SettingsApp| async move {
            let (sender, receiver) = tokio::sync::watch::channel(None);
            let observe = async {
                let connection = connect_wayland().await?;
                watch_outputs(connection, move |outputs| {
                    sender.send(Some(outputs)).map_err(std::io::Error::other)
                })
                .await
            };
            let socket =
                crate::settings_backend::command_socket_path().map_err(|error| error.to_string());
            let deliver = primary::deliver(receiver, socket, |snapshot| {
                proxy.message(Message::Changed(snapshot)).is_ok()
            });
            tokio::select! {
                result = observe => {
                    if let Err(error) = result { drop(proxy.message(Message::Unavailable(error.to_string()))); }
                },
                _ = deliver => {},
            }
        },
        |state: &mut SettingsApp, message| match message {
            Message::Changed(snapshot) => apply_snapshot(state, snapshot),
            Message::Unavailable(error) => {
                if !state.output_observation.received {
                    state.status = format!("Live monitors unavailable: {error}");
                }
            }
        },
    )
}

fn live_outputs(outputs: Vec<OutputInfo>) -> Vec<LiveOutput> {
    outputs
        .into_iter()
        .filter_map(|output| {
            let (width, height) = output.size?;
            let id = output.connector();
            Some(LiveOutput {
                name: output
                    .description
                    .or(output.name)
                    .unwrap_or_else(|| id.clone()),
                id,
                width,
                height,
                scale: output.scale,
                refresh_millihertz: output.refresh_millihertz,
            })
        })
        .collect()
}

pub(crate) fn apply_snapshot(state: &mut SettingsApp, snapshot: Snapshot) {
    state.live_outputs = snapshot.outputs;
    state.output_observation.received = true;
    state.output_observation.primary = snapshot.primary;
    refresh_monitor_rows(state);
}

/// Called both by live observation and successful transaction completion.
/// Every recomputation starts from retained preferences, with local drafts overlaid.
pub(crate) fn refresh_monitor_rows(state: &mut SettingsApp) {
    if !state.output_observation.received {
        return;
    }
    if state.display_layout.has_current_edit() {
        for monitor in &state.settings.monitors {
            if let Some(draft) = state
                .output_observation
                .preview
                .iter_mut()
                .find(|draft| draft.id == monitor.id)
            {
                *draft = monitor.clone();
            } else {
                state.output_observation.preview.push(monitor.clone());
            }
        }
    }
    state
        .display_layout
        .retain_disconnected_preview(!state.output_observation.preview.is_empty());
    let mut rows = monitors_for_live_outputs(
        &state.saved_monitors,
        &state.live_outputs,
        state.output_observation.primary.as_deref(),
    );
    for row in &mut rows {
        if let Some(draft) = state
            .output_observation
            .preview
            .iter()
            .find(|draft| draft.id == row.id)
        {
            row.x = draft.x;
            row.y = draft.y;
            row.scale = draft.scale;
        }
    }
    // Newly connected screens must not displace the in-progress drag. Place them
    // beyond existing draft edges while leaving surviving draft coordinates intact.
    if state
        .output_observation
        .preview
        .iter()
        .any(|draft| rows.iter().any(|row| row.id == draft.id))
    {
        let mut next_x = rows
            .iter()
            .filter(|row| {
                state
                    .output_observation
                    .preview
                    .iter()
                    .any(|draft| draft.id == row.id)
                    || state
                        .settings
                        .monitors
                        .iter()
                        .any(|previous| previous.id == row.id)
            })
            .map(|row| row.x.saturating_add(monitor_logical_width(row)))
            .max()
            .unwrap_or(0);
        for row in &mut rows {
            if !state
                .output_observation
                .preview
                .iter()
                .any(|draft| draft.id == row.id)
                && !state
                    .settings
                    .monitors
                    .iter()
                    .any(|previous| previous.id == row.id)
            {
                row.x = next_x;
                row.y = 0;
                next_x = next_x.saturating_add(monitor_logical_width(row));
            }
        }
    }
    state.settings.monitors = rows;
    crate::actions::keep_selected_monitor_available(state);
}

/// A successful connected-layout save acknowledges only the submitted connectors.
/// Failed or unsent drafts for disconnected outputs remain available on replug.
pub(crate) fn acknowledge_preview(state: &mut SettingsApp, submitted: &[MonitorConfig]) {
    state
        .output_observation
        .preview
        .retain(|draft| !submitted.iter().any(|row| row.id == draft.id));
    state
        .display_layout
        .retain_disconnected_preview(!state.output_observation.preview.is_empty());
}

pub(crate) fn saved_monitors_changed(
    state: &mut SettingsApp,
    monitors: Vec<MonitorConfig>,
    accepted_edit: bool,
) {
    state.saved_monitors = monitors;
    if accepted_edit && !state.output_observation.received {
        state.settings.monitors = state.saved_monitors.clone();
        crate::actions::keep_selected_monitor_available(state);
    }
    refresh_monitor_rows(state);
}

#[cfg(test)]
mod tests;
