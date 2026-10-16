use hearthspace_common::{CONTROL_BAR_HEIGHT, SHELL_OUTPUT_WATCHER_APP_ID};
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowOptionsExtLinux, WindowView,
    core::fork,
    dpi::LogicalSize,
    masonry::layout::AsUnit,
    style::Style,
    view::{FlexExt, flex_row, label, resize_observer, sized_box},
    window,
};

use super::{
    canvas_map::{map_button, map_window},
    canvas_task::canvas_task,
    constants::{SHELL_BAR_H_PAD, launcher_size},
    icons::{Icon, icon_button},
    launcher::{self, Entry},
    notification_view,
    notifications::{Notification, notification_task},
    output_watcher::output_watcher_task,
    popup::{self, PopupKind},
    session_controls::{clock_task, power_button, power_window},
    shell_events::shell_events_task,
    state::{ShellOutputInstance, ShellState},
    window_catalog::window_catalog_task,
};

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
    super::tray_state::reconcile(state);
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

    for output in &state.outputs {
        windows.push(shell_bar_window(
            output,
            state.clock.clone(),
            &state.notifications,
            &state.tray,
            state.battery.as_ref(),
            &state.window_group,
        ));
        if let Some(osd) = &output.osd {
            windows.push(super::control_osd::window(output, osd));
        }
        if output.show_controls {
            windows.push(super::control_center::window(output, &state.controls));
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
            windows.push(power_window(output, &state.power, state.battery.as_ref()));
            continue;
        }
        let results = launcher::results(state, output);
        if output.show_search {
            windows.push(launcher_window(
                state,
                output,
                results,
                state.windows_error.clone(),
            ));
        }
    }

    windows.into_iter()
}

fn shell_bar_window(
    output: &ShellOutputInstance,
    clock: String,
    notifications: &[Notification],
    tray: &super::tray_state::TrayState,
    battery: Option<&crate::battery::BatteryStatus>,
    group: &super::window_group::GroupState,
) -> WindowView<ShellState> {
    let bar_app_id = output.bar_app_id();
    window(
        output.bar_window_id,
        "Hearthspace Shell",
        bar_view(output, clock, notifications, tray, battery, group),
    )
    .with_options(move |options| {
        options
            .with_name(&bar_app_id, &bar_app_id)
            .with_decorations(false)
            .with_resizable(false)
            .with_initial_inner_size(LogicalSize::new(760.0, f64::from(CONTROL_BAR_HEIGHT)))
            .on_close(ShellState::stop)
    })
}

fn notification_window(
    output: &ShellOutputInstance,
    notifications: Vec<Notification>,
    error: Option<String>,
) -> WindowView<ShellState> {
    let app_id = output.notifications_app_id();
    let key = output.key().to_string();
    let size = popup::size(output, PopupKind::Notifications, 360.0, 100.0);
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
    let launcher_app_id = output.launcher_app_id();
    let mut size = launcher_size(results.len().max(1));
    let key = output.key().to_string();
    if output.launch_error.is_some() || windows_error.is_some() {
        size.height += 100.0;
    }
    let size = popup::size(output, PopupKind::Search, size.width, size.height);
    popup::window(
        output.launcher_window_id,
        "Hearthspace Launcher",
        launcher_app_id,
        size,
        launcher::launcher_view(state, output, results, windows_error),
        move |state| state.dismiss_search(&key),
    )
}

pub(super) fn bar_view(
    output: &ShellOutputInstance,
    clock: String,
    notifications: &[Notification],
    tray: &super::tray_state::TrayState,
    battery: Option<&crate::battery::BatteryStatus>,
    group: &super::window_group::GroupState,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let search = icon_button(Icon::Search, "Search", output.show_search, move |state| {
        state.toggle_search(&key);
    });
    let width_key = output.key().to_string();
    // On very small outputs retain the battery's accessible percentage while
    // making room for both the app tray and all desktop controls.
    let compact_battery = output.tray.bar_width <= 360.0
        && super::tray_state::visible(&tray.snapshot).next().is_some();
    resize_observer(
        move |state: &mut ShellState, size| {
            if size.width.is_finite()
                && let Some(output) = state.outputs.iter_mut().find(|o| o.key() == width_key)
            {
                output.tray.bar_width = size.width;
            }
        },
        sized_box(
            flex_row((
                label(
                    tray.hovered
                        .as_ref()
                        .map(|(_, title)| title.clone())
                        .or_else(|| group.hint())
                        .unwrap_or(clock),
                )
                .text_size(13.0)
                .text_alignment(xilem::TextAlign::End)
                .color(components::TEXT_MUTED)
                .line_break_mode(xilem::masonry::properties::LineBreaking::Clip)
                .flex(1.0),
                super::tray_view::strip(output, tray, if battery.is_some() { 148.0 } else { 84.0 }),
                super::battery_view::indicator(battery, compact_battery),
                super::window_group::button(group),
                search,
                notification_view::button(output, notifications),
                map_button(output),
                super::control_center::toolbar_button(output),
                power_button(output),
            ))
            .gap((if compact_battery { 5.0 } else { 6.0 }).px()),
        )
        .background_color(components::SURFACE)
        .padding(xilem::masonry::properties::Padding::from_vh(
            7.0.px(),
            SHELL_BAR_H_PAD.px(),
        )),
    )
}

fn output_watcher_view(map_open: bool, osd_open: bool) -> impl WidgetView<ShellState> + use<> {
    sized_box(fork(
        fork(
            fork(label(""), output_watcher_task()),
            fork(
                fork(window_catalog_task(), shell_events_task()),
                super::window_group::task(),
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
