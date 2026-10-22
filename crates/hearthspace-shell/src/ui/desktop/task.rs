//! Filesystem discovery, launch authorization and favorite writes stay off the UI.
use std::{path::Path, time::Duration};

use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use crate::{
    app_catalog::AppCatalog,
    desktop_items::{self, DesktopItem},
};

use super::{ShellState, events};
use crate::ui::{command_client, control_worker};

#[path = "worker.rs"]
mod worker;
use worker::{Completed, Failure, discover, perform, register};

pub(super) enum Request {
    Command(String),
    Accept(u64),
    Context(super::context_actions::Request),
}

#[derive(Debug)]
enum Update {
    Source(AppCatalog, Vec<DesktopItem>, Option<String>),
    Snapshot(events::Snapshot),
    Finished(Result<Completed, Failure>, bool),
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;

pub(super) fn submit(state: &mut ShellState, request: Request) {
    let accepts_token = matches!(request, Request::Accept(_));
    let menu_generation = match &request {
        Request::Context(request) => Some(request.generation),
        _ => None,
    };
    if state
        .desktop
        .requests
        .as_ref()
        .is_none_or(|sender| sender.try_send(request).is_err())
    {
        let error = "Desktop items are busy. Try again shortly.".to_string();
        if let Some(generation) = menu_generation {
            super::context::finished(state, generation, Err(error));
        } else if accepts_token {
            // An event can still be queued after lock or VT loss. Only a worker
            // that confirms an active session may open feedback for it.
            state.desktop.error = Some(error);
        } else {
            super::feedback(state, error);
        }
    }
}

pub(in crate::ui) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let refresh = state.desktop.refresh.clone();
            let runtime = state.favorites.tracks_installed_apps;
            let mut catalog = state.catalog.clone();
            let mut favorites = state.favorites.store.clone();
            let (sender, mut requests) = tokio::sync::mpsc::channel(32);
            state.desktop.requests = Some(sender);
            async move {
                let mut items = Vec::new();
                let mut previous_snapshot = None;
                let mut previous_error = None;
                let mut registered = None;
                let mut initialized = false;
                let mut poll = tokio::time::interval(Duration::from_secs(2));
                poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        biased;
                        request = requests.recv() => {
                            let Some(request) = request else { break; };
                            let changed_files = matches!(&request, Request::Context(super::context_actions::Request { action: super::context_actions::Action::Paste | super::context_actions::Action::Rename(_) | super::context_actions::Action::Trash | super::context_actions::Action::NewFolder(_), .. }));
                            let socket = socket.clone();
                            let worker_socket = socket.clone();
                            let known = items.clone();
                            let store = favorites.clone();
                            let mut pending = tokio::task::spawn_blocking(move || {
                                perform(&worker_socket, request, &known, store, runtime)
                            });
                            // Large transfers stay serial, but must not hold desktop
                            // snapshots hostage: outside clicks and lock still close menus.
                            let result = loop {
                                tokio::select! {
                                    result = &mut pending => break result.unwrap_or_else(|error| Err(Failure::report(format!("Desktop action failed: {error}")))),
                                    _ = refresh.notified() => {
                                        if !refresh_snapshot(&socket, &proxy, &mut previous_snapshot).await { return; }
                                    }
                                    _ = poll.tick() => {
                                        if !refresh_snapshot(&socket, &proxy, &mut previous_snapshot).await { return; }
                                    }
                                }
                            };
                            if let Ok(Completed::Favorite(store) | Completed::Context(_, Ok(super::context_actions::Completed::Closed(Some(store))))) = &result {
                                favorites = store.clone();
                            }
                            if changed_files { poll.reset_immediately(); }
                            let active = control_worker::session(socket).await.available;
                            if proxy.message(Update::Finished(result, active)).is_err() { break; }
                            refresh.notify_one();
                        }
                        _ = refresh.notified() => {
                            if !refresh_snapshot(&socket, &proxy, &mut previous_snapshot).await { break; }
                        }
                        _ = poll.tick() => {
                            let previous_catalog = catalog.clone();
                            let previous_items = items.clone();
                            let source = tokio::task::spawn_blocking(move || {
                                let catalog = if runtime { AppCatalog::load() } else { previous_catalog };
                                let directory = runtime.then(desktop_items::desktop_directory).flatten();
                                discover(catalog, directory.as_deref(), &previous_items)
                            }).await;
                            if let Ok((next_catalog, next_items, mut error)) = source {
                                if registered.as_ref() != Some(&next_items) {
                                    let worker_items = next_items.clone();
                                    let worker_socket = socket.clone();
                                    match tokio::task::spawn_blocking(move || register(&worker_socket, &worker_items)).await {
                                        Ok(Ok(())) => registered = Some(next_items.clone()),
                                        Ok(Err(message)) => error = Some(message),
                                        Err(error) => tracing::warn!(%error, "desktop registration worker failed"),
                                    }
                                }
                                if !initialized || catalog.apps() != next_catalog.apps() || items != next_items || previous_error != error {
                                    initialized = true;
                                    catalog = next_catalog;
                                    items = next_items;
                                    previous_error = error.clone();
                                    if proxy.message(Update::Source(catalog.clone(), items.clone(), error)).is_err() { break; }
                                }
                            }
                            if !refresh_snapshot(&socket, &proxy, &mut previous_snapshot).await { break; }
                        }
                    }
                }
            }
        },
        |state, update| match update {
            Update::Source(catalog, items, error) => {
                state.catalog = catalog;
                state.desktop.replace_items(items);
                state.desktop.source_error = error;
            }
            Update::Snapshot(snapshot) => {
                super::context::apply(state, snapshot.context.clone());
                state.desktop.snapshot = snapshot;
            }
            Update::Finished(result, active) => match result {
                Ok(completed) => {
                    apply_completed(state, completed, active);
                    state.desktop.error = None;
                }
                Err(error) if active && error.report => super::feedback(state, error.message),
                Err(error) => state.desktop.error = Some(error.message),
            },
        },
    )
}

