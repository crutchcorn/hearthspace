use std::{env, io, path::PathBuf};

#[cfg(test)]
use hearthspace_ipc::Settings;
use hearthspace_ipc::{
    MonitorConfig,
    settings_snapshot::{SettingsSnapshot, client_set_display_layout_snapshot_at},
};
use xilem::{
    core::{MessageProxy, NoElement, View},
    tokio::{
        self,
        sync::mpsc::{UnboundedReceiver, UnboundedSender},
    },
    view::task_raw,
};

use crate::state::SettingsApp;

const SAVING: &str = "Saving monitor layout…";

#[derive(Default)]
pub(crate) struct DisplayLayoutState {
    pub(crate) sender: Option<UnboundedSender<LayoutRequest>>,
    revision: u64,
    pending: Option<u64>,
    preview: bool,
    retained_preview: bool,
    committed_revision: u64,
    reconciliation: Option<LayoutRequest>,
}

impl DisplayLayoutState {
    /// Includes an unsaved drag or failed save, so observations cannot erase it.
    pub(crate) fn is_pending(&self) -> bool {
        self.has_current_edit() || self.retained_preview
    }

    pub(crate) fn has_current_edit(&self) -> bool {
        self.preview || self.pending.is_some()
    }

    pub(crate) fn retain_disconnected_preview(&mut self, retained: bool) {
        self.retained_preview = retained;
    }

    /// Call before changing the local layout, even for an uncommitted drag.
    /// Any outstanding completion becomes stale; selection alone needs no bump.
    pub(crate) fn invalidate_preview(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.preview = true;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LayoutRequest {
    revision: u64,
    observation_generation: u64,
    pub(crate) monitors: Vec<MonitorConfig>,
}

#[derive(Debug)]
pub(crate) struct LayoutResponse {
    request: LayoutRequest,
    saved: Result<SettingsSnapshot, String>,
}

pub(crate) fn save_display_layout(state: &mut SettingsApp) {
    state.display_layout.invalidate_preview();
    let request = LayoutRequest {
        revision: state.display_layout.revision,
        observation_generation: state.preferences.observation_generation,
        monitors: state.settings.monitors.clone(),
    };
    let sent = state
        .display_layout
        .sender
        .as_ref()
        .is_some_and(|sender| sender.send(request).is_ok());
    if sent {
        state.display_layout.pending = Some(state.display_layout.revision);
        state.status = SAVING.into();
    } else {
        state.status = "Failed to save monitor layout: settings worker unavailable".into();
    }
}

pub(crate) fn receive_display_layout(state: &mut SettingsApp, response: LayoutResponse) {
    if state.display_layout.pending == Some(response.request.revision) {
        state.display_layout.pending = None;
    }
    // Mode changes and hotplug do not invalidate an edit generation. Compare
    // editable fields only for surviving connectors; observation owns their sizes.
    let current = is_current_edit(state, &response.request);
    let status = match response.saved {
        Ok(snapshot) => {
            // Copy only the transaction's domain. Another page may have saved newer
            // appearance or background preferences while this request was in flight.
            if response.request.revision < state.display_layout.committed_revision {
                return;
            }
            state.display_layout.committed_revision = response.request.revision;
            // A read during a pending write may precede OR follow that commit.
            // Cursor equality detects ambiguity; observation order cannot resolve it.
            let ambiguous = state.preferences.observation_generation
                != response.request.observation_generation
                && state
                    .preferences
                    .snapshot
                    .as_ref()
                    .is_some_and(|observed| observed.cursor() != snapshot.cursor());
            crate::settings_refresh::local_save_finished(state);
            if ambiguous {
                if current {
                    // Keep the acknowledged local layout visible until the refresh
                    // worker's forced GET, begun after this acknowledgement, returns.
                    state.display_layout.reconciliation = Some(response.request.clone());
                }
            } else {
                if current {
                    state.display_layout.reconciliation = None;
                    state.display_layout.preview = false;
                    crate::wayland_outputs::acknowledge_preview(state, &response.request.monitors);
                }
                state.preferences.snapshot = Some(snapshot.clone());
                crate::wayland_outputs::saved_monitors_changed(
                    state,
                    snapshot.settings.monitors,
                    current,
                );
            }
            if !current {
                return;
            }
            String::new()
        }
        Err(error) if current => format!("Failed to save monitor layout: {error}"),
        Err(_) => return,
    };
    if state.status == SAVING {
        state.status = status;
    }
}

fn is_current_edit(state: &SettingsApp, request: &LayoutRequest) -> bool {
    request.revision == state.display_layout.revision
        && state.settings.monitors.iter().all(|row| {
            request
                .monitors
                .iter()
                .find(|saved| saved.id == row.id)
                .is_none_or(|saved| (saved.x, saved.y, saved.scale) == (row.x, row.y, row.scale))
        })
}

/// Called only after the refresh generation check. That generation was bumped
/// at acknowledgement, so this snapshot was read after the committed write.
pub(crate) fn reconcile_display_layout(state: &mut SettingsApp) {
    if let Some(request) = state.display_layout.reconciliation.take()
        && is_current_edit(state, &request)
    {
        state.display_layout.preview = false;
        crate::wayland_outputs::acknowledge_preview(state, &request.monitors);
    }
}

#[derive(Clone)]
struct Endpoints {
    settings: Result<PathBuf, String>,
}

/// Lives above the page switcher. One worker awaits each committed save before
/// accepting the next operation; no socket work occurs on the UI thread.
pub(crate) fn task() -> impl View<SettingsApp, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<LayoutResponse>, state: &mut SettingsApp| {
            let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
            state.display_layout.sender = Some(sender);
            let endpoints = Endpoints {
                settings: hearthspace_ipc::settings_socket_path()
                    .map_err(|error| error.to_string()),
            };
            run_worker(receiver, endpoints, move |response| {
                proxy.message(response).is_ok()
            })
        },
        receive_display_layout,
    )
}

async fn run_worker(
    mut receiver: UnboundedReceiver<LayoutRequest>,
    endpoints: Endpoints,
    mut respond: impl FnMut(LayoutResponse) -> bool,
) {
    while let Some(request) = receiver.recv().await {
        let work = request.clone();
        let endpoints = endpoints.clone();
        let response = tokio::task::spawn_blocking(move || execute(work, endpoints))
            .await
            .unwrap_or_else(|error| LayoutResponse {
                request,
                saved: Err(format!("Settings worker failed: {error}")),
            });
        if !respond(response) {
            return;
        }
    }
}

fn execute(request: LayoutRequest, endpoints: Endpoints) -> LayoutResponse {
    let saved = endpoints.settings.and_then(|path| {
        client_set_display_layout_snapshot_at(&path, &request.monitors)
            .map_err(|error| error.to_string())
    });
    LayoutResponse { request, saved }
}

pub(crate) fn command_socket_path() -> io::Result<PathBuf> {
    env::var_os(hearthspace_common::SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{} is not set",
                    hearthspace_common::SHELL_COMMAND_SOCKET_ENV
                ),
            )
        })
}

#[cfg(test)]
mod tests;
