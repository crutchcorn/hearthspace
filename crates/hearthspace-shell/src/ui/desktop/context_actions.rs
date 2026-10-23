//! Context actions validate the live compositor menu before touching files.
use crate::ui::command_client;
use crate::{
    desktop_items::{self, DesktopItem, DesktopItemKind, actions},
    favorites::FavoriteStore,
};
use hearthspace_common::{desktop_context::DesktopContext, desktop_files::FileClipboard};
use std::{path::Path, time::Duration};

#[derive(Clone, Debug)]
pub(super) enum Action {
    Open,
    OpenWithOptions,
    OpenWith(String),
    Cut,
    Copy,
    Paste,
    Rename(String),
    Trash,
    Properties,
    Favorite(bool),
    NewFolder(String),
    SelectAll,
    Arrange,
    OpenDesktop,
    Background,
    Settings,
    Layout,
}

pub(super) struct Request {
    pub generation: u64,
    pub action: Action,
}

#[derive(Debug)]
pub(super) enum Completed {
    Closed(Option<FavoriteStore>),
    OpenWith(Vec<actions::OpenWithApp>),
}

pub(super) fn perform(
    socket: &Path,
    request: Request,
    known: &[DesktopItem],
    mut favorites: FavoriteStore,
    runtime: bool,
) -> Result<Completed, String> {
    #[derive(serde::Deserialize)]
    struct Accepted {
        context: DesktopContext,
        xwayland_display: Option<String>,
    }
    let bytes = command_client::query_payload_bounded(
        socket,
        &format!("desktop-context-read {}", request.generation),
        Duration::from_millis(750),
        12 * 1024 * 1024,
    )
    .map_err(|e| e.to_string())?;
    let accepted: Accepted = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let selected: Vec<_> = accepted
        .context
        .ids
        .iter()
        .map(|id| {
            known
                .iter()
                .find(|item| &item.id == id)
                .cloned()
                .ok_or("This item is no longer available.".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let one = || {
        selected
            .first()
            .filter(|_| selected.len() == 1)
            .ok_or("Select one item for this action.")
    };
    let directory = || {
        if let [
            DesktopItem {
                kind: DesktopItemKind::Folder { path },
                ..
            },
        ] = selected.as_slice()
        {
            return Ok(path.clone());
        }
        if !selected.is_empty() {
            return Err("Select a folder or right-click an empty area of the canvas.".to_string());
        }
        desktop_items::desktop_directory()
            .ok_or("The desktop folder is not configured.".to_string())
    };
    match request.action {
        Action::Open => {
            for item in &selected {
                match &item.kind {
                    DesktopItemKind::App { app_id, .. } => command_client::launch_app(
                        socket,
                        app_id,
                        accepted.context.output.as_deref(),
                    )?,
                    DesktopItemKind::File { path } | DesktopItemKind::Folder { path } => {
                        desktop_items::open_path(path, accepted.xwayland_display.as_deref())?
                    }
                }
            }
        }
        Action::OpenWithOptions => {
            return Ok(Completed::OpenWith(actions::open_with_apps(
                &actions::paths(std::slice::from_ref(one()?))?[0],
            )?));
        }
        Action::OpenWith(id) => actions::open_with(
            &actions::paths(&selected)?,
            &id,
            accepted.xwayland_display.as_deref(),
        )?,
        Action::Cut | Action::Copy => {
            actions::paths(&selected)?;
            command_client::send_command(
                socket,
                &format!(
                    "desktop-context-{} {}",
                    if matches!(request.action, Action::Cut) {
                        "cut"
                    } else {
                        "copy"
                    },
                    request.generation
                ),
            )?;
        }
        Action::Paste => {
            let destination = directory()?;
            let bytes = command_client::query_payload_bounded(
                socket,
                "desktop-files-clipboard-read",
                Duration::from_secs(3),
                12 * 1024 * 1024,
            )
            .map_err(|e| e.to_string())?;
            let clipboard: FileClipboard =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let paths = clipboard
                .uris
                .iter()
                .map(|uri| hearthspace_common::desktop_files::local_path(uri))
                .collect::<Result<Vec<_>, _>>()?;
            actions::paste(&paths, &destination, clipboard.cut)?;
            if clipboard.cut
                && let Some(generation) = clipboard.generation
            {
                let _ = command_client::send_command(
                    socket,
                    &format!(
                        "desktop-files-clipboard-clear {}",
                        serde_json::json!({"generation": generation})
                    ),
                );
            }
        }
        Action::Rename(name) => {
            actions::rename(&actions::paths(std::slice::from_ref(one()?))?[0], &name)?
        }
        Action::NewFolder(name) => actions::new_folder(&directory()?, &name)?,
        Action::Trash => actions::trash(&actions::paths(&selected)?)?,
        Action::Properties => actions::properties(&selected)?,
        Action::Favorite(add) => {
            let DesktopItemKind::App { app_id, .. } = &one()?.kind else {
                return Err("Only applications can be favorites.".into());
            };
            if runtime {
                favorites = FavoriteStore::load_default().map_err(|e| e.to_string())?;
            }
            if add {
                favorites.add(app_id)
            } else {
                favorites.remove(app_id)
            }
            .map_err(|e| e.to_string())?;
            return Ok(Completed::Closed(Some(favorites)));
        }
        Action::SelectAll => command_client::send_command(
            socket,
            &format!("desktop-context-select-all {}", request.generation),
        )?,
        Action::Arrange => command_client::send_command(
            socket,
            &format!("desktop-context-arrange {}", request.generation),
        )?,
        Action::OpenDesktop => desktop_items::open_path(
            &desktop_items::desktop_directory().ok_or("The desktop folder is not configured.")?,
            accepted.xwayland_display.as_deref(),
        )?,
        Action::Background => command_client::send_command(
            socket,
            &format!("desktop-context-background {}", request.generation),
        )?,
        Action::Settings => command_client::launch_app(
            socket,
            "dev.hearthspace.Settings.desktop",
            accepted.context.output.as_deref(),
        )?,
        Action::Layout => command_client::send_command(socket, "layout-tool")?,
    }
    Ok(Completed::Closed(None))
}
