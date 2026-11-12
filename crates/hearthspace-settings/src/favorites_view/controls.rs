use crate::{state::SettingsApp, typography::label};
use hearthspace_ui_components::{
    self as components,
    controls::{self, ButtonVariant},
    icons::Icon,
};
use xilem::{WidgetView, masonry::properties::LineBreaking, style::Style};
pub(super) fn note(
    palette: components::Palette,
    text: impl Into<String>,
) -> impl WidgetView<SettingsApp, Widget: Sized> {
    label(text.into())
        .text_size(11.0)
        .color(palette.muted)
        .line_break_mode(LineBreaking::WordWrap)
}

pub(super) fn icon_button(
    palette: components::Palette,
    glyph: Icon,
    name: impl Into<String>,
    disabled: bool,
    action: impl Fn(&mut SettingsApp) + Send + Sync + 'static,
) -> impl WidgetView<SettingsApp, Widget: Sized> {
    controls::touch_icon_button(palette, glyph, name, disabled, action)
}
pub(super) fn text(
    palette: components::Palette,
    glyph: Icon,
    name: &'static str,
    disabled: bool,
    action: impl Fn(&mut SettingsApp) + Send + Sync + 'static,
) -> impl WidgetView<SettingsApp, Widget: Sized> {
    controls::form_button_with_icon(
        palette,
        glyph,
        name,
        ButtonVariant::Neutral,
        disabled,
        action,
    )
}

pub(super) fn save(
    palette: components::Palette,
    disabled: bool,
) -> impl WidgetView<SettingsApp, Widget: Sized> {
    controls::form_button_with_icon(
        palette,
        Icon::Check,
        "Save shortcut",
        ButtonVariant::Primary,
        disabled,
        |state: &mut SettingsApp| state.favorites.save(),
    )
}
