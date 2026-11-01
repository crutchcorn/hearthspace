//! The app dock: persistent favorites, live window groups and app activation.
use std::collections::BTreeMap;

use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, peniko::ImageBrush},
    style::Style,
    view::{flex_row, sized_box},
};

use super::{
    command_client::send_command,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_desktop::app_catalog::DesktopApp;

#[path = "dock_apps.rs"]
mod applications;
use applications::{App, Key, apps};

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
    images: BTreeMap<icons::CacheKey, Option<ImageBrush>>,
    requests: Option<xilem::tokio::sync::watch::Sender<Vec<icons::Request>>>,
    last_focused_window: Option<u64>,
    compact_last_app: BTreeMap<String, Key>,
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
        if state.dock.last_focused_window != Some(window.id) {
            state.dock.compact_last_app.clear();
        }
        state.dock.last_focused_window = Some(window.id);
    } else if !state
        .windows
        .iter()
        .any(|window| Some(window.id) == state.dock.last_focused_window)
    {
        state.dock.last_focused_window = None;
    }
    if let Some(requests) = &state.dock.requests {
        let next = icons::requests(state);
        requests.send_if_modified(|previous| {
            if *previous == next {
                false
            } else {
                *previous = next;
                true
            }
        });
    }
}

pub(super) fn image(state: &ShellState, name: Option<&str>) -> Option<ImageBrush> {
    name.and_then(|name| state.dock.images.get(&icons::CacheKey::Named(name.into())))
        .and_then(Clone::clone)
}

pub(super) fn app_image(state: &ShellState, app: &DesktopApp) -> Option<ImageBrush> {
    state
        .dock
        .images
        .get(&icons::CacheKey::app(app))
        .and_then(Clone::clone)
}

pub(super) fn window_image(state: &ShellState, identity: &str) -> Option<ImageBrush> {
    if let Some(app) = state.catalog.app_for_window(identity) {
        app_image(state, app)
    } else {
        image(state, Some(identity))
    }
}

#[cfg(test)]
pub(super) fn view(
    state: &ShellState,
    output: &ShellOutputInstance,
    available_width: f64,
) -> impl WidgetView<ShellState> + use<> {
    frame_view(state, output, available_width, false, &Default::default())
}

pub(super) fn frame_view(
    state: &ShellState,
    output: &ShellOutputInstance,
    available_width: f64,
    vertical: bool,
    chain: &super::frame_layout::focus::Chain,
) -> impl WidgetView<ShellState> + use<> {
    let apps = apps(state);
    if available_width < MIN_OVERFLOW_WIDTH && (apps.len() > 1 || available_width < BUTTON_WIDTH) {
        let next = compact_next(state, output.key(), &apps);
        let mut buttons = Vec::<Box<AnyWidgetView<ShellState>>>::new();
        if let Some(app) = apps.get(next)
            && available_width >= 2.0
        {
            let output_key = output.key().to_owned();
            buttons.push(
                super::frame_layout::focus::item(
                    dock_button(
                        icons::Glyph::Next,
                        format!("Next app: {}", app.label()),
                        false,
                        available_width.min(BUTTON_WIDTH),
                        context_target(state, output, &app.key),
                        move |state| {
                            // This remains the same control as its next-app label
                            // changes, preserving focus for repeated keyboard use.
                            let apps = self::apps(state);
                            let next = compact_next(state, &output_key, &apps);
                            if let Some(app) = apps.get(next) {
                                state
                                    .dock
                                    .compact_last_app
                                    .insert(output_key.clone(), app.key.clone());
                                activate(state, &output_key, &app.key);
                            }
                        },
                    ),
                    chain,
                    0,
                )
                .boxed(),
            );
        }
        return flex_row((buttons, None::<Box<AnyWidgetView<ShellState>>>))
            .direction(if vertical {
                xilem::masonry::kurbo::Axis::Vertical
            } else {
                xilem::masonry::kurbo::Axis::Horizontal
            })
            .gap(GAP.px());
    }
    let capacity = capacity(available_width, apps.len());
    let pages = apps.len().div_ceil(capacity).max(1);
    let page = output.dock_page.min(pages - 1);
    let buttons: Vec<Box<AnyWidgetView<ShellState>>> = apps
        .iter()
        .skip(page * capacity)
        .take(capacity)
        .enumerate()
        .map(|(index, app)| {
            let key = app.key.clone();
            let output_key = output.key().to_owned();
            let image = app
                .icon
                .as_ref()
                .and_then(|request| state.dock.images.get(&request.key()))
                .and_then(Clone::clone);
            let focused = app.focused();
            // Track the native button before type erasure introduces a
            // Passthrough widget. Frame focus must land on the action target.
            let control = super::frame_layout::focus::item(
                dock_button(
                    icons::Glyph::App {
                        image,
                        count: app.windows.len(),
                        focused,
                    },
                    app.label(),
                    focused,
                    BUTTON_WIDTH,
                    context_target(state, output, &app.key),
                    move |state| activate(state, &output_key, &key),
                ),
                chain,
                index,
            );
            identity::item(app.key.clone(), control).boxed()
        })
        .collect();
    let pager = (pages > 1).then(|| {
        let output_key = output.key().to_owned();
        super::frame_layout::focus::item(
            dock_button(
                icons::Glyph::Next,
                format!("Next apps (page {} of {pages})", page + 1),
                false,
                (available_width - BUTTON_WIDTH - GAP).min(BUTTON_WIDTH),
                None,
                move |state| {
                    if let Some(output) = state.outputs.iter_mut().find(|o| o.key() == output_key) {
                        output.dock_page = (page + 1) % pages;
                    }
                },
            ),
            chain,
            99,
        )
        .boxed()
    });
    // A partial last page changes the app vector's length, never the pager's
    // sequence slot or native focus identity.
    flex_row((buttons, pager))
        .direction(if vertical {
            xilem::masonry::kurbo::Axis::Vertical
        } else {
            xilem::masonry::kurbo::Axis::Horizontal
        })
        .gap(GAP.px())
}

