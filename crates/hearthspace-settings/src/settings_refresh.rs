//! Each consumer owns projection/preview merging; the daemon publishes commits.
use crate::state::SettingsApp;
use hearthspace_ipc::settings_snapshot::{
    SettingsSnapshot, client_get_snapshot_at, client_wait_for_change_at,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::{path::PathBuf, time::Duration};
use xilem::{
    core::{MessageProxy, NoElement, View},
    tokio,
    view::task_raw,
};

#[derive(Default)]
pub(crate) struct RefreshState {
    pub(crate) generation: Arc<AtomicU64>,
    pub(crate) snapshot: Option<SettingsSnapshot>,
    pub(crate) observation_generation: u64,
}

pub(crate) fn local_save_finished(state: &mut SettingsApp) {
    state.preferences.generation.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn task() -> impl View<SettingsApp, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<(u64, SettingsSnapshot)>, state: &mut SettingsApp| {
            let generation = state.preferences.generation.clone();
            let socket = hearthspace_ipc::settings_socket_path().map_err(|error| error.to_string());
            run_refresh(generation, socket, move |message| {
                proxy.message(message).is_ok()
            })
        },
        apply_refresh,
    )
}

/// Session-specific transport loop. A local acknowledgement invalidates its
/// cursor so the next accepted message comes from a new post-commit GET.
pub(crate) async fn run_refresh(
    generation: Arc<AtomicU64>,
    socket: Result<PathBuf, String>,
    mut publish: impl FnMut((u64, SettingsSnapshot)) -> bool,
) {
    let mut cursor = None;
    let mut previous_generation = generation.load(Ordering::SeqCst);
    loop {
        let started_at = generation.load(Ordering::SeqCst);
        if started_at != previous_generation {
            cursor = None;
            previous_generation = started_at;
        }
        let after = cursor.clone();
        let socket = socket.clone();
        let result = tokio::task::spawn_blocking(move || {
            let socket = socket.map_err(std::io::Error::other)?;
            match after {
                Some(after) => {
                    client_wait_for_change_at(&socket, &after, Duration::from_millis(500))
                }
                None => client_get_snapshot_at(&socket).map(Some),
            }
        })
        .await;
        match result {
            Ok(Ok(Some(snapshot))) => {
                cursor = Some(snapshot.cursor());
                if !publish((started_at, snapshot)) {
                    return;
                }
            }
            Ok(Ok(None)) => {}
            _ => {
                cursor = None; // Reconnect obtains the latest snapshot.
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

pub(crate) fn apply_refresh(
    state: &mut SettingsApp,
    (generation, snapshot): (u64, SettingsSnapshot),
) {
    if generation != state.preferences.generation.load(Ordering::SeqCst) {
        return;
    }
    // A reconnect GET can observe a restored file with the same cursor. It is
    // still a new observation when ordering it against an outstanding commit.
    state.preferences.observation_generation =
        state.preferences.observation_generation.wrapping_add(1);
    let settings = &snapshot.settings;
    state.settings.appearance = settings.appearance;
    state.settings.test = settings.test;
    state.settings.background_mode = settings.background_mode;
    if !state.background_picker_pending
        && state.settings.background_image_path != settings.background_image_path
    {
        state.settings.background_image_path = settings.background_image_path.clone();
        state
            .background_preview
            .refresh(state.settings.background_image_path.as_deref());
    }
    crate::settings_backend::reconcile_display_layout(state);
    let accepted = !state.display_layout.is_pending();
    crate::wayland_outputs::saved_monitors_changed(state, settings.monitors.clone(), accepted);
    state.preferences.snapshot = Some(snapshot);
}

#[cfg(test)]
mod tests;
