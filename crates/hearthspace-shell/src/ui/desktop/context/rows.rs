use super::{Action, Page, ShellState};
use crate::desktop_items::DesktopItemKind;

pub(super) struct Row {
    pub name: String,
    pub action: Choice,
    pub submenu: bool,
}
pub(super) enum Choice {
    Action(Action),
    Rename,
    NewFolder,
    Back,
    Close,
}
impl Row {
    fn new(name: &str, action: Choice) -> Self {
        Self {
            name: name.into(),
            action,
            submenu: false,
        }
    }
    fn action(name: &str, action: Action) -> Self {
        Self::new(name, Choice::Action(action))
    }
}

pub(super) fn rows(state: &ShellState) -> Vec<Row> {
    let mut rows = page_rows(state);
    if state.desktop.menu.busy && !matches!(state.desktop.menu.page, Page::Name(_)) {
        rows.push(Row::new("Close", Choice::Close));
    }
    rows
}

fn page_rows(state: &ShellState) -> Vec<Row> {
    let menu = &state.desktop.menu;
    match &menu.page {
        Page::OpenWith(apps) => {
            return std::iter::once(Row::new("Back", Choice::Back))
                .chain(
                    apps.iter()
                        .map(|app| Row::action(&app.name, Action::OpenWith(app.id.clone()))),
                )
                .collect();
        }
        Page::Properties(_) => {
            return vec![
                Row::new("Back", Choice::Back),
                Row::new("Close", Choice::Close),
            ];
        }
        Page::Name(_) => return vec![],
        Page::Menu => {}
    }
    let Some(context) = &menu.current else {
        return vec![];
    };
    let items: Vec<_> = context
        .ids
        .iter()
        .filter_map(|id| state.desktop.items.iter().find(|i| &i.content.id == id))
        .collect();
    if items.is_empty() {
        return vec![
            Row::new("New Folder…", Choice::NewFolder),
            Row::action("Paste", Action::Paste),
            Row::action("Select All", Action::SelectAll),
            Row::action("Arrange Icons", Action::Arrange),
            Row::action("Open Desktop Folder", Action::OpenDesktop),
            Row::action("Change Background…", Action::Background),
            Row::action("Change Layout…", Action::Layout),
            Row::action("Settings", Action::Settings),
        ];
    }
    let mut rows = vec![Row::action("Open", Action::Open)];
    if items
        .iter()
        .all(|item| !matches!(item.content.kind, DesktopItemKind::App { .. }))
    {
        if items.len() == 1 {
            let mut row = Row::action("Open With…", Action::OpenWithOptions);
            row.submenu = true;
            rows.push(row);
        }
        rows.extend([
            Row::action("Cut", Action::Cut),
            Row::action("Copy", Action::Copy),
        ]);
        if let [item] = items.as_slice() {
            if matches!(item.content.kind, DesktopItemKind::Folder { .. }) {
                rows.push(Row::action("Paste Into Folder", Action::Paste));
                rows.push(Row::new("New Folder…", Choice::NewFolder));
            }
            rows.push(Row::new("Rename…", Choice::Rename));
        }
        rows.push(Row::action("Move to Trash", Action::Trash));
    } else if let [item] = items.as_slice()
        && let DesktopItemKind::App { app_id, .. } = &item.content.kind
    {
        let favorite = state
            .favorites
            .store
            .entries()
            .iter()
            .any(|f| &f.app_id == app_id);
        rows.push(Row::action(
            if favorite {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            },
            Action::Favorite(!favorite),
        ));
    }
    if items.len() == 1
        || items
            .iter()
            .all(|item| !matches!(item.content.kind, DesktopItemKind::App { .. }))
    {
        rows.push(Row::action("Properties", Action::Properties));
    }
    rows
}
