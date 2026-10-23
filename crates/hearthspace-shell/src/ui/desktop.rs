//! Canvas items are independent transparent surfaces; the compositor owns their
//! placement and selection so moving them never waits for a shell repaint.
use std::{collections::BTreeMap, sync::Arc};

use hearthspace_common::{DESKTOP_SURFACE_HEIGHT, DESKTOP_SURFACE_WIDTH, desktop_item_app_id};
use tokio::sync::{Notify, mpsc};
use xilem::{WindowId, WindowView, dpi::LogicalSize};

use crate::desktop_items::{DesktopItem, DesktopItemKind};

use super::{desktop_item, state::ShellState};

#[path = "desktop/events.rs"]
mod events;
#[path = "desktop/task.rs"]
mod task;
#[cfg(test)]
#[path = "desktop/tests.rs"]
mod tests;

pub(super) use events::{Event, apply_event, parse_event};
pub(super) use task::task;

pub(super) fn selection_active(state: &ShellState) -> bool {
    state.desktop.snapshot.select_mode
}

#[derive(Default)]
pub(super) struct State {
    items: Vec<Item>,
    snapshot: events::Snapshot,
    requests: Option<mpsc::Sender<task::Request>>,
    refresh: Arc<Notify>,
    pub(super) error: Option<String>,
    pub(super) source_error: Option<String>,
}

struct Item {
    content: DesktopItem,
    window_id: WindowId,
}

impl State {
    fn replace_items(&mut self, items: Vec<DesktopItem>) {
        let mut previous: BTreeMap<_, _> = std::mem::take(&mut self.items)
            .into_iter()
            .map(|item| (item.content.id, item.window_id))
            .collect();
        self.items = items
            .into_iter()
            .map(|content| Item {
                window_id: previous.remove(&content.id).unwrap_or_else(WindowId::next),
                content,
            })
            .collect();
    }
}

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    state
        .desktop
        .items
        .iter()
        .map(|item| {
            let id = item.content.id.clone();
            let content = desktop_item::TileContent {
                name: item.content.title.clone(),
                artwork: match &item.content.kind {
                    DesktopItemKind::App { app_id, .. }
                        if app_id == "dev.hearthspace.Settings.desktop" =>
                    {
                        desktop_item::Artwork::Settings
                    }
                    DesktopItemKind::App { icon, .. } => {
                        desktop_item::Artwork::App(super::dock::image(state, icon.as_deref()))
                    }
                    DesktopItemKind::Folder { .. } => desktop_item::Artwork::Folder,
                    DesktopItemKind::File { .. } => desktop_item::Artwork::File,
                },
            };
            let flags = state.desktop.snapshot.items.get(&id);
            let search = &state.canvas_search.snapshot;
            let view = desktop_item::view(
                content,
                desktop_item::TileState {
                    selected: if search.active {
                        search.selected_id.as_ref() == Some(&id)
                    } else {
                        flags.is_some_and(|item| item.selected)
                    },
                    focused: !search.active && flags.is_some_and(|item| item.focused),
                    keyboard_focus: !search.active && flags.is_some_and(|item| item.keyboard_focus),
                    hovered: search.active && search.hovered_id.as_ref() == Some(&id),
                    photo_background: state.appearance.palette.photo,
                },
                move |state: &mut ShellState, action| {
                    if state.canvas_search.snapshot.active {
                        match action {
                            desktop_item::TileAction::Activate => super::canvas_search::activate(state, Some(&id)),
                            desktop_item::TileAction::Focus => super::canvas_search::submit_command(state, format!(
                                "canvas-search-select {}",
                                serde_json::json!({"session":state.canvas_search.snapshot.session,"id":id}),
                            )),
                        }
                        return;
                    }
                    let command = match action {
                        desktop_item::TileAction::Activate => "desktop-activate",
                        desktop_item::TileAction::Focus => "desktop-focus",
                    };
                    task::submit(
                        state,
                        task::Request::Command(format!(
                            "{command} {}",
                            serde_json::to_string(&id).expect("item ID is serializable")
                        )),
                    );
                },
            );
            super::popup::window(
                item.window_id,
                &item.content.title,
                desktop_item_app_id(&item.content.id),
                LogicalSize::new(
                    f64::from(DESKTOP_SURFACE_WIDTH),
                    f64::from(DESKTOP_SURFACE_HEIGHT),
                ),
                view,
                |_| {},
            )
        })
        .collect()
}

pub(super) fn selection_button(state: &ShellState) -> impl xilem::WidgetView<ShellState> + use<> {
    super::icons::icon_button(
        super::icons::Icon::SelectDesktop,
        "Select desktop items",
        state.desktop.snapshot.select_mode,
        |state| {
            state.dismiss_panels();
            task::submit(
                state,
                task::Request::Command("desktop-selection-toggle".into()),
            );
        },
    )
}

fn feedback(state: &mut ShellState, error: String) {
    state.desktop.error = Some(error.clone());
    state.dismiss_panels();
    if let Some(output) = state.outputs.first_mut() {
        output.show_search = true;
        output.launch_error = Some(error);
    }
}
