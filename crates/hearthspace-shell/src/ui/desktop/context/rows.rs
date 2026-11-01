use super::super::context_items;
use super::{Action, Page, ShellState};
use crate::desktop_items::DesktopItemKind;

pub(super) struct Row {
    pub name: String,
    pub action: Choice,
    pub submenu: bool,
    pub checked: Option<bool>,
}
pub(super) enum Choice {
    Action(Action),
    Rename,
    NewFolder,
    Close,
    Back,
}
impl Row {
    fn new(name: &str, action: Choice) -> Self {
        Self {
            name: name.into(),
            action,
            submenu: false,
            checked: None,
        }
    }
    fn action(name: &str, action: Action) -> Self {
        Self::new(name, Choice::Action(action))
    }
}

pub(super) fn rows(state: &ShellState, submenu: bool) -> Vec<Row> {
    let mut rows = if submenu {
        state
            .desktop
            .menu
            .submenu
            .as_ref()
            .and_then(|menu| menu.apps.as_ref())
            .into_iter()
            .flatten()
            .map(|app| Row::action(&app.name, Action::OpenWith(app.id.clone())))
            .collect()
    } else {
        page_rows(state)
    };
    if submenu && rows.is_empty() {
        rows.push(Row::new("Back", Choice::Back));
    }
    if state.desktop.menu.busy
        && state.desktop.menu.submenu.is_none()
        && !matches!(state.desktop.menu.page, Page::Name(_))
        && !rows.iter().any(|row| matches!(row.action, Choice::Close))
    {
        rows.push(Row::new("Close", Choice::Close));
    }
    rows
}

fn page_rows(state: &ShellState) -> Vec<Row> {
    let menu = &state.desktop.menu;
    match &menu.page {
        Page::Name(_) => return vec![],
        Page::Menu => {}
    }
    let Some(context) = &menu.current else {
        return vec![];
    };
    if context.ids.is_empty() {
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
    let Some(items) = context
        .ids
        .iter()
        .map(|id| {
            context_items::resolve(
                id,
                &state.catalog,
                state.desktop.items.iter().map(|item| &item.content),
            )
        })
        .collect::<Option<Vec<_>>>()
    else {
        return vec![Row::new("Close", Choice::Close)];
    };
    let mut rows = vec![Row::action("Open", Action::Open)];
    if items
        .iter()
        .all(|item| !matches!(item.kind, DesktopItemKind::App { .. }))
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
            if matches!(item.kind, DesktopItemKind::Folder { .. }) {
                rows.push(Row::action("Paste Into Folder", Action::Paste));
                rows.push(Row::new("New Folder…", Choice::NewFolder));
            }
            rows.push(Row::new("Rename…", Choice::Rename));
        }
        rows.push(Row::action("Move to Trash", Action::Trash));
    } else if let [item] = items.as_slice()
        && let DesktopItemKind::App { app_id, .. } = &item.kind
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
        if let Some((_, checked)) = gpu_preference(state) {
            let mut row = Row::action(
                "Run using dedicated graphics card",
                Action::GpuPreference(!checked),
            );
            row.checked = Some(checked);
            rows.push(row);
        }
    }
    if items.len() == 1
        || items
            .iter()
            .all(|item| !matches!(item.kind, DesktopItemKind::App { .. }))
    {
        rows.push(Row::action("Properties", Action::Properties));
    }
    rows
}

pub(super) const GPU_HELP: &str =
    "Applies when the app starts. Running apps keep their current GPU.";

pub(super) fn gpu_preference(state: &ShellState) -> Option<(&str, bool)> {
    let context = state.desktop.menu.current.as_ref()?;
    let [id] = context.ids.as_slice() else {
        return None;
    };
    let app = context_items::app(&state.catalog, id)?;
    Some((&app.id, app.prefers_non_default_gpu))
}
