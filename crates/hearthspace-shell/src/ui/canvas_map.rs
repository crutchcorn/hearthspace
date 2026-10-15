//! Right-hand canvas navigation popover, rendered as ordinary shell UI.
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowOptionsExtLinux, WindowView,
    dpi::LogicalSize,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, FlexSpacer, flex_col, flex_row, sized_box},
    window,
};

use super::{
    command_client::send_command,
    icons::{Icon, icon_button},
    minimap::{CanvasSnapshot, minimap_view},
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
    let size = LogicalSize::new(340.0, 336.0);
    window(
        output.map_window_id,
        "Hearthspace Canvas Map",
        panel(output, snapshot, error),
    )
    .with_options(move |options| {
        options
            .with_name(&app_id, &app_id)
            .with_decorations(false)
            .with_resizable(false)
            .with_initial_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .on_close(move |state| state.dismiss_map(&key))
    })
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
    let hint = if error.is_some() {
        "Map unavailable · retrying…"
    } else {
        "Drag the blue area to move your view"
    };
    sized_box(components::scroll_area(
        flex_col((
            flex_row((
                components::section_title("Canvas map"),
                FlexSpacer::Flex(1.0),
                close,
            )),
            minimap_view(snapshot, output.output_name.as_deref()),
            components::muted_text(hint),
            footer,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px()),
    ))
    .padding(12.0.px())
    .background_color(components::SURFACE)
    .border(components::BORDER, 1.0.px())
    .corner_radius(10.0.px())
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
