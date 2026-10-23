//! Display arrangement and the settings supported by the native monitor backend.
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, parley::style::FontWeight, properties::Padding},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, button, flex_col, flex_row, sized_box},
};

use crate::{
    actions::save_monitor_scale,
    monitor_layout_view::monitor_layout_editor,
    monitors::MONITOR_SCALE_OPTIONS,
    page_components::{note, select},
    state::SettingsApp,
    typography::label,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    flex_col((
        flex_row((
            FlexSpacer::Flex(1.0),
            sized_box(monitor_layout_editor()).width(xilem::masonry::layout::Dim::FitContent),
            FlexSpacer::Flex(1.0),
        ))
        .gap(0.0.px()),
        monitor_details_view(state),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(22.0.px())
    .boxed()
}

fn row<V: WidgetView<SettingsApp>>(name: &'static str, value: V) -> impl WidgetView<SettingsApp> {
    sized_box(flex_row((label(name).text_size(11.0).flex(1.0), value)).gap(12.0.px()))
        .height(56.0.px())
        .padding(Padding::from_vh(8.0.px(), 12.0.px()))
}

fn divider(palette: components::Palette) -> impl WidgetView<SettingsApp> {
    sized_box(label(""))
        .fixed_height(1.0.px())
        .background_color(palette.border)
}

fn monitor_details_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let Some((index, monitor)) = state
        .settings
        .monitors
        .iter()
        .enumerate()
        .find(|(_, monitor)| state.selected_monitor_id.as_ref() == Some(&monitor.id))
        .or_else(|| state.settings.monitors.iter().enumerate().next())
    else {
        return note(palette, "No monitors are currently available.").boxed();
    };
    let live_output = state
        .live_outputs
        .iter()
        .find(|output| output.id == monitor.id || output.name == monitor.name);
    let connector = live_output.map_or(monitor.id.as_str(), |output| output.id.as_str());
    let built_in = connector.starts_with("eDP")
        || connector.starts_with("LVDS")
        || monitor.name.to_lowercase().contains("built-in");
    let name = display_name(&monitor.name, connector, built_in);
    let monitor_id = monitor.id.clone();
    let selected_scale = MONITOR_SCALE_OPTIONS
        .iter()
        .position(|scale| *scale == monitor.scale)
        .unwrap_or(0);
    let scale = select(
        palette,
        "Scale",
        MONITOR_SCALE_OPTIONS
            .map(|scale| format!("{}%", scale * 100))
            .to_vec(),
        selected_scale,
        false,
        move |state, index| save_monitor_scale(state, &monitor_id, MONITOR_SCALE_OPTIONS[index]),
    )
    .height(36.0.px());
    let refresh = live_output
        .and_then(|output| output.refresh_millihertz)
        .map_or_else(
            || "Not reported".to_owned(),
            |refresh| {
                if refresh % 1000 == 0 {
                    format!("{} Hz", refresh / 1000)
                } else {
                    format!("{:.2} Hz", f64::from(refresh) / 1000.0)
                }
            },
        );
    flex_col((
        flex_row((
            components::settings_surface(
                sized_box(
                    icon(
                        if built_in {
                            Icon::Laptop
                        } else {
                            Icon::Monitor
                        },
                        21.0,
                    )
                    .color(palette.muted),
                )
                .fixed_width(38.0.px())
                .fixed_height(38.0.px())
                .padding(8.5.px())
                .corner_radius(components::RADIUS_CONTROL.px()),
                components::SettingsSurface::Control,
            ),
            flex_col((
                label(name).text_size(13.0).weight(FontWeight::MEDIUM),
                label(format!("Display {} · {connector}", index + 1))
                    .text_size(10.0)
                    .color(palette.muted),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(4.0.px()),
        ))
        .gap(10.0.px()),
        components::settings_surface(
            sized_box(
                flex_col((
                    row(
                        "Resolution",
                        read_only_field(palette, format!("{} × {}", monitor.width, monitor.height)),
                    ),
                    divider(palette),
                    row("Scale", scale),
                    divider(palette),
                    row("Refresh rate", read_only_field(palette, refresh)),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(0.0.px()),
            )
            .corner_radius(components::RADIUS_TILE.px()),
            components::SettingsSurface::Subtle,
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(14.0.px())
    .boxed()
}

fn read_only_field(palette: components::Palette, value: String) -> impl WidgetView<SettingsApp> {
    components::settings_button(
        button(
            flex_row((
                label(value).text_size(11.0),
                icon(Icon::ChevronDown, 12.0).color(palette.muted),
            ))
            .gap(8.0.px()),
            |_: &mut SettingsApp| {},
        )
        .disabled(true)
        .height(36.0.px())
        .padding(Padding::from_vh(6.0.px(), 8.0.px())),
        components::SettingsButton::Control,
        false,
    )
}

fn display_name(description: &str, connector: &str, built_in: bool) -> String {
    if built_in {
        return "Built-in display".into();
    }
    // Smithay descriptions combine make, model and connector. Our native
    // backend uses "DRM - DP-1 - DP-1"; keep real product names when available,
    // and show the connector once in the smaller identity line below.
    let description = description
        .trim()
        .strip_suffix(&format!(" ({connector})"))
        .unwrap_or(description.trim());
    let name = description
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != connector && *part != "DRM")
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        "External display".into()
    } else {
        name
    }
}

#[cfg(test)]
mod tests;
