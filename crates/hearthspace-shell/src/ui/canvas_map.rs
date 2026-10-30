//! The Overview popover, rendered as ordinary shell UI.
use components::icons::{Icon as Glyph, icon};
use hearthspace_ipc::command::{ShellCommand, ZoomOperation};
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowView,
    masonry::{
        layout::AsUnit,
        parley::style::{FontWeight, LineHeight},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexSpacer, button, flex_col, flex_row, label},
};

use super::{
    appearance::{ThemeStyle as _, Tone},
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
    let size = popup::size(output, PopupKind::Map, 352.0, 312.0);
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
    let zoom = snapshot
        .map(|snapshot| format!("{:.0}%", snapshot.scale() * 100.0))
        .unwrap_or_else(|| "—".into());
    let reset_disabled = snapshot.is_none_or(|snapshot| (snapshot.scale() - 1.0).abs() <= 1e-6);
    let header = flex_row((
        super::popup_group::group(
            "Overview",
            label("Overview")
                .font("Liberation Sans")
                .text_size(24.0)
                .weight(FontWeight::new(450.0))
                .letter_spacing(-0.7)
                .line_height(LineHeight::Absolute(28.0))
                .theme_color(Tone::Text),
        )
        .heading(2),
        FlexSpacer::Flex(1.0),
        super::popup_group::group(
            "Current canvas zoom",
            label(zoom)
                .font("Liberation Sans")
                .text_size(14.0)
                .line_height(LineHeight::Absolute(16.0))
                .theme_color(Tone::Muted),
        )
        .status(),
    ))
    .gap(15.0.px());
    let footer = flex_row((
        FlexSpacer::Flex(1.0),
        zoom_button(
            Glyph::RotateCcw,
            "Reset canvas zoom to 100%",
            output.output_name.as_deref(),
            ZoomOperation::To(1.0),
            reset_disabled,
        ),
        zoom_button(
            Glyph::ZoomOut,
            "Zoom out",
            output.output_name.as_deref(),
            ZoomOperation::Out,
            false,
        ),
        zoom_button(
            Glyph::ZoomIn,
            "Zoom in",
            output.output_name.as_deref(),
            ZoomOperation::In,
            false,
        ),
    ))
    .gap(2.0.px());
    let unavailable = error.map(|_| components::muted_text("Map unavailable · retrying…"));
    popup::panel(
        output,
        PopupKind::Map,
        flex_col((
            header,
            flex_col((
                minimap_view(snapshot, output.output_name.as_deref()),
                unavailable,
                footer,
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(18.0.px()),
    )
}

fn zoom_button(
    glyph: Glyph,
    name: &'static str,
    output_name: Option<&str>,
    operation: ZoomOperation,
    disabled: bool,
) -> impl WidgetView<ShellState> + use<> {
    let command = output_name.map(|output_name| {
        ShellCommand::ZoomOnOutput {
            output_name: output_name.to_owned(),
            operation,
        }
        .wire_name()
    });
    let disabled = disabled || command.is_none();
    components::button_surface(
        button(
            icon(glyph, 18.0).label(name).theme_color(if disabled {
                Tone::Disabled
            } else {
                Tone::Text
            }),
            move |state: &mut ShellState| {
                if let Some(command) = &command {
                    state.canvas_error = send_command(&state.command_socket, command).err();
                }
            },
        )
        .disabled(disabled)
        .padding(12.0.px())
        .corner_radius(10.0.px()),
        components::ButtonSurface::Bare,
        false,
    )
}
