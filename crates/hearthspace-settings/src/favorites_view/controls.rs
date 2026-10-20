use crate::state::SettingsApp;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    WidgetView,
    masonry::{
        layout::AsUnit,
        properties::{Dimensions, LineBreaking, Padding},
    },
    style::Style,
    view::{MainAxisAlignment, button, flex_row, label},
};

pub(super) fn note(text: impl Into<String>) -> impl WidgetView<SettingsApp, Widget: Sized> {
    label(text.into())
        .text_size(11.0)
        .color(components::TEXT_MUTED)
        .line_break_mode(LineBreaking::WordWrap)
}

pub(super) fn icon_button(
    glyph: Icon,
    name: impl Into<String>,
    disabled: bool,
    action: impl Fn(&mut SettingsApp) + Send + Sync + 'static,
) -> impl WidgetView<SettingsApp, Widget: Sized> {
    components::button_surface(
        button(
            icon(glyph, 16.0).color(components::TEXT_MUTED).label(name),
            action,
        )
        .disabled(disabled),
        components::ButtonSurface::Bare,
        false,
    )
    .dims(Dimensions::fixed(44.0.px(), 44.0.px()))
    .padding(0.0.px())
    .corner_radius(10.0.px())
}

pub(super) fn text(
    glyph: Icon,
    name: &'static str,
    disabled: bool,
    action: impl Fn(&mut SettingsApp) + Send + Sync + 'static,
) -> impl WidgetView<SettingsApp, Widget: Sized> {
    components::button_surface(
        button(
            flex_row((icon(glyph, 16.0), label(name).text_size(11.0))).gap(7.0.px()),
            action,
        )
        .disabled(disabled),
        components::ButtonSurface::Control,
        false,
    )
    .height(44.0.px())
    .padding(Padding::from_vh(0.0.px(), 12.0.px()))
    .border_width(1.0.px())
    .corner_radius(10.0.px())
}

pub(super) fn save(disabled: bool) -> impl WidgetView<SettingsApp, Widget: Sized> {
    let color = if disabled {
        components::TEXT_MUTED
    } else {
        components::ACCENT_INK
    };
    components::button_surface(
        button(
            flex_row((
                icon(Icon::Check, 15.0).color(color),
                label("Save shortcut").text_size(12.0).color(color),
            ))
            .main_axis_alignment(MainAxisAlignment::Center)
            .gap(8.0.px()),
            |state: &mut SettingsApp| state.favorites.save(),
        )
        .disabled(disabled),
        components::ButtonSurface::Primary,
        false,
    )
    .height(44.0.px())
    .padding(Padding::from_vh(0.0.px(), 13.0.px()))
    .border_width(1.0.px())
    .corner_radius(10.0.px())
}
