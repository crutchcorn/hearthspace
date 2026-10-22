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
    OpenWith(Vec<crate::desktop_items::actions::OpenWithApp>),
    Properties(Vec<(String, String)>),
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
            state.desktop.menu.page = Page::OpenWith(apps);
            state.desktop.menu.selected = 0;
        }
        Ok(Completed::Properties(info)) => {
            state.desktop.menu.page = Page::Properties(info);
            state.desktop.menu.selected = 0;
        }
        Err(error) => state.desktop.menu.error = Some(error),
    }
}

pub(in crate::ui) fn button_event(
    state: &mut ShellState,
    generation: u64,
    index: usize,
    action: ButtonAction,
) {
    if state
        .desktop
        .menu
        .current
        .as_ref()
        .is_none_or(|c| c.generation != generation)
    {
        return;
    }
    if state.desktop.menu.busy
        && !matches!(action, ButtonAction::Dismiss)
        && !(matches!(action, ButtonAction::Activate)
            && rows::rows(state)
                .get(index)
                .is_some_and(|row| matches!(row.action, rows::Choice::Close)))
    {
        return;
    }
    match action {
        ButtonAction::Dismiss => dismiss(state, generation),
        ButtonAction::Back => {
            state.desktop.menu.page = Page::Menu;
            state.desktop.menu.selected = 0;
        }
        ButtonAction::Step(delta) => {
            let count = rows::rows(state).len();
            if count > 0 {
                state.desktop.menu.selected =
                    (state.desktop.menu.selected as i32 + delta).rem_euclid(count as i32) as usize;
            }
        }
        ButtonAction::Focus => state.desktop.menu.selected = index,
        ButtonAction::Activate => {
            let Some(row) = rows::rows(state).into_iter().nth(index) else {
                return;
            };
            match row.action {
                rows::Choice::Action(action) => send(state, action),
                rows::Choice::Rename => {
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
                    state.desktop.menu.text = "New Folder".into();
                    state.desktop.menu.page = Page::Name(true);
                }
                rows::Choice::Back => {
                    state.desktop.menu.page = Page::Menu;
                    state.desktop.menu.selected = 0;
                }
                rows::Choice::Close => dismiss(state, generation),
            }
        }
        _ => {}
    }
}

pub(super) fn window(state: &ShellState) -> Option<WindowView<ShellState>> {
    view::window(state)
}
