//! Compact output-anchored hardware controls with explicit unavailable states.
use super::{
    control_slider::ControlSlider,
    control_state::{Action, Controls},
    icons::{Icon, glyph, icon_button},
    popup::{self, PopupKind},
    state::{ShellOutputInstance, ShellState},
};

#[cfg(test)]
#[path = "control_center_tests.rs"]
mod tests;
use hearthspace_common::{CONTROL_CENTER_APP_ID, control_center_app_id_for_output};
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, button, flex_col, flex_row, label, sized_box},
};

pub(super) fn toolbar_button(output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    icon_button(
        Icon::Controls,
        "Control center",
        output.show_controls,
        move |state| state.toggle_controls(&key),
    )
}

pub(super) fn window(output: &ShellOutputInstance, controls: &Controls) -> WindowView<ShellState> {
    let app_id = output
        .output_name
        .as_deref()
        .map(control_center_app_id_for_output)
        .unwrap_or_else(|| CONTROL_CENTER_APP_ID.into());
    let key = output.key().to_string();
    popup::window(
        output.controls_window_id,
        "Hearthspace Control Center",
        app_id,
        popup::size(output, PopupKind::Controls, 340.0, 240.0),
        panel(output, controls),
        move |state| state.dismiss_controls(&key),
    )
}

pub(super) fn panel(
    output: &ShellOutputInstance,
    controls: &Controls,
) -> impl WidgetView<ShellState> + use<> {
    let close_key = output.key().to_string();
    let volume = controls.volume();
    let brightness = controls.brightness();
    let audio = controls.snapshot.audio.as_ref().ok();
    let muted = audio.is_some_and(|audio| audio.muted);
    let volume_reason = controls
        .volume_error
        .clone()
        .or_else(|| controls.snapshot.audio.as_ref().err().cloned());
    let brightness_reason =
        controls
            .brightness_error
            .clone()
            .or_else(|| match &controls.snapshot.brightness {
                Ok(Some(_)) => None,
                Ok(None) => Some("This display has no adjustable backlight.".into()),
                Err(error) => Some(error.clone()),
            });
    let mute_key = output.key().to_string();
    let mute = components::button_surface(
        button(
            glyph(
                Icon::Volume { muted },
                if muted { "Unmute" } else { "Mute" }.into(),
            ),
            move |state: &mut ShellState| {
                super::control_task::submit(state, &mute_key, Action::ToggleMute, None);
            },
        )
        .disabled(audio.is_none() || controls.sender.is_none())
        .padding(7.0.px())
        .corner_radius(6.0.px()),
        components::ButtonSurface::Subtle,
        muted,
    );
    popup::panel_with_header(
        output,
        PopupKind::Controls,
        flex_row((
            components::section_title("Control center"),
            FlexSpacer::Flex(1.0),
            icon_button(Icon::Close, "Close control center", false, move |state| {
                state.dismiss_controls(&close_key)
            }),
        )),
        flex_col((
            super::quick_panel::shortcuts(output),
            flex_row((
                glyph(Icon::Brightness, "Display brightness".into()),
                label("Brightness").text_size(14.0).flex(1.0),
                label(brightness.map_or("Unavailable".into(), |value| format!("{value}%")))
                    .text_size(13.0)
                    .color(components::TEXT_MUTED),
            ))
            .gap(8.0.px()),
            sized_box(ControlSlider {
                name: "Brightness",
                output: output.key().into(),
                minimum: 1,
                value: brightness.unwrap_or(1),
                enabled: brightness.is_some() && controls.sender.is_some(),
            })
            .height(32.0.px()),
            brightness_reason.map(|reason| {
                label(reason)
                    .text_size(12.0)
                    .color(components::TEXT_MUTED)
                    .line_break_mode(LineBreaking::WordWrap)
            }),
            flex_row((
                mute,
                label("Volume").text_size(14.0).flex(1.0),
                label(if muted {
                    "Muted".into()
                } else {
                    volume.map_or("Unavailable".into(), |value| format!("{value}%"))
                })
                .text_size(13.0)
                .color(components::TEXT_MUTED),
            ))
            .gap(8.0.px()),
            sized_box(ControlSlider {
                name: "Volume",
                output: output.key().into(),
                minimum: 0,
                value: volume.unwrap_or(0),
                enabled: volume.is_some() && controls.sender.is_some(),
            })
            .height(32.0.px()),
            audio.map(|audio| {
                label(audio.output.clone())
                    .text_size(12.0)
                    .color(components::TEXT_MUTED)
                    .line_break_mode(LineBreaking::WordWrap)
            }),
            volume_reason.map(|reason| {
                label(reason)
                    .text_size(12.0)
                    .color(components::TEXT_MUTED)
                    .line_break_mode(LineBreaking::WordWrap)
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px()),
    )
}
