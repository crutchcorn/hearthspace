use super::ButtonVariant;
use crate::Palette;
use xilem::{
    WidgetView,
    masonry::core::ArcStr,
    style::Style,
    view::{button as native_button, label},
};

pub(super) fn button<State: 'static, F: Fn(&mut State) + Send + Sync + 'static>(
    palette: Palette,
    text: ArcStr,
    variant: ButtonVariant,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    let text = label(text)
        .font("Liberation Sans")
        .text_size(if variant == ButtonVariant::Quiet {
            11.
        } else {
            12.
        });
    let text = text.color(super::style::button_foreground(&palette, variant, disabled));
    button_content(palette, text, variant, disabled, callback)
}

pub(super) fn button_content<
    State: 'static,
    V: WidgetView<State>,
    F: Fn(&mut State) + Send + Sync + 'static,
>(
    _palette: Palette,
    content: V,
    variant: ButtonVariant,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    super::button_frame(
        super::button_layout(native_button(content, callback).disabled(disabled), variant),
        variant,
        disabled,
    )
}
