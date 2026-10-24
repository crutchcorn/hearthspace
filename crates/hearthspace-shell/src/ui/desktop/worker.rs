//! Blocking desktop operations, including bounded private IPC transactions.
use super::Request;
use crate::desktop_items::{self, DesktopItem, DesktopItemKind};
use crate::ui::command_client;
use hearthspace_common::MAX_DESKTOP_ITEMS;
use hearthspace_desktop::app_catalog::AppCatalog;
use hearthspace_desktop::favorites::FavoriteStore;
use serde::Deserialize;
use std::{path::Path, time::Duration};

#[derive(Debug)]
pub(super) enum Completed {
    Command,
    Activated,
    Favorite(FavoriteStore),
    Context(
        u64,
        Result<super::super::context_actions::Completed, String>,
    ),
}

#[derive(Debug)]
pub(super) struct Failure {
    pub message: String,
    pub report: bool,
}

impl Failure {
    pub(super) fn report(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            report: true,
        }
    }

    fn silent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            report: false,
        }
    }
}

pub(super) fn discover(
    catalog: AppCatalog,
    directory: Option<&Path>,
    previous: &[DesktopItem],
) -> (AppCatalog, Vec<DesktopItem>, Option<String>) {
    let (mut items, mut error) = match desktop_items::discover(&catalog, directory) {
        Ok(items) => (items, None),
        Err(error) => {
            let mut items: Vec<_> = previous
                .iter()
                .filter(|item| !matches!(item.kind, DesktopItemKind::App { .. }))
                .cloned()
                .collect();
            items.extend(desktop_items::discover(&catalog, None).unwrap_or_default());
            (
                items,
                Some(format!("Desktop folder is unavailable: {error}")),
            )
        }
    };
    if items.len() > MAX_DESKTOP_ITEMS {
        let overflow = format!(
            "Showing {MAX_DESKTOP_ITEMS} of {} desktop items. Additional items remain available in their folders and Apps List.",
            items.len()
        );
        error =
            Some(error.map_or_else(|| overflow.clone(), |error| format!("{error}. {overflow}")));
        items.truncate(MAX_DESKTOP_ITEMS);
    }
    (catalog, items, error)
}

pub(super) fn registration_commands(ids: &[String]) -> Vec<String> {
    let mut commands = Vec::new();
    let mut offset = 0;
    while offset < ids.len() {
        let mut end = offset + 1;
        let build = |end| {
            format!(
                "desktop-register {}",
                serde_json::json!({
                    "offset": offset,
                    "ids": &ids[offset..end],
                })
            )
        };
        let mut command = build(end);
        while end < ids.len() {
            let next = build(end + 1);
            if next.len() + 1 > hearthspace_ipc::transport::MAX_REQUEST_BYTES {
                break;
            }
            command = next;
            end += 1;
        }
        commands.push(command);
        offset = end;
    }
    if ids.is_empty() {
        commands.push("desktop-register {\"offset\":0,\"ids\":[]}".into());
    }
    commands
}

pub(super) fn register(socket: &Path, items: &[DesktopItem]) -> Result<(), String> {
    let ids: Vec<_> = items.iter().map(|item| item.id.clone()).collect();
    for command in registration_commands(&ids)
        .into_iter()
        .chain(search_registration::commands(items))
        .chain(file_registration::commands(items))
    {
        command_client::send_command(socket, &command)
            .map_err(|error| format!("Could not arrange desktop items: {error}"))?;
    }
    Ok(())
}

#[path = "file_registration.rs"]
mod file_registration;
#[path = "search_registration.rs"]
mod search_registration;

pub(super) fn perform(
    socket: &Path,
    request: Request,
    items: &[DesktopItem],
    mut favorites: FavoriteStore,
    runtime: bool,
) -> Result<Completed, Failure> {
    let token = match request {
        Request::Context(request) => {
            let generation = request.generation;
            let result =
                super::super::context_actions::perform(socket, request, items, favorites, runtime);
            return Ok(Completed::Context(generation, result));
        }
        Request::Command(command) => {
            command_client::send_command(socket, &command).map_err(|error| {
                if command.starts_with("desktop-focus ") {
                    Failure::silent(error)
                } else {
                    Failure::report(error)
                }
            })?;
            return Ok(Completed::Command);
        }
        Request::Accept(token) => token,
    };
    #[derive(Deserialize)]
    struct Accepted {
        action: String,
        id: String,
    }
    let bytes = command_client::query_payload(
        socket,
        &format!("desktop-event-accept {token}"),
        Duration::from_millis(750),
    )
    .map_err(|error| Failure::silent(error.to_string()))?;
    let action: Accepted = serde_json::from_slice(&bytes).map_err(|error| {
        Failure::silent(format!("Could not confirm the desktop action: {error}"))
    })?;
    let item = items
        .iter()
        .find(|item| item.id == action.id)
        .ok_or_else(|| Failure::report("This desktop item is no longer available."))?;
    match action.action.as_str() {
        "activate" => {
            match &item.kind {
                DesktopItemKind::App { app_id, .. } => {
                    command_client::launch_app(socket, app_id, None)
                }
                DesktopItemKind::File { path } | DesktopItemKind::Folder { path } => {
                    desktop_items::open_path(socket, path, None)
                }
            }
            .map_err(|error| Failure::report(format!("Could not open {}: {error}", item.title)))?;
            // A successfully launched app should clear the shortcut selection.
            // A stale clear must never turn the successful launch into an error.
            let _ = command_client::send_command(socket, "desktop-clear");
            Ok(Completed::Activated)
        }
        "favorite" => {
            let DesktopItemKind::App { app_id, .. } = &item.kind else {
                return Err(Failure::report("Only apps can be added to favorites."));
            };
            if runtime {
                favorites = FavoriteStore::load_default()
                    .map_err(|error| Failure::report(error.to_string()))?;
            }
            favorites
                .add(app_id)
                .map_err(|error| Failure::report(error.to_string()))?;
            Ok(Completed::Favorite(favorites))
        }
        _ => Err(Failure::silent("Unknown desktop action.")),
    }
}
