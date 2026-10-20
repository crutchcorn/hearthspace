//! Four output-local surfaces keep awareness above and actions below the canvas.
use hearthspace_common::{
    CONTROL_BAR_HEIGHT, SHELL_CORNER_RADIUS, ShellCorner, shell_corner_app_id,
};
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{FlexExt, MainAxisAlignment, flex_row, label, resize_observer, sized_box},
};

use super::{
    corner_button::focus,
    icons::{Icon, icon_button},
    state::{ShellOutputInstance, ShellState},
};

const PADDING: f64 = 6.0;
const FRAME: f64 = 2.0 * (PADDING + 1.0);
const APPS_BUTTON: f64 = 38.0;

#[path = "corner_measure.rs"]
mod measure;

pub(super) fn windows(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> Vec<WindowView<ShellState>> {
    vec![
        corner_window(
            output,
            ShellCorner::Notifications,
            48.0,
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
            88.0,
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
        sized_box(super::popup_group::group(name, view))
            .padding(PADDING.px())
            .background_color(components::SURFACE.with_alpha(0.72))
            .border(components::BORDER, 1.0.px())
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
            focus(super::canvas_map::map_button(output)),
            focus(super::window_group::button(&state.window_group)),
        ))
        .main_axis_alignment(MainAxisAlignment::Center)
        .gap(6.0.px()),
    )
}

fn system_width(state: &ShellState, output: &ShellOutputInstance) -> f64 {
    if let Some(width) = output.system_content_width {
        return width + FRAME;
    }
    let items = super::tray_state::visible(&state.tray.snapshot).count();
    250.0
        + if state.battery.is_some() { 64.0 } else { 0.0 }
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
    // A self-sized corner may be narrow simply because its clock is short.
    // Only collapse the percentage while its full preferred width is clipped
    // by the output (or is still waiting for the resize configure).
    let compact =
        state.battery.is_some() && output.corner_widths[1] + 0.5 < system_width(state, output);
    let items = super::tray_state::visible(&state.tray.snapshot)
        .count()
        .min(8);
    let reserved = system_width(state, output) - items as f64 * 38.0;
    let visible = super::tray_state::capacity(output.tray.bar_width - reserved).min(items);
    frame(
        output,
        ShellCorner::System,
        "System tray",
        measure::content(
            output,
            (items - visible) as f64 * 38.0,
            !compact && state.tray.hovered.is_none() && state.window_group.hint().is_none(),
            flex_row((
                label(
                    state
                        .tray
                        .hovered
                        .as_ref()
                        .map(|(_, title)| title.clone())
                        .or_else(|| state.window_group.hint())
                        .unwrap_or_else(|| state.clock.clone())
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                )
                .text_size(13.0)
                .text_alignment(xilem::TextAlign::End)
                .color(components::TEXT)
                .line_break_mode(LineBreaking::Clip)
                .flex(1.0),
                super::tray_view::strip(output, &state.tray, reserved),
                super::battery_view::indicator(state.battery.as_ref(), compact),
                focus(super::control_center::toolbar_button(output)),
                focus(super::session_controls::power_button(output)),
            ))
            .gap(6.0.px()),
        ),
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
        .main_axis_alignment(MainAxisAlignment::Center)
        .gap(4.0.px()),
    )
}

fn apps_width(state: &ShellState) -> f64 {
    if state.windows.is_empty() {
        48.0
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
