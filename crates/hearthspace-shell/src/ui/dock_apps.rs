//! Saved pins and live windows share the installed catalog's application identity.
use std::collections::BTreeMap;

use super::ShellState;
use crate::{app_catalog::DesktopApp, ui::window_catalog::RunningWindow};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Key {
    App(String),
    Window(u64),
}

pub(super) struct App {
    pub key: Key,
    pub name: String,
    pub icon: Option<String>,
    pub windows: Vec<RunningWindow>,
    pub pinned: bool,
}

impl App {
    fn installed(app: &DesktopApp, pinned: bool) -> Self {
        Self {
            key: Key::App(app.id.clone()),
            name: app.name.clone(),
            icon: app.icon.clone(),
            windows: Vec::new(),
            pinned,
        }
    }

    pub fn focused(&self) -> bool {
        self.windows.iter().any(|window| window.focused)
    }

    pub fn next_window(&self) -> Option<&RunningWindow> {
        if self.windows.is_empty() {
            return None;
        }
        let next = self
            .windows
            .iter()
            .position(|window| window.focused)
            .map_or(0, |index| (index + 1) % self.windows.len());
        self.windows.get(next)
    }

    pub fn label(&self) -> String {
        let pinned = if self.pinned { ", pinned" } else { "" };
        let focused = if self.focused() { ", active" } else { "" };
        match self.windows.len() {
            0 => format!("Open {}{pinned}", self.name),
            1 => format!("Switch to {}{pinned}{focused}", self.name),
            count => format!(
                "{}: {count} windows{pinned}{focused}. Next: {}",
                self.name,
                self.next_window().unwrap().title
            ),
        }
    }
}

pub(super) fn apps(state: &ShellState) -> Vec<App> {
    let mut grouped = BTreeMap::<Key, App>::new();
    let mut pins = Vec::new();
    for favorite in state.favorites.store.entries() {
        if let Some(installed) = state.catalog.app_by_id(&favorite.app_id) {
            let app = App::installed(installed, true);
            grouped.entry(app.key.clone()).or_insert_with(|| {
                pins.push(app.key.clone());
                app
            });
        }
    }
    let focused = state
        .windows
        .iter()
        .find(|window| window.focused)
        .map(|window| window.id)
        .or(state.dock.last_focused_window);
    let mut windows = state.windows.iter().collect::<Vec<_>>();
    windows.sort_by_key(|window| window.id);
    for window in windows {
        let installed = state.catalog.app_for_window(&window.app_id);
        let key = if let Some(app) = installed {
            Key::App(app.id.clone())
        } else if window.app_id.is_empty() {
            Key::Window(window.id)
        } else {
            Key::App(window.app_id.clone())
        };
        let app = grouped.entry(key.clone()).or_insert_with(|| {
            installed.map_or_else(
                || App {
                    key,
                    name: if !window.app_id.is_empty() {
                        window.app_id.clone()
                    } else if !window.title.is_empty() {
                        window.title.clone()
                    } else {
                        "Untitled window".into()
                    },
                    icon: (!window.app_id.is_empty()).then(|| window.app_id.clone()),
                    windows: Vec::new(),
                    pinned: false,
                },
                |app| App::installed(app, false),
            )
        });
        let mut window = window.clone();
        window.focused = Some(window.id) == focused;
        app.windows.push(window);
    }
    // Saved pin order stays stable as apps open and close. Unpinned running apps
    // follow in deterministic catalog identity order; unknown windows stay apart.
    let mut apps = pins
        .iter()
        .filter_map(|key| grouped.remove(key))
        .collect::<Vec<_>>();
    apps.extend(grouped.into_values());
    apps
}
