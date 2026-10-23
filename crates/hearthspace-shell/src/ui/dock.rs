//! The running-app strip: stable application groups and live window activation.
use std::collections::BTreeMap;

use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, peniko::ImageBrush},
    style::Style,
    view::{button, flex_row, sized_box},
};

use super::{
    command_client::send_command,
    state::{ShellOutputInstance, ShellState},
    window_catalog::RunningWindow,
};

#[path = "dock_icons.rs"]
mod icons;
#[path = "dock_identity.rs"]
mod identity;
#[cfg(test)]
#[path = "dock_tests.rs"]
mod tests;

pub(super) use icons::task;

const BUTTON_WIDTH: f64 = 44.0;
const GAP: f64 = 4.0;
const MAX_VISIBLE: usize = 8;
const MIN_OVERFLOW_WIDTH: f64 = BUTTON_WIDTH + GAP + 20.0;

#[derive(Default)]
pub(super) struct State {
    images: BTreeMap<String, Option<ImageBrush>>,
    requests: Option<xilem::tokio::sync::watch::Sender<Vec<String>>>,
    last_focused_window: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    App(String),
    Window(u64),
}

struct App {
    key: Key,
    name: String,
    icon: Option<String>,
    windows: Vec<RunningWindow>,
}

impl App {
    fn focused(&self) -> bool {
        self.windows.iter().any(|window| window.focused)
    }

    fn next_window(&self) -> &RunningWindow {
        let next = self
            .windows
            .iter()
            .position(|window| window.focused)
            .map_or(0, |index| (index + 1) % self.windows.len());
        &self.windows[next]
    }

    fn label(&self) -> String {
        let focused = if self.focused() { ", active" } else { "" };
        if self.windows.len() == 1 {
            format!("Switch to {}{focused}", self.name)
        } else {
            format!(
                "{}: {} windows{focused}. Next: {}",
                self.name,
                self.windows.len(),
                self.next_window().title
            )
        }
    }
}

fn apps(state: &ShellState) -> Vec<App> {
    let mut grouped = BTreeMap::<Key, Vec<RunningWindow>>::new();
    let focused = state
        .windows
        .iter()
        .find(|window| window.focused)
        .map(|window| window.id)
        .or(state.dock.last_focused_window);
    for window in &state.windows {
        let key = if window.app_id.is_empty() {
            Key::Window(window.id)
        } else {
            Key::App(window.app_id.clone())
        };
        let mut window = window.clone();
        window.focused = Some(window.id) == focused;
        grouped.entry(key).or_default().push(window);
    }
    grouped
        .into_iter()
        .map(|(key, mut windows)| {
            windows.sort_by_key(|window| window.id);
            let app_id = &windows[0].app_id;
            let installed = state.catalog.apps().iter().find(|app| {
                app.id.strip_suffix(".desktop").unwrap_or(&app.id)
                    == app_id.strip_suffix(".desktop").unwrap_or(app_id)
            });
            let name = installed.map(|app| app.name.clone()).unwrap_or_else(|| {
                if !app_id.is_empty() {
                    app_id.clone()
                } else if !windows[0].title.is_empty() {
                    windows[0].title.clone()
                } else {
                    "Untitled window".into()
                }
            });
            let icon = installed
                .and_then(|app| app.icon.clone())
                .or_else(|| (!app_id.is_empty()).then(|| app_id.clone()));
            App {
                key,
                name,
                icon,
                windows,
            }
        })
        .collect()
}

/// A single wrapping page control keeps every app reachable on small outputs.
fn capacity(width: f64, count: usize) -> usize {
    let slots = ((width + GAP).max(0.0) / (BUTTON_WIDTH + GAP))
        .floor()
        .min(MAX_VISIBLE as f64) as usize;
    if count > slots {
        slots.saturating_sub(1).max(1)
    } else {
        slots.max(1)
    }
}

pub(super) fn desired_width(state: &ShellState) -> f64 {
    let count = apps(state).len().min(MAX_VISIBLE);
    if count == 0 {
        0.0
    } else {
        count as f64 * (BUTTON_WIDTH + GAP) - GAP
    }
}

pub(super) fn reconcile(state: &mut ShellState) {
    // Focusing a shell control temporarily removes app focus. Preserve the last
    // app so a held pointer press or keyboard navigation does not reset cycling.
    if let Some(window) = state.windows.iter().find(|window| window.focused) {
        state.dock.last_focused_window = Some(window.id);
    } else if !state
        .windows
        .iter()
        .any(|window| Some(window.id) == state.dock.last_focused_window)
    {
        state.dock.last_focused_window = None;
    }
    if let Some(requests) = &state.dock.requests {
        let names = icons::requests(state);
        requests.send_if_modified(|previous| {
            if *previous == names {
                false
            } else {
                *previous = names;
                true
            }
        });
    }
}

