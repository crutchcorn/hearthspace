//! Four output-local surfaces keep awareness above and actions below the canvas.
use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_common::{
    CONTROL_BAR_HEIGHT, NOTIFICATION_CORNER_WIDTH, SHELL_CORNER_RADIUS, ShellCorner,
    shell_corner_app_id,
};
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::layout::AsUnit,
    style::Style,
    view::{MainAxisAlignment, flex_row, resize_observer, sized_box},
};

use super::{
    corner_button::focus,
    icons::{Icon, icon_button},
    state::{ShellOutputInstance, ShellState},
};

const PADDING: f64 = 4.0;
const FRAME: f64 = 2.0 * (PADDING + 1.0);
const APPS_BUTTON: f64 = 48.0;

pub(super) fn windows(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> Vec<WindowView<ShellState>> {
    vec![
        corner_window(
            output,
            ShellCorner::Notifications,
            f64::from(NOTIFICATION_CORNER_WIDTH),
            notifications_view(state, output),
        ),
        corner_window(
            output,
            ShellCorner::System,
            system_width(state, output),
            system_view(state, output),
        ),
        corner_window(
            output,
            ShellCorner::Canvas,
            150.0,
            canvas_view(state, output),
        ),
        corner_window(
            output,
            ShellCorner::Apps,
            apps_width(state),
            apps_view(state, output),
        ),
    ]
}

fn index(corner: ShellCorner) -> usize {
    match corner {
        ShellCorner::Notifications => 0,
        ShellCorner::System => 1,
        ShellCorner::Canvas => 2,
        ShellCorner::Apps => 3,
    }
}

fn corner_window<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    corner: ShellCorner,
    width: f64,
    view: V,
) -> WindowView<ShellState> {
    let title = match corner {
        ShellCorner::Notifications => "Hearthspace Notifications Corner",
        ShellCorner::System => "Hearthspace System Tray",
        ShellCorner::Canvas => "Hearthspace Canvas Tools",
        ShellCorner::Apps => "Hearthspace App Dock",
    };
    super::popup::window(
        output.corner_window_ids[index(corner)],
        title,
        shell_corner_app_id(corner, output.output_name.as_deref()),
        LogicalSize::new(width, f64::from(CONTROL_BAR_HEIGHT)),
        view,
        ShellState::stop,
    )
}

fn frame<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    corner: ShellCorner,
    name: &'static str,
    view: V,
) -> impl WidgetView<ShellState> + use<V> {
    let key = output.key().to_string();
    resize_observer(
        move |state: &mut ShellState, size| {
            if size.width.is_finite()
                && let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key)
            {
                output.corner_widths[index(corner)] = size.width;
                if corner == ShellCorner::System {
                    output.tray.bar_width = size.width;
                }
            }
        },
        sized_box(super::popup_group::group(name, view).reset_focus_on_deactivation())
            .padding(PADDING.px())
            .theme_background(Tone::Chrome)
            .theme_border(Tone::ChromeBorder)
            .border_width(1.0.px())
            .corner_radius(SHELL_CORNER_RADIUS.px()),
    )
}

pub(super) fn notifications_view(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    frame(
        output,
        ShellCorner::Notifications,
        "Notifications",
        flex_row((focus(super::notification_view::button(
            output,
            &state.notifications,
        )),))
        .main_axis_alignment(MainAxisAlignment::Center),
    )
}

pub(super) fn canvas_view(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    frame(
        output,
        ShellCorner::Canvas,
        "Canvas tools",
        flex_row((
            focus(super::desktop::selection_button(state)),
            focus(super::window_group::button(&state.window_group)),
            focus(super::canvas_map::map_button(output)),
        ))
        .main_axis_alignment(MainAxisAlignment::Center)
        .gap(4.0.px()),
    )
}

fn system_width(state: &ShellState, _output: &ShellOutputInstance) -> f64 {
    let items = super::tray_state::visible(&state.tray.snapshot).count();
    290.0
        + if state.battery.is_some() { 48.0 } else { 0.0 }
        + if items > 0 {
            38.0 * (items.min(8) + 1) as f64
        } else {
            0.0
        }
}