fn apply_completed(state: &mut ShellState, completed: Completed, active: bool) {
    match completed {
        Completed::Favorite(store) => {
            state.favorites.store = store;
            state.favorites.error = None;
        }
        Completed::Activated if active => state.dismiss_panels(),
        Completed::Activated | Completed::Command => {}
        Completed::Context(generation, result) => {
            // A completed write belongs to shared state even if its menu was
            // dismissed or replaced while the worker was saving the favorite.
            let result = match result {
                Ok(super::context_actions::Completed::Closed(Some(store))) => {
                    state.favorites.store = store;
                    state.favorites.error = None;
                    Ok(super::context_actions::Completed::Closed(None))
                }
                result => result,
            };
            if active {
                super::context::finished(state, generation, result);
            }
        }
    }
}

async fn refresh_snapshot(
    socket: &Path,
    proxy: &MessageProxy<Update>,
    previous: &mut Option<events::Snapshot>,
) -> bool {
    let socket = socket.to_path_buf();
    if let Ok(Ok(snapshot)) = tokio::task::spawn_blocking(move || {
        command_client::query_desktop(&socket)
            .map_err(|error| error.to_string())
            .and_then(|bytes| events::parse_snapshot(&bytes))
    })
    .await
    {
        // Coordinates and the compositor revision change continuously while
        // dragging. Rebuild tile surfaces only when their visible state changes.
        let changed = previous.as_ref().is_none_or(|old| {
            old.select_mode != snapshot.select_mode
                || old.items != snapshot.items
                || old.context != snapshot.context
        });
        *previous = Some(snapshot.clone());
        if changed {
            return proxy.message(Update::Snapshot(snapshot)).is_ok();
        }
    }
    true
}
