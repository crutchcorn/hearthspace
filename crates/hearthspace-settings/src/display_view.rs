use hearthspace_ipc::MonitorConfig;
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, flex_col, flex_row, label, sized_box},
};

use crate::{
    actions::save_monitor_scale,
    monitor_layout_view::monitor_layout_editor,
    monitors::{MONITOR_SCALE_OPTIONS, monitor_logical_height, monitor_logical_width},
    page_components::{card, note, section},
    state::SettingsApp,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    flex_col((
        section(
            "Monitor layout",
            flex_col((
                note("Drag monitors to arrange their relative positions."),
                card(sized_box(monitor_layout_editor()).fixed_height(220.0.px())),
                note(format_monitor_summary(&state.settings.monitors)),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(10.0.px()),
        ),
        monitor_details_view(state),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(24.0.px())
    .boxed()
}

fn format_monitor_summary(monitors: &[MonitorConfig]) -> String {
    if monitors.is_empty() {
        return "No monitors are currently available.".to_string();
    }
    monitors
        .iter()
        .map(|monitor| {
            format!(
                "{}: {}×{} at {}× scale",
                monitor.name, monitor.width, monitor.height, monitor.scale
            )
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn monitor_details_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let Some(monitor) = state
        .settings
        .monitors
        .iter()
        .find(|monitor| state.selected_monitor_id.as_ref() == Some(&monitor.id))
    else {
        return note("Select a monitor to edit its settings.").boxed();
    };
    let selected_scale = monitor.scale;
    let scale_buttons = MONITOR_SCALE_OPTIONS.map(|scale| {
        let id = monitor.id.clone();
        components::segmented_option(
            format!("{scale}x"),
            selected_scale == scale,
            move |state: &mut SettingsApp| save_monitor_scale(state, &id, scale),
        )
    });
    section(
        monitor.name.clone(),
        card(
            flex_col((
                note(format!(
                    "Physical: {}×{} · Desktop: {}×{} · Position: {},{}",
                    monitor.width,
                    monitor.height,
                    monitor_logical_width(monitor),
                    monitor_logical_height(monitor),
                    monitor.x,
                    monitor.y
                )),
                label(format!("Scale: {}x", monitor.scale)).text_size(13.0),
                flex_row(scale_buttons).gap(6.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(12.0.px()),
        ),
    )
    .boxed()
}
