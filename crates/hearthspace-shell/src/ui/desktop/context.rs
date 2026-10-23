//! Desktop context menus reuse the shell's native, keyboard-accessible menu rows.
use super::{
    ShellState,
    context_actions::{self, Action, Completed},
    task,
};
use crate::ui::tray_widget::ButtonAction;
use hearthspace_common::desktop_context::DesktopContext;
use xilem::{WindowId, WindowView};

mod rows;
#[cfg(test)]
mod tests;
mod view;

#[derive(Default)]
pub(super) struct State {
    pub current: Option<DesktopContext>,
    dismissed: u64,
    window: Option<WindowId>,
    page: Page,
    submenu: Option<Submenu>,
    row_y: Option<u32>,
    hover_blocked: bool,
    selected: usize,
    pub busy: bool,
    error: Option<String>,
    text: String,
    focus: crate::ui::search_focus::FocusTarget,
}

#[derive(Default)]
enum Page {
    #[default]
    Menu,
    Name(bool), // New folder, otherwise rename.
}

struct Submenu {
    window: WindowId,
    parent: usize,
    row_y: u32,
    apps: Option<Vec<crate::desktop_items::actions::OpenWithApp>>,
    selected: usize,
}

pub(super) fn apply(state: &mut ShellState, context: Option<DesktopContext>) {
    let dismissed = state.desktop.menu.dismissed;
    let context = context.filter(|context| context.generation > dismissed);
    if state.desktop.menu.current.as_ref().map(|c| c.generation)
        == context.as_ref().map(|c| c.generation)
    {
        return;
    }
    state.desktop.menu = State {
        window: context.as_ref().map(|_| WindowId::next()),
        current: context,
        dismissed,
        ..Default::default()
    };
}

pub(super) fn dismiss(state: &mut ShellState, generation: u64) {
    if state
        .desktop
        .menu
        .current
        .as_ref()
        .is_some_and(|c| c.generation == generation)
    {
        state.desktop.menu = State {
            dismissed: generation,
            ..Default::default()
        };
    }
    task::submit(
        state,
        task::Request::Command(format!("desktop-context-close {generation}")),
    );
}

fn send(state: &mut ShellState, action: Action) {
    let Some(context) = &state.desktop.menu.current else {
        return;
    };
    if state.desktop.menu.busy {
        return;
    }
    let generation = context.generation;
    state.desktop.menu.busy = true;
    state.desktop.menu.error = None;
    task::submit(
        state,
        task::Request::Context(context_actions::Request { generation, action }),
    );
}

pub(super) fn finished(state: &mut ShellState, generation: u64, result: Result<Completed, String>) {
    if state
        .desktop
        .menu
        .current
        .as_ref()
        .is_none_or(|c| c.generation != generation)
    {
        return;
    }
    state.desktop.menu.busy = false;
    match result {
        Ok(Completed::Closed(favorites)) => {
            if let Some(favorites) = favorites {
                state.favorites.store = favorites;
            }
            dismiss(state, generation);
        }
        Ok(Completed::OpenWith(apps)) => {
            if let Some(submenu) = &mut state.desktop.menu.submenu {
                submenu.apps = Some(apps);
            }
        }
        Err(error) => state.desktop.menu.error = Some(error),
    }
}

