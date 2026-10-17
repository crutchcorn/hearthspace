//! Four output-local surfaces keep awareness above and actions below the canvas.
use hearthspace_common::{CONTROL_BAR_HEIGHT, ShellCorner, shell_corner_app_id};
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{FlexExt, flex_row, label, resize_observer, sized_box},
};

use super::{
    icons::{Icon, icon_button},
    state::{ShellOutputInstance, ShellState},
};

const PADDING: f64 = 6.0;
const FRAME: f64 = 2.0 * (PADDING + 1.0);
const APPS_BUTTON: f64 = 38.0;

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
            system_width(state),
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
            (FRAME + APPS_BUTTON + super::dock::desired_width(state)).max(48.0),
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
            .background_color(components::SURFACE)
            .border(components::BORDER, 1.0.px())
            .corner_radius(10.0.px()),
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
        super::notification_view::button(output, &state.notifications),
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
            super::canvas_map::map_button(output),
            super::window_group::button(&state.window_group),
        ))
        .gap(6.0.px()),
    )
}

fn system_width(state: &ShellState) -> f64 {
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
    let compact = output.corner_widths[1] < 300.0;
    let reserved = if state.battery.is_some() && !compact {
        286.0
    } else {
        242.0
    };
    frame(
        output,
        ShellCorner::System,
        "System tray",
        flex_row((
            label(
                state
                    .tray
                    .hovered
                    .as_ref()
                    .map(|(_, title)| title.clone())
                    .or_else(|| state.window_group.hint())
                    .unwrap_or_else(|| state.clock.clone()),
            )
            .text_size(13.0)
            .text_alignment(xilem::TextAlign::End)
            .color(components::TEXT_MUTED)
            .line_break_mode(LineBreaking::Clip)
            .flex(1.0),
            super::tray_view::strip(output, &state.tray, reserved),
            super::battery_view::indicator(state.battery.as_ref(), compact),
            super::control_center::toolbar_button(output),
            super::session_controls::power_button(output),
        ))
        .gap(6.0.px()),
    )
}

pub(super) fn apps_view(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    frame(
        output,
        ShellCorner::Apps,
        "Running apps and Apps List",
        flex_row((
            super::dock::view(
                state,
                output,
                (output.corner_widths[3] - FRAME - APPS_BUTTON).max(0.0),
            ),
            icon_button(Icon::Apps, "Apps List", output.show_search, move |state| {
                state.toggle_search(&key);
            }),
        ))
        .gap(4.0.px()),
    )
}

#[cfg(test)]
#[path = "corners_tests.rs"]
mod tests;
