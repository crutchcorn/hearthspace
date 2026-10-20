//! Display arrangement and the settings supported by the native monitor backend.
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, parley::style::FontWeight, properties::Padding},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, flex_col, flex_row, label, sized_box},
};

use crate::{
    actions::save_monitor_scale,
    monitor_layout_view::monitor_layout_editor,
    monitors::{MONITOR_SCALE_OPTIONS, monitor_logical_height, monitor_logical_width},
    page_components::note,
    state::SettingsApp,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    flex_col((
        flex_row((
            FlexSpacer::Flex(1.0),
            sized_box(monitor_layout_editor())
                .fixed_height(225.0.px())
                .width(xilem::masonry::layout::Dim::FitContent),
            FlexSpacer::Flex(1.0),
        )),
        monitor_details_view(state),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(22.0.px())
    .boxed()
}

fn row<V: WidgetView<SettingsApp>>(name: &'static str, value: V) -> impl WidgetView<SettingsApp> {
    sized_box(flex_row((label(name).text_size(12.0).flex(1.0), value)).gap(12.0.px()))
        .padding(Padding::from_vh(12.0.px(), 12.0.px()))
}

fn divider() -> impl WidgetView<SettingsApp> {
    sized_box(label(""))
        .fixed_height(1.0.px())
        .background_color(components::CHROME_BORDER)
}

fn monitor_details_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let Some((index, monitor)) = state
        .settings
        .monitors
        .iter()
        .enumerate()
        .find(|(_, monitor)| state.selected_monitor_id.as_ref() == Some(&monitor.id))
        .or_else(|| state.settings.monitors.iter().enumerate().next())
    else {
        return note("No monitors are currently available.").boxed();
    };
    let connector = state
        .live_outputs
        .iter()
        .find(|output| output.id == monitor.id || output.name == monitor.name)
        .map_or(monitor.id.as_str(), |output| output.id.as_str());
    let built_in = connector.starts_with("eDP")
        || connector.starts_with("LVDS")
        || monitor.name.to_lowercase().contains("built-in");
    let scale_buttons = MONITOR_SCALE_OPTIONS.map(|scale| {
        let id = monitor.id.clone();
        components::segmented_option(
            format!("{}%", scale * 100),
            monitor.scale == scale,
            move |state: &mut SettingsApp| save_monitor_scale(state, &id, scale),
        )
    });
    flex_col((
        flex_row((
            sized_box(
                icon(
                    if built_in {
                        Icon::Laptop
                    } else {
                        Icon::Monitor
                    },
                    21.0,
                )
                .color(components::TEXT_MUTED),
            )
            .padding(8.0.px())
            .background_color(components::SURFACE_CONTROL)
            .border(components::CHROME_BORDER, 1.0.px())
            .corner_radius(components::RADIUS_CONTROL.px()),
            flex_col((
                label(monitor.name.clone())
                    .text_size(13.0)
                    .weight(FontWeight::MEDIUM),
                label(format!("Display {} · {connector}", index + 1))
                    .text_size(10.0)
                    .color(components::TEXT_MUTED),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(4.0.px()),
        ))
        .gap(10.0.px()),
        sized_box(
            flex_col((
                row(
                    "Resolution",
                    label(format!("{} × {}", monitor.width, monitor.height)).text_size(12.0),
                ),
                divider(),
                row("Scale", flex_row(scale_buttons).gap(4.0.px())),
                divider(),
                row(
                    "Desktop size",
                    label(format!(
                        "{} × {}",
                        monitor_logical_width(monitor),
                        monitor_logical_height(monitor)
                    ))
                    .text_size(12.0),
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(0.0.px()),
        )
        .background_color(components::SURFACE_SUBTLE)
        .border(components::CHROME_BORDER, 1.0.px())
        .corner_radius(components::RADIUS_TILE.px()),
        note(format!("Position: {}, {}", monitor.x, monitor.y)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(14.0.px())
    .boxed()
}
