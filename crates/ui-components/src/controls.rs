//! Shared form controls matching the web catalog, with native input semantics.
use xilem::{
    WidgetView,
    masonry::{core::ArcStr, layout::AsUnit, properties::Padding},
    style::Style,
    view::{label, text_input},
};

pub const FORM_FONT: &str = "Liberation Sans";
mod field;
mod input;
mod paint;
pub mod progress;
pub mod select;
pub mod style;
mod switch_row;
pub mod toggle;
mod view;
mod widget;
pub use view::{control, labelled_checkbox};
pub use widget::ControlKind;
use widget::ControlWidget;
/// The same native slider widget and painter used by the Shared Controls catalog.
pub type LevelSlider = widget::ControlWidget<xilem::masonry::widgets::SliderMoved>;

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
) -> impl WidgetView<State, Widget: Sized> {
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
) -> impl WidgetView<State, Widget: Sized> {
    input::button_content(palette, content, variant, disabled, callback)
}

/// Shared sizing; put app-specific focus/ownership wrappers around this native control.
pub fn button_layout<
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action, Widget: Sized>,
>(
    inner: V,
    variant: ButtonVariant,
) -> impl WidgetView<State, Action, Widget: Sized> {
    inner
        .background_color(xilem::Color::TRANSPARENT)
        .border_color(xilem::Color::TRANSPARENT)
        .height(style::BUTTON_HEIGHT.px())
        .corner_radius(10.0.px())
        .padding(Padding::from_vh(
            8.0.px(),
            if variant == ButtonVariant::Quiet {
                8.0
            } else {
                12.0
            }
            .px(),
        ))
}
/// Shared backdrop; ownership adapters may wrap the native button inside this frame.
pub fn button_frame<State: 'static, Action: 'static, V: WidgetView<State, Action>>(
    inner: V,
    variant: ButtonVariant,
    disabled: bool,
) -> crate::settings_material::MaterialView<V> {
    crate::settings_material::form_button(inner, variant, disabled)
}
pub fn form_icon_button<State: 'static, F: Fn(&mut State) + Send + Sync + 'static>(
    palette: crate::Palette,
    glyph: crate::icons::Icon,
    name: impl Into<String>,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    icon_button(palette, glyph, name.into(), disabled, callback, 36.)
}
/// Same icon-button material, with an explicit 44-unit finger target for dense rows.
pub fn touch_icon_button<State: 'static, F: Fn(&mut State) + Send + Sync + 'static>(
    palette: crate::Palette,
    glyph: crate::icons::Icon,
    name: impl Into<String>,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    icon_button(palette, glyph, name.into(), disabled, callback, 44.)
}
fn icon_button<State: 'static, F: Fn(&mut State) + Send + Sync + 'static>(
    palette: crate::Palette,
    glyph: crate::icons::Icon,
    name: String,
    disabled: bool,
    callback: F,
    target: f64,
) -> impl WidgetView<State, Widget: Sized> {
    let content = crate::icons::icon(glyph, 18.).color(style::button_foreground(
        &palette,
        ButtonVariant::Quiet,
        disabled,
    ));
    let button = button_layout(
        crate::semantic_button(content, name, None, callback),
        ButtonVariant::Quiet,
    )
    .dims(xilem::masonry::properties::Dimensions::fixed(
        target.px(),
        target.px(),
    ));
    button_frame(button, ButtonVariant::Quiet, disabled)
}
pub fn form_button_with_icon<State: 'static, F: Fn(&mut State) + Send + Sync + 'static>(
    palette: crate::Palette,
    glyph: crate::icons::Icon,
    text: impl Into<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    let color = style::button_foreground(&palette, variant, disabled);
    form_button_content(
        palette,
        xilem::view::flex_row((
            crate::icons::icon(glyph, 16.).color(color),
            form_label(text)
                .text_size(if variant == ButtonVariant::Quiet {
                    11.
                } else {
                    12.
                })
                .color(color),
        ))
        .gap(6.0.px())
        .width(xilem::masonry::layout::Dim::FitContent),
        variant,
        disabled,
        callback,
    )
}

/// Shared field appearance around an editor/combobox with its own input adapter.
pub fn field_container<State: 'static, V: WidgetView<State>>(
    inner: V,
    invalid: bool,
) -> crate::settings_material::MaterialView<V> {
    crate::settings_material::form_field(inner, invalid)
}

pub fn text_field<State: 'static, F: Fn(&mut State, String) + Send + Sync + 'static>(
    value: String,
    placeholder: &str,
    invalid: bool,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    let input = text_input(value, callback)
        .font(FORM_FONT)
        .text_size(12.0)
        .placeholder(placeholder.to_string())
        .disabled(disabled)
        .padding(Padding::from_vh(10.0.px(), 14.0.px()))
        .height(style::FIELD_HEIGHT.px())
        .background_color(xilem::Color::TRANSPARENT)
        .border_color(xilem::Color::TRANSPARENT);
    // Resolve validation through the shared stack, retaining the editor's
    // native focused-border treatment.
    field_container(
        field::FieldView(crate::palette::classes::class(
            input,
            if invalid {
                "hearthspace-invalid-field"
            } else {
                "hearthspace-form-field"
            },
        )),
        invalid,
    )
    .disabled(disabled)
}

pub fn form_label(text: impl Into<ArcStr>) -> xilem::view::Label {
    label(text).font(FORM_FONT).text_size(12.0)
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
