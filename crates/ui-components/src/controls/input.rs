use super::ButtonVariant;
use crate::{ButtonSurface, Palette, SettingsButton, button_surface, settings_button};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{core::ArcStr, layout::AsUnit, properties::Padding},
    style::Style,
    view::{button as native_button, label},
};

pub(super) fn button<State: 'static, F: Fn(&mut State) + Send + Sync + 'static>(
    palette: Palette,
    text: ArcStr,
    variant: ButtonVariant,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State> {
    let text = label(text)
        .font("Liberation Sans")
        .text_size(if variant == ButtonVariant::Quiet {
            11.
        } else {
            12.
        });
    let text = text.color(
        if variant == ButtonVariant::Danger {
            xilem::Color::WHITE
        } else if variant == ButtonVariant::Primary {
            palette.accent_ink
        } else if variant == ButtonVariant::Quiet {
            palette.muted
        } else {
            palette.text
        }
        .multiply_alpha(if disabled { 0.5 } else { 1. }),
    );
    let inner = native_button(text, callback)
        .disabled(disabled)
        .height(36.0.px())
        .corner_radius(10.0.px())
        .padding(Padding::from_vh(
            8.0.px(),
            if variant == ButtonVariant::Quiet {
                8.0
            } else {
                12.0
            }
            .px(),
        ));
    if disabled && variant == ButtonVariant::Primary {
        return inner
            .background_color(palette.accent.multiply_alpha(0.5))
            .border_color(palette.accent_border.multiply_alpha(0.5))
            .boxed();
    }
    let result: Box<AnyWidgetView<State>> = match variant {
        ButtonVariant::Neutral => settings_button(inner, SettingsButton::Control, false)
            .disabled(disabled)
            .boxed(),
        ButtonVariant::Primary => settings_button(inner, SettingsButton::Primary, false)
            .disabled(disabled)
            .boxed(),
        ButtonVariant::Danger => button_surface(inner, ButtonSurface::Danger, false).boxed(),
        ButtonVariant::Quiet => settings_button(inner, SettingsButton::Bare, false)
            .disabled(disabled)
            .boxed(),
    };
    result
}