pub(in crate::ui) fn button_event(
    state: &mut ShellState,
    generation: u64,
    submenu: bool,
    index: usize,
    action: ButtonAction,
) {
    if state
        .desktop
        .menu
        .current
        .as_ref()
        .is_none_or(|c| c.generation != generation)
        || (submenu && state.desktop.menu.submenu.is_none())
    {
        return;
    }
    if state.desktop.menu.busy
        && !matches!(action, ButtonAction::Dismiss | ButtonAction::Back)
        && !(matches!(action, ButtonAction::Activate)
            && rows::rows(state, submenu)
                .get(index)
                .is_some_and(|row| matches!(row.action, rows::Choice::Close | rows::Choice::Back)))
    {
        return;
    }
    match action {
        ButtonAction::Dismiss | ButtonAction::Back if submenu => back(state, generation),
        ButtonAction::Dismiss | ButtonAction::Back => dismiss(state, generation),
        ButtonAction::Step(delta) => {
            if !submenu && state.desktop.menu.submenu.is_some() {
                back(state, generation);
            }
            let count = rows::rows(state, submenu).len();
            if count > 0 {
                let selected = selection(state, submenu);
                *selected = (*selected as i32 + delta).rem_euclid(count as i32) as usize;
            }
        }
        ButtonAction::Focus => *selection(state, submenu) = index,
        ButtonAction::MenuAnchor(y) if !submenu => {
            state.desktop.menu.row_y = Some((y - 6.0).round().clamp(0.0, 16_384.0) as u32);
        }
        ButtonAction::Hover(false) if !submenu => state.desktop.menu.hover_blocked = false,
        ButtonAction::Hover(true) if !submenu && !state.desktop.menu.hover_blocked => {
            if rows::rows(state, false)
                .get(index)
                .is_some_and(|row| row.submenu)
            {
                open_submenu(state, index);
            } else if state.desktop.menu.submenu.is_some() {
                back(state, generation);
                state.desktop.menu.selected = index;
            }
        }
        ButtonAction::Activate => {
            let Some(row) = rows::rows(state, submenu).into_iter().nth(index) else {
                return;
            };
            match row.action {
                rows::Choice::Action(Action::OpenWithOptions) => {
                    state.desktop.menu.hover_blocked = false;
                    if state.desktop.menu.submenu.is_some() {
                        task::submit(
                            state,
                            task::Request::Command(format!("desktop-context-child {generation}")),
                        );
                    } else {
                        open_submenu(state, index);
                    }
                }
                rows::Choice::Action(action) => send(state, action),
                rows::Choice::Rename => {
                    state.desktop.menu.submenu = None;
                    state.desktop.menu.text = state
                        .desktop
                        .menu
                        .current
                        .as_ref()
                        .and_then(|c| c.ids.first())
                        .and_then(|id| state.desktop.items.iter().find(|i| &i.content.id == id))
                        .map(|i| i.content.title.clone())
                        .unwrap_or_default();
                    state.desktop.menu.page = Page::Name(false);
                }
                rows::Choice::NewFolder => {
                    state.desktop.menu.submenu = None;
                    state.desktop.menu.text = "New Folder".into();
                    state.desktop.menu.page = Page::Name(true);
                }
                rows::Choice::Close => dismiss(state, generation),
                rows::Choice::Back => back(state, generation),
            }
        }
        _ => {}
    }
}

fn selection(state: &mut ShellState, submenu: bool) -> &mut usize {
    if submenu && let Some(submenu) = &mut state.desktop.menu.submenu {
        &mut submenu.selected
    } else {
        &mut state.desktop.menu.selected
    }
}

fn open_submenu(state: &mut ShellState, parent: usize) {
    if state.desktop.menu.submenu.is_some() || state.desktop.menu.busy {
        return;
    }
    state.desktop.menu.selected = parent;
    state.desktop.menu.submenu = Some(Submenu {
        window: WindowId::next(),
        parent,
        row_y: state.desktop.menu.row_y.unwrap_or((parent * 34) as u32),
        apps: None,
        selected: 0,
    });
    send(state, Action::OpenWithOptions);
}

fn back(state: &mut ShellState, generation: u64) {
    if let Some(submenu) = state.desktop.menu.submenu.take() {
        state.desktop.menu.hover_blocked = true;
        state.desktop.menu.selected = submenu.parent;
        state.desktop.menu.error = None;
        task::submit(
            state,
            task::Request::Command(format!("desktop-context-parent {generation}")),
        );
    }
}

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    view::windows(state)
}
