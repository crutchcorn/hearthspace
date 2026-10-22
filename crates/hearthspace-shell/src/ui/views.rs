use hearthspace_common::SHELL_OUTPUT_WATCHER_APP_ID;
use xilem::{
    WidgetView, WindowOptionsExtLinux, WindowView,
    core::fork,
    dpi::LogicalSize,
    masonry::layout::AsUnit,
    view::{label, sized_box},
    window,
};

use super::{
    canvas_map::map_window,
    canvas_task::canvas_task,
    constants::launcher_size,
    launcher::{self, Entry},
    notification_view,
    notifications::{Notification, notification_task},
    output_watcher::output_watcher_task,
    popup::{self, PopupKind},
    session_controls::{clock_task, power_window},
    shell_events::shell_events_task,
    state::{ShellOutputInstance, ShellState},
    window_catalog::window_catalog_task,
};

#[cfg(test)]
#[path = "favorite_catalog_layout_tests.rs"]
mod favorite_catalog_layout_tests;
#[cfg(test)]
#[path = "views_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "toolbar_tests.rs"]
mod toolbar_tests;

pub(super) fn app_logic(
    state: &mut ShellState,
) -> impl Iterator<Item = WindowView<ShellState>> + use<> {
    super::search_selection::reconcile(state);
    super::dock::reconcile(state);
    super::tray_state::reconcile(state);
    super::frame_layout::reconcile(state);
    let watcher = window(
        state.watcher_window_id,
        "Hearthspace Output Watcher",
        output_watcher_view(
            state.outputs.iter().any(|output| output.show_map),
            state.outputs.iter().any(|output| output.osd.is_some()),
        ),
    )
    .with_options(|options| {
        options
            .with_name(SHELL_OUTPUT_WATCHER_APP_ID, SHELL_OUTPUT_WATCHER_APP_ID)
            .with_decorations(false)
            .with_resizable(false)
            .with_initial_inner_size(LogicalSize::new(1.0, 1.0))
            .on_close(ShellState::stop)
    });
    let mut windows = vec![watcher];
    windows.extend(super::desktop::windows(state));
    windows.extend(super::canvas_search::windows(state));
    windows.extend(super::window_switcher::window(state));
    windows.extend(super::frame_layout::windows(state));

    for output in &state.outputs {
        if let Some(osd) = &output.osd {
            windows.push(super::control_osd::window(output, osd));
        }
        if output.show_controls {
            windows.push(super::quick_panel::window(state, output));
            continue;
        }
        if output.tray.open {
            windows.push(super::tray_view::window(output, &state.tray));
            continue;
        }
        if output.show_map {
            windows.push(map_window(
                output,
                state.canvas.as_ref(),
                state.canvas_error.as_deref(),
            ));
            continue;
        }
        if output.show_notifications {
            windows.push(notification_window(
                output,
                state.notifications.clone(),
                state.notifications_error.clone(),
            ));
            continue;
        }

        if output.show_power {
            windows.push(power_window(output, &state.power));
            continue;
        }
        let results = launcher::results(state, output);
        if output.show_search {
            windows.push(launcher_window(
                state,
                output,
                results,
                state
                    .windows_error
                    .clone()
                    .or_else(|| state.desktop.source_error.clone()),
            ));
        }
    }

    let properties = state.appearance.properties.clone();
    windows
        .into_iter()
        .map(move |window| window.with_default_properties(properties.clone()))
}

fn notification_window(
    output: &ShellOutputInstance,
    notifications: Vec<Notification>,
    error: Option<String>,
) -> WindowView<ShellState> {
    let app_id = output.notifications_app_id();
    let key = output.key().to_string();
    let size = popup::size(output, PopupKind::Notifications, 390.0, 280.0);
    popup::window(
        output.notifications_window_id,
        "Hearthspace Notifications",
        app_id,
        size,
        notification_view::panel(output, notifications, error),
        move |state| state.dismiss_notifications(&key),
    )
}

fn launcher_window(
    state: &ShellState,
    output: &ShellOutputInstance,
    results: Vec<Entry>,
    windows_error: Option<String>,
) -> WindowView<ShellState> {
    // Background catalog errors must not move the pad or resize its popup,
    // which would invalidate an in-flight drawing capture.
    let windows_error = windows_error.filter(|_| !output.drawing.active());
    let launcher_app_id = output.launcher_app_id();
    let mut size = launcher_size(results.len().max(1));
    let key = output.key().to_string();
    if output.launch_error.is_some() || windows_error.is_some() {
        size.height += 100.0;
    }
    let size = popup::size(output, PopupKind::Search, size.width, size.height);
    popup::window(
        output.launcher_window_id,
        "Hearthspace Apps List",
        launcher_app_id,
        size,
        launcher::launcher_view(state, output, results, windows_error),
        move |state| state.dismiss_search(&key),
    )
}

fn output_watcher_view(map_open: bool, osd_open: bool) -> impl WidgetView<ShellState> + use<> {
    sized_box(fork(
        fork(
            fork(
                fork(label(""), output_watcher_task()),
                super::appearance::task(),
            ),
            fork(
                fork(
                    fork(window_catalog_task(), shell_events_task()),
                    super::dock::task(),
                ),
                fork(
                    super::window_group::task(),
                    fork(
                        super::desktop::task(),
                        fork(
                            super::canvas_search::task(),
                            fork(super::window_switcher::task(), super::frame_layout::task()),
                        ),
                    ),
                ),
            ),
        ),
        fork(
            fork(
                fork(
                    fork(clock_task(), notification_task()),
                    fork(super::battery_view::task(), super::power_task::task()),
                ),
                fork(
                    super::tray_task::task(),
                    fork(
                        super::control_task::task(),
                        osd_open.then(super::control_osd::task),
                    ),
                ),
            ),
            map_open.then(canvas_task),
        ),
    ))
    .fixed_width(1.0.px())
    .fixed_height(1.0.px())
}
