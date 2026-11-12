//! Shared form controls matching the web catalog, with native input semantics.
use xilem::{
    WidgetView,
    masonry::{core::ArcStr, layout::AsUnit, properties::Padding},
    style::Style,
    view::{label, text_input},
};

pub mod compact_slider;
mod field;
mod input;
mod paint;
pub mod progress;
pub mod select;
mod switch_row;
pub mod toggle;
mod view;
mod widget;
pub use view::control;
pub use widget::ControlKind;
use widget::ControlWidget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Neutral,
    Primary,
    Danger,
    Quiet,
}

pub fn form_button<State: 'static, F: Fn(&mut State) + Send + Sync + 'static>(
    palette: crate::Palette,
    text: impl Into<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State> {
    input::button(palette, text.into(), variant, disabled, callback)
}

/// Form-button sizing and materials around a consumer-supplied icon/label view.
/// The content must provide an accessible name (normally its text label).
pub fn form_button_content<
    State: 'static,
    V: WidgetView<State>,
    F: Fn(&mut State) + Send + Sync + 'static,
>(
    palette: crate::Palette,
    content: V,
    variant: ButtonVariant,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State> {
    input::button_content(palette, content, variant, disabled, callback)
}

pub fn text_field<State: 'static, F: Fn(&mut State, String) + Send + Sync + 'static>(
    value: String,
    placeholder: &str,
    invalid: bool,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    let input = text_input(value, callback)
        .font("Liberation Sans")
        .text_size(12.0)
        .placeholder(placeholder.to_string())
        .disabled(disabled)
        .padding(Padding::from_vh(10.0.px(), 14.0.px()))
        .height(42.0.px());
    // Resolve validation through the shared stack, retaining the editor's
    // native focused-border treatment.
    field::FieldView(crate::palette::classes::class(
        input,
        if invalid {
            "hearthspace-invalid-field"
        } else {
            "hearthspace-form-field"
        },
    ))
}

pub fn form_label(text: impl Into<ArcStr>) -> xilem::view::Label {
    label(text).font("Liberation Sans").text_size(12.0)
}

pub(crate) fn theme_properties(
    p: &mut xilem::masonry::core::DefaultProperties,
    palette: crate::Palette,
) {
    p.insert::<widget::ControlWidget, _>(palette);
    p.insert::<widget::ControlWidget<xilem::masonry::widgets::SliderMoved>, _>(palette);
    p.insert::<toggle::NamedSwitch, _>(palette);
    p.insert::<progress::ProgressWidget, _>(palette);
    select::theme_properties(p, palette);
}

#[cfg(test)]
mod tests;