fn compact_next(state: &ShellState, output_key: &str, apps: &[App]) -> usize {
    let last = state.dock.compact_last_app.get(output_key);
    apps.iter()
        .position(|app| Some(&app.key) == last)
        .or_else(|| apps.iter().position(App::focused))
        .map_or(0, |index| (index + 1) % apps.len())
}

fn dock_button(
    glyph: icons::Glyph,
    name: String,
    focused: bool,
    width: f64,
    context: Option<super::app_context::Target>,
    action: impl Fn(&mut ShellState) + Send + Sync + 'static,
) -> impl WidgetView<ShellState> {
    let padding = ((width - 2.0) / 2.0).clamp(0.0, 3.0);
    let content_width = (width - 2.0 - 2.0 * padding).max(0.0);
    super::search_result::app_control(
        name.clone(),
        focused,
        context,
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
    .corner_radius(10.0.px())
}

fn context_target(
    state: &ShellState,
    output: &ShellOutputInstance,
    key: &Key,
) -> Option<super::app_context::Target> {
    let Key::App(id) = key else {
        return None;
    };
    let app = state.catalog.app_by_id(id)?;
    Some(super::app_context::Target {
        app_id: app.id.clone(),
        output_name: output.output_name.clone(),
        source_app_id: hearthspace_common::frame_layout::frame_widget_app_id(
            output.output_name.as_deref(),
            "apps",
        ),
    })
}

fn activate(state: &mut ShellState, output_key: &str, key: &Key) {
    let app = apps(state).into_iter().find(|app| &app.key == key);
    let result = if let Some(app) = app {
        if let Some(window) = app.next_window() {
            let id = window.id;
            send_command(&state.command_socket, &format!("focus-window {id}")).map(|()| {
                // Reflect an acknowledged focus immediately so repeated clicks can
                // cycle before the next half-second compositor catalog refresh.
                for window in &mut state.windows {
                    window.focused = window.id == id;
                }
                state.dock.last_focused_window = Some(id);
            })
        } else if let Key::App(id) = app.key {
            let output_name = state
                .outputs
                .iter()
                .find(|output| output.key() == output_key)
                .and_then(|output| output.output_name.clone());
            super::launch::app(state, output_key, &id, output_name.as_deref(), true);
            return;
        } else {
            Err("This app has closed".into())
        }
    } else {
        Err("This app has closed".into())
    };
    state.dismiss_panels();
    if let Err(error) = result
        && let Some(output) = state.outputs.iter_mut().find(|o| o.key() == output_key)
    {
        output.show_search = true;
        output.launch_error = Some(format!(
            "Could not open or switch to this app: {error}. Choose an app from Apps List to reopen it."
        ));
    }
}

pub(super) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: components::Palette,
) {
    properties.insert::<icons::IconWidget, _>(palette);
}