pub(super) fn image(state: &ShellState, name: Option<&str>) -> Option<ImageBrush> {
    name.and_then(|name| state.dock.images.get(name))
        .and_then(Clone::clone)
}

pub(super) fn view(
    state: &ShellState,
    output: &ShellOutputInstance,
    available_width: f64,
) -> impl WidgetView<ShellState> + use<> {
    let apps = apps(state);
    if available_width < MIN_OVERFLOW_WIDTH && (apps.len() > 1 || available_width < BUTTON_WIDTH) {
        let next = apps
            .iter()
            .position(App::focused)
            .map_or(0, |index| (index + 1) % apps.len());
        let mut buttons = Vec::<Box<AnyWidgetView<ShellState>>>::new();
        if let Some(app) = apps.get(next)
            && available_width >= 2.0
        {
            let output_key = output.key().to_owned();
            buttons.push(
                dock_button(
                    icons::Glyph::Next,
                    format!("Next running app: {}", app.name),
                    false,
                    available_width.min(BUTTON_WIDTH),
                    move |state| {
                        // This remains the same control as its next-app label
                        // changes, preserving focus for repeated keyboard use.
                        let apps = self::apps(state);
                        let next = apps
                            .iter()
                            .position(App::focused)
                            .map_or(0, |index| (index + 1) % apps.len());
                        if let Some(app) = apps.get(next) {
                            activate(state, &output_key, &app.key);
                        }
                    },
                )
                .boxed(),
            );
        }
        return flex_row(buttons).gap(GAP.px());
    }
    let capacity = capacity(available_width, apps.len());
    let pages = apps.len().div_ceil(capacity).max(1);
    let page = output.dock_page.min(pages - 1);
    let mut buttons: Vec<Box<AnyWidgetView<ShellState>>> = apps
        .iter()
        .skip(page * capacity)
        .take(capacity)
        .map(|app| {
            let key = app.key.clone();
            let output_key = output.key().to_owned();
            let image = app
                .icon
                .as_ref()
                .and_then(|name| state.dock.images.get(name))
                .and_then(Clone::clone);
            let focused = app.focused();
            identity::item(
                app.key.clone(),
                dock_button(
                    icons::Glyph::App {
                        image,
                        count: app.windows.len(),
                        focused,
                    },
                    app.label(),
                    focused,
                    BUTTON_WIDTH,
                    move |state| activate(state, &output_key, &key),
                ),
            )
            .boxed()
        })
        .collect();
    if pages > 1 {
        let output_key = output.key().to_owned();
        buttons.push(
            dock_button(
                icons::Glyph::Next,
                format!("Next running apps (page {} of {pages})", page + 1),
                false,
                (available_width - BUTTON_WIDTH - GAP).min(BUTTON_WIDTH),
                move |state| {
                    if let Some(output) = state.outputs.iter_mut().find(|o| o.key() == output_key) {
                        output.dock_page = (page + 1) % pages;
                    }
                },
            )
            .boxed(),
        );
    }
    flex_row(buttons).gap(GAP.px())
}

fn dock_button(
    glyph: icons::Glyph,
    name: String,
    focused: bool,
    width: f64,
    action: impl Fn(&mut ShellState) + Send + Sync + 'static,
) -> impl WidgetView<ShellState> {
    let padding = ((width - 2.0) / 2.0).clamp(0.0, 3.0);
    let content_width = (width - 2.0 - 2.0 * padding).max(0.0);
    components::button_surface(
        button(
            sized_box(icons::IconView {
                glyph,
                name,
                width: content_width,
            })
            .fixed_width(content_width.px())
            .fixed_height(36.0.px()),
            action,
        )
        .padding(padding.px())
        .corner_radius(10.0.px()),
        components::ButtonSurface::Raised,
        focused,
    )
}

fn activate(state: &mut ShellState, output_key: &str, key: &Key) {
    let app = apps(state).into_iter().find(|app| &app.key == key);
    let result = if let Some(app) = app {
        let id = app.next_window().id;
        send_command(&state.command_socket, &format!("focus-window {id}")).map(|()| {
            // Reflect an acknowledged focus immediately so repeated clicks can
            // cycle before the next half-second compositor catalog refresh.
            for window in &mut state.windows {
                window.focused = window.id == id;
            }
            state.dock.last_focused_window = Some(id);
        })
    } else {
        Err("This app has closed".into())
    };
    state.dismiss_panels();
    if let Err(error) = result
        && let Some(output) = state.outputs.iter_mut().find(|o| o.key() == output_key)
    {
        output.show_search = true;
        output.launch_error = Some(format!(
            "Could not switch to this app: {error}. Choose an app from Apps List to reopen it."
        ));
    }
}

pub(super) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: components::Palette,
) {
    properties.insert::<icons::IconWidget, _>(palette);
}