pub(super) fn system_view(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let items = super::tray_state::visible(&state.tray.snapshot)
        .count()
        .min(8);
    let full_base = 290.0
        + if state.battery.is_some() { 48.0 } else { 0.0 }
        + if items > 0 { 38.0 } else { 0.0 };
    let compact = output.corner_widths[1] < full_base;
    // Preserve Sound, Power, and the tray drawer on very small scaled outputs.
    let gap = if compact && output.corner_widths[1] < 140.0 {
        0.0
    } else {
        4.0
    };
    let compact_base = FRAME + 88.0 + gap + if items > 0 { 34.0 + gap } else { 0.0 };
    let show_brightness = !compact || output.corner_widths[1] >= compact_base + 44.0 + gap;
    let compact_controls = compact_base + if show_brightness { 44.0 + gap } else { 0.0 };
    let show_battery = !compact || output.corner_widths[1] >= compact_controls + 44.0 + gap;
    let reserved = if compact {
        compact_controls
            + if show_battery && state.battery.is_some() {
                44.0 + gap
            } else {
                0.0
            }
    } else {
        full_base
    };
    frame(
        output,
        ShellCorner::System,
        "System tray",
        flex_row((
            (!compact).then(|| focus(super::quick_panel::clock_button(state, output))),
            super::tray_view::strip(output, &state.tray, reserved),
            (!compact).then(|| {
                focus(super::quick_panel::button_view(
                    output,
                    super::quick_panel::Kind::Wifi,
                    Icon::Wifi,
                    "Wi-Fi settings",
                ))
            }),
            show_brightness.then(|| {
                focus(super::quick_panel::button_view(
                    output,
                    super::quick_panel::Kind::Brightness,
                    Icon::Brightness,
                    "Brightness settings",
                ))
            }),
            focus(super::quick_panel::button_view(
                output,
                super::quick_panel::Kind::Sound,
                Icon::Volume {
                    muted: state
                        .controls
                        .snapshot
                        .audio
                        .as_ref()
                        .is_ok_and(|audio| audio.muted),
                },
                "Sound settings",
            )),
            state
                .battery
                .as_ref()
                .filter(|_| show_battery)
                .map(|battery| {
                    focus(super::quick_panel::button_view(
                        output,
                        super::quick_panel::Kind::Battery,
                        Icon::Battery {
                            percentage: battery.percentage,
                            charging: battery.is_charging(),
                        },
                        &battery.accessible_label(),
                    ))
                }),
            focus(super::session_controls::power_button(output)),
        ))
        .main_axis_alignment(MainAxisAlignment::Center)
        .gap(gap.px()),
    )
}

pub(super) fn apps_view(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let has_apps = !state.windows.is_empty();
    frame(
        output,
        ShellCorner::Apps,
        "Running apps and Apps List",
        flex_row((
            has_apps.then(|| {
                super::dock::view(
                    state,
                    output,
                    (output.corner_widths[3] - FRAME - APPS_BUTTON).max(0.0),
                )
            }),
            focus(icon_button(
                Icon::Apps,
                "Apps List",
                output.show_search,
                move |state| {
                    state.toggle_search(&key);
                },
            )),
        ))
        // Keep the screen-edge action stationary while the catalog changes
        // before a matching Wayland surface resize has arrived.
        .main_axis_alignment(MainAxisAlignment::End)
        .gap(4.0.px()),
    )
}

fn apps_width(state: &ShellState) -> f64 {
    if state.windows.is_empty() {
        54.0
    } else {
        FRAME + APPS_BUTTON + super::dock::desired_width(state)
    }
}

#[cfg(test)]
#[path = "corners_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "corners_battery_tests.rs"]
mod battery_tests;

#[cfg(test)]
#[path = "corners_dock_tests.rs"]
mod dock_tests;
