use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        parley::style::LineHeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row},
};

use crate::{
    frame_settings::Operation,
    page_components::{note, section, toggle_row},
    state::SettingsApp,
    typography::label,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    flex_col((
        section(
            "Frame layout",
            flex_col((
                label("Arrange and group your frame items, or change how much detail they show.")
                    .text_size(12.0)
                    .line_height(LineHeight::FontSizeRelative(1.6))
                    .line_break_mode(LineBreaking::WordWrap)
                    .color(palette.muted),
                action(state, "Change layout", Icon::FrameLayout, 15.0, Operation::OpenEditor),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(8.0.px()),
        ),
        section(
            "Canvas tools",
            flex_col((
                toggle_row(
                    "Make canvas tools hide-able",
                    Some("Removing canvas tools hides their buttons. Press / or Ctrl+K to find and restore them with search."),
                    state.frame_settings.snapshot.is_some_and(|snapshot| snapshot.allow_hide_tools),
                    !state.frame_settings.enabled(),
                    |state, allow| state.frame_settings.request(Operation::AllowHideTools(allow)),
                ),
                action(state, "Restore canvas tools", Icon::RotateCcw, 12.0, Operation::RestoreTools),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
        state.frame_settings.message().map(|message| note(palette, message)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(27.0.px())
    .boxed()
}
fn action(
    state: &SettingsApp,
    name: &'static str,
    glyph: Icon,
    size: f64,
    operation: Operation,
) -> impl WidgetView<SettingsApp> + use<> {
    let palette = state.palette();
    components::settings_button(
        button(
            flex_row((
                icon(glyph, size).color(palette.muted),
                label(name).text_size(10.0).color(palette.muted),
            ))
            .gap(7.0.px()),
            move |state: &mut SettingsApp| state.frame_settings.request(operation),
        )
        .height(36.0.px())
        .padding(Padding::from_vh(8.0.px(), 10.0.px())),
        components::SettingsButton::Bare,
        false,
    )
    .disabled(!state.frame_settings.enabled())
}
