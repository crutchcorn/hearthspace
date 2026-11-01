//! The dock adapter keeps the native raised button's box and state tokens.
use super::widget::ResultButton;
use hearthspace_ui_components::Palette;
use xilem::masonry::{
    core::{DefaultProperties, PropertyStack, Selector},
    layout::AsUnit,
    properties::{Background, BorderColor, BorderWidth},
};

pub(super) const RAISED: &str = "shell-raised-app-control";
pub(super) const SELECTED: &str = "shell-selected-app-control";

pub(in crate::ui) fn theme_properties(
    properties: &mut DefaultProperties,
    palette: Palette,
    mut stack: PropertyStack,
) {
    // ButtonSurface's defaults are registered for native Button and
    // SemanticButton, not this app-context adapter. Scope the same raised
    // states to dock controls so launcher rows retain their existing geometry.
    stack.push_layer(
        Selector::classes(&[RAISED]),
        (
            BorderWidth { width: 1.0.px() },
            BorderColor {
                color: xilem::Color::TRANSPARENT,
            },
        ),
    );
    stack.push_layer(
        Selector::classes(&[RAISED]).with_hovered(true),
        Background::Color(palette.raised_button_hover),
    );
    stack.push_layer(
        Selector::classes(&[RAISED, SELECTED]),
        (
            Background::Color(palette.accent_soft),
            BorderColor {
                color: palette.accent_border,
            },
        ),
    );
    stack.push_layer(
        Selector::classes(&[RAISED]).with_active(true),
        Background::Color(palette.button_active),
    );
    stack.push_layer(
        Selector::classes(&[RAISED]).with_focused(true),
        BorderColor {
            color: palette.focus_ring_on_glass,
        },
    );
    properties.insert::<ResultButton, _>(palette);
    properties.insert_stack::<ResultButton>(stack);
}
