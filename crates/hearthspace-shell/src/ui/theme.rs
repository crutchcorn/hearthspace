use xilem::{
    Color,
    masonry::{
        core::{DefaultProperties, PropertyStack, Selector},
        layout::AsUnit,
        properties::{
            Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Padding,
            PlaceholderColor,
        },
        theme::default_property_set,
        widgets::{Button, Label, TextArea, TextInput},
    },
};

use super::constants::LAUNCHER_ITEM_H_PAD;

/// Window background for the bar and launcher palette.
pub(super) const SHELL_BG: Color = Color::from_rgb8(0xf4, 0xf4, 0xf5);
const SHELL_TEXT: Color = Color::from_rgb8(0x1f, 0x1f, 0x23);
const SHELL_PLACEHOLDER: Color = Color::from_rgb8(0x8a, 0x8a, 0x93);
const BUTTON_HOVER_BG: Color = Color::from_rgb8(0xe4, 0xe4, 0xe7);
const BUTTON_ACTIVE_BG: Color = Color::from_rgb8(0xd4, 0xd4, 0xd8);
const INPUT_BG: Color = Color::from_rgb8(0xff, 0xff, 0xff);
const INPUT_BORDER: Color = Color::from_rgb8(0xd4, 0xd4, 0xd8);
const INPUT_FOCUS_BORDER: Color = Color::from_rgb8(0x3b, 0x7e, 0xe4);

/// Light theme overrides applied to every shell widget.
pub(super) fn shell_properties() -> DefaultProperties {
    let mut properties = default_property_set();

    properties.insert::<Button, _>(Background::Color(Color::TRANSPARENT));
    properties.insert::<Button, _>(BorderWidth { width: 0.0.px() });
    properties.insert::<Button, _>(CornerRadius { radius: 6.0.px() });
    properties.insert::<Button, _>(ContentColor::new(SHELL_TEXT));
    properties.insert::<Button, _>(Padding::from_vh(8.0.px(), LAUNCHER_ITEM_H_PAD.px()));
    {
        let mut stack = PropertyStack::new();
        stack.push(
            Selector::new().with_hovered(true),
            Background::Color(BUTTON_HOVER_BG),
        );
        stack.push(
            Selector::new().with_active(true),
            Background::Color(BUTTON_ACTIVE_BG),
        );
        properties.insert_stack::<Button>(stack);
    }

    properties.insert::<Label, _>(ContentColor::new(SHELL_TEXT));
    properties.insert::<TextArea<true>, _>(ContentColor::new(SHELL_TEXT));
    properties.insert::<TextArea<false>, _>(ContentColor::new(SHELL_TEXT));

    properties.insert::<TextInput, _>(Background::Color(INPUT_BG));
    properties.insert::<TextInput, _>(BorderColor {
        color: INPUT_BORDER,
    });
    properties.insert::<TextInput, _>(CaretColor { color: SHELL_TEXT });
    properties.insert::<TextInput, _>(PlaceholderColor::new(SHELL_PLACEHOLDER));
    {
        let mut stack = PropertyStack::new();
        stack.push(
            Selector::new().with_focused(true),
            BorderColor {
                color: INPUT_FOCUS_BORDER,
            },
        );
        properties.insert_stack::<TextInput>(stack);
    }

    properties
}
