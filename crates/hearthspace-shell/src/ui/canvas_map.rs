//! Right-hand canvas navigation popover, rendered as ordinary shell UI.
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, FlexSpacer, flex_col, flex_row, sized_box},
};

use super::{
    command_client::send_command,
    icons::{Icon, icon_button},
    minimap::{CanvasSnapshot, minimap_view},
    popup::{self, PopupKind},
    state::{ShellOutputInstance, ShellState},
};

#[cfg(test)]
mod tests;

pub(super) fn map_button(output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    icon_button(Icon::Map, "Canvas map", output.show_map, move |state| {
        state.toggle_map(&key)
    })
}

pub(super) fn map_window(
    output: &ShellOutputInstance,
    snapshot: Option<&CanvasSnapshot>,
    error: Option<&str>,
) -> WindowView<ShellState> {
    let app_id = output.map_app_id();
    let key = output.key().to_string();
    let size = popup::size(output, PopupKind::Map, 340.0, 336.0);
    popup::window(
        output.map_window_id,
        "Hearthspace Canvas Map",
        app_id,
        size,
        panel(output, snapshot, error),
        move |state| state.dismiss_map(&key),
    )
}

fn panel(
    output: &ShellOutputInstance,
    snapshot: Option<&CanvasSnapshot>,
    error: Option<&str>,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let zoom = snapshot
        .map(|snapshot| format!("{:.0}%", snapshot.scale() * 100.0))
        .unwrap_or_else(|| "—".into());
    let close = icon_button(Icon::Close, "Close canvas map", false, move |state| {
        state.dismiss_map(&key)
    });
    let footer = flex_row((
        components::muted_text("Zoom"),
        FlexSpacer::Flex(1.0),
        zoom_button(Icon::ZoomOut, "Zoom out", "zoom-out"),
        sized_box(components::field_label(zoom)).fixed_width(48.0.px()),
        zoom_button(Icon::ZoomIn, "Zoom in", "zoom-in"),
    ))
    .gap(8.0.px());
    let unavailable = error.map(|_| components::muted_text("Map unavailable · retrying…"));
    popup::panel(
        output,
        PopupKind::Map,
        flex_col((
            flex_row((
                components::section_title("Canvas map"),
                FlexSpacer::Flex(1.0),
                close,
            )),
            minimap_view(snapshot, output.output_name.as_deref()),
            unavailable,
            footer,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px()),
    )
}

fn zoom_button(
    icon: Icon,
    name: &'static str,
    command: &'static str,
) -> impl WidgetView<ShellState> + use<> {
    icon_button(icon, name, false, move |state| {
        state.canvas_error = send_command(&state.command_socket, command).err();
    })
}
