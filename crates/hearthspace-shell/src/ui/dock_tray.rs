//! Share a status menu only when one unambiguous, visible dock app represents it.
use super::{App, Key, ShellState, apps};
use crate::{
    tray::{ItemKey, TrayItem, TrayStatus},
    ui::{
        frame_layout, tray_button, tray_state,
        tray_widget::{Appearance, ButtonAction},
    },
};

fn item_for_app<'a>(state: &'a ShellState, app: &str) -> Option<&'a TrayItem> {
    let mut matches = tray_state::visible(&state.tray.snapshot).filter(|item| {
        state
            .catalog
            .app_for_status_item(&item.id)
            .is_some_and(|entry| entry.id == app)
    });
    let item = matches.next()?;
    // Two items can expose different menus for the same application. Keep both
    // in the tray instead of silently choosing one, even if only one has a menu.
    if matches.next().is_some()
        || item.menu.is_none()
        || item.menu_path.is_none()
        || item.menu_error.is_some()
        || !state
            .tray
            .handle
            .as_ref()
            .is_some_and(crate::tray::TrayHandle::is_available)
    {
        return None;
    }
    Some(item)
}

pub(super) fn button(
    state: &ShellState,
    output: &str,
    app: &App,
) -> Option<tray_button::ButtonView> {
    let Key::App(id) = &app.key else { return None };
    let item = item_for_app(state, id)?;
    Some(tray_button::ButtonView {
        output: output.to_owned(),
        target: tray_button::Target::Dock {
            app: id.clone(),
            item: item.key.clone(),
            menu_path: item.menu_path.clone()?,
        },
        appearance: Appearance {
            name: app.label(),
            description: format!(
                "{}. Application menu: secondary click, Menu key, or touch and hold.",
                item.tooltip
            ),
            icon: true,
            dock: Some(super::icons::Glyph::App {
                image: super::image(state, app.icon.as_deref()),
                count: app.windows.len(),
                focused: app.focused(),
            }),
            selected: app.focused(),
            attention: item.status == TrayStatus::NeedsAttention,
            ..Default::default()
        },
    })
}

pub(in crate::ui) fn merged_items(state: &ShellState, output: &str) -> Vec<ItemKey> {
    let Some(width) = frame_layout::dock_menu_width(state, output) else {
        return Vec::new();
    };
    let Some(output) = state.outputs.iter().find(|item| item.key() == output) else {
        return Vec::new();
    };
    let apps = apps(state);
    // The tiny single-control cycle fallback has no stable app/menu target.
    if width < super::MIN_OVERFLOW_WIDTH && (apps.len() > 1 || width < super::BUTTON_WIDTH) {
        return Vec::new();
    }
    let capacity = super::capacity(width, apps.len());
    let pages = apps.len().div_ceil(capacity).max(1);
    apps.iter()
        .skip(output.dock_page.min(pages - 1) * capacity)
        .take(capacity)
        .filter_map(|app| match &app.key {
            Key::App(id) => item_for_app(state, id).map(|item| item.key.clone()),
            Key::Window(_) => None,
        })
        .collect()
}

fn current(state: &ShellState, app: &str, key: &ItemKey, path: &str) -> bool {
    item_for_app(state, app)
        .is_some_and(|item| item.key == *key && item.menu_path.as_deref() == Some(path))
        && apps(state)
            .iter()
            .any(|item| item.key == Key::App(app.to_owned()))
}

pub(in crate::ui) fn event(
    state: &mut ShellState,
    output: &str,
    app: &str,
    key: &ItemKey,
    path: &str,
    action: ButtonAction,
) {
    if !current(state, app, key, path) {
        return;
    }
    match action {
        ButtonAction::Activate => super::activate(state, output, &Key::App(app.to_owned())),
        ButtonAction::Context => {
            if !frame_layout::popups::defer(
                state,
                output,
                frame_layout::popups::Popup::DockTrayMenu {
                    app: app.to_owned(),
                    item: key.clone(),
                    menu_path: path.to_owned(),
                },
            ) {
                tray_state::open_menu(state, output, key);
            }
        }
        ButtonAction::Secondary => tray_state::send(
            state,
            output,
            crate::tray::TrayCommand::Secondary {
                key: key.clone(),
                x: 0,
                y: 0,
            },
        ),
        ButtonAction::Scroll { delta, horizontal } => tray_state::send(
            state,
            output,
            crate::tray::TrayCommand::Scroll {
                key: key.clone(),
                delta,
                horizontal,
            },
        ),
        ButtonAction::Dismiss => tray_state::dismiss(state, output),
        _ => {}
    }
}

pub(in crate::ui) fn open_menu(
    state: &mut ShellState,
    output: &str,
    app: &str,
    key: &ItemKey,
    path: &str,
) {
    // An anchor reply cannot resurrect a removed/reconnected item or open a
    // replacement menu, nor target an app that has left the visible dock page.
    if current(state, app, key, path) && merged_items(state, output).contains(key) {
        tray_state::open_menu(state, output, key);
    }
}
