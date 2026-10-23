use hearthspace_ui_components::{self as components, SettingsButton};
use xilem::{
    WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row, label, sized_box},
};
use xilem_storybook::StorybookState;

pub(super) fn preview(state: &StorybookState) -> impl WidgetView<StorybookState> + use<> {
    let selected = state.selected_bool("selected").unwrap_or(false);
    let disabled = state.selected_bool("disabled").unwrap_or(false);
    let controls = [
        ("Bare", SettingsButton::Bare),
        ("Control", SettingsButton::Control),
        ("Outline", SettingsButton::Outline),
        ("Card", SettingsButton::Card),
        ("Primary", SettingsButton::Primary),
    ]
    .map(|(name, kind)| {
        components::settings_button(
            button(
                label(name).font("Liberation Sans").text_size(11.0).color(
                    if kind == SettingsButton::Primary && !disabled {
                        components::ACCENT_INK
                    } else {
                        components::TEXT
                    },
                ),
                |state: &mut StorybookState| {
                    let current = state.selected_bool("selected").unwrap_or(false);
                    state.set_selected_bool("selected", !current);
                },
            )
            .height(40.0.px())
            .padding(12.0.px()),
            kind,
            selected,
        )
        .disabled(disabled)
    });
    components::settings_surface(
        sized_box(
            flex_col((
                label("Hover, click, or Tab to compare the control states.")
                    .font("Liberation Sans")
                    .text_size(11.0)
                    .color(components::settings_material::MUTED),
                flex_row(controls).gap(12.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(18.0.px()),
        )
        .padding(24.0.px()),
        components::SettingsSurface::Subtle,
    )
}
