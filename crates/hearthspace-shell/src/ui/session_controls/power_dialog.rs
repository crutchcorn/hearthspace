//! The shared destructive confirmation keeps Cancel as its initial focus.
use super::{
    Confirmation, power_confirmation,
    power_view::{feedback, observed},
};
use crate::ui::{
    canvas_search::material,
    popup_group,
    search_focus::{Record, focus_on_activation},
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_ui_components as components;
use xilem::{
    WidgetView,
    masonry::{
        layout::AsUnit,
        parley::style::{FontWeight, LineHeight},
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, MainAxisAlignment, button, flex_col, flex_row, label, sized_box},
};

pub(super) fn view(
    state: &ShellState,
    output: &ShellOutputInstance,
    action: Confirmation,
) -> impl WidgetView<ShellState> + use<> {
    let palette = state.appearance.palette;
    let target = output.power_ui.cancel.clone();
    let cancel_key = output.key().to_owned();
    let confirm_key = cancel_key.clone();
    let cancel = Record {
        inner: components::button_surface(
            button(
                label(if action.pending(state) {
                    "Close"
                } else {
                    "Cancel"
                })
                .font("Liberation Sans")
                .text_size(13.0)
                .line_height(LineHeight::FontSizeRelative(1.0))
                .color(palette.text),
                move |state: &mut ShellState| state.dismiss_power(&cancel_key),
            )
            // Include the focus border in the reference's 10px / 14px padding.
            .padding(Padding::from_vh(9.0.px(), 13.0.px()))
            .height(33.0.px())
            .corner_radius(10.0.px()),
            components::ButtonSurface::Control,
            false,
        ),
        target: target.clone(),
    };
    let confirm = components::button_surface(
        button(
            label(if action.pending(state) {
                match action {
                    Confirmation::Reboot => "Rebooting…",
                    Confirmation::PowerOff => "Turning off…",
                    Confirmation::LogOff => action.label(),
                }
            } else {
                action.label()
            })
            .font("Liberation Sans")
            .text_size(13.0)
            .line_height(LineHeight::FontSizeRelative(1.0))
            .color(xilem::Color::WHITE),
            move |state: &mut ShellState| power_confirmation::confirm(state, &confirm_key, action),
        )
        .disabled(!action.enabled(state))
        .padding(Padding::from_vh(9.0.px(), 13.0.px()))
        .height(33.0.px())
        .corner_radius(10.0.px()),
        components::ButtonSurface::Danger,
        false,
    );
    let error = output
        .launch_error
        .clone()
        .or_else(|| action.unavailable(state));
    let content = flex_col((
        label(action.title())
            .font("Liberation Sans")
            .text_size(19.0)
            .line_height(LineHeight::FontSizeRelative(1.0))
            .weight(FontWeight::BOLD)
            .color(palette.text)
            .line_break_mode(LineBreaking::WordWrap),
        label(action.body())
            .font("Liberation Sans")
            .text_size(14.0)
            .line_height(LineHeight::FontSizeRelative(1.6))
            .line_break_mode(LineBreaking::WordWrap)
            .color(palette.muted),
        error.map(|error| feedback(error, palette)),
        sized_box(
            flex_row((cancel, confirm))
                .main_axis_alignment(MainAxisAlignment::End)
                .gap(8.0.px()),
        )
        .padding(Padding {
            top: 12.0.px(),
            ..Default::default()
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(12.0.px());
    let dialog = popup_group::group(
        action.title(),
        components::touch_portal(observed(output, 25.0, content))
            .constrain_horizontal(true)
            .must_fill(false),
    )
    .dialog();
    focus_on_activation(
        material::shell(sized_box(dialog).padding(25.0.px()), palette),
        target,
        output.key().to_owned(),
    )
    .with_dismiss(ShellState::dismiss_power)
    .with_focus_recovery()
    .dismiss_directly()
}
