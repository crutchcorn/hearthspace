use crate::{Palette, RADIUS_CONTROL, RADIUS_DETAIL, button_surface, icons, settings_material};
use xilem::{
    Color,
    masonry::{
        core::{DefaultProperties, PropertyStack, Selector},
        layout::AsUnit,
        properties::{
            Background, BorderColor, BorderWidth, CaretColor, CheckmarkColor, CheckmarkStrokeWidth,
            ContentColor, CornerRadius, Padding, PlaceholderColor, ThumbColor, TrackColor,
        },
        theme::default_property_set,
        widgets::{Button, Checkbox, Label, RadioButton, Slider, TextArea, TextInput},
    },
};

/// Shell controls may sit directly on translucent panels.
pub fn hearthspace_light_properties() -> DefaultProperties {
    hearthspace_properties(Palette::default(), PropertyStyle::Glass)
}

/// Opaque app surfaces can use the reference's brighter orange focus edge.
pub fn hearthspace_opaque_properties() -> DefaultProperties {
    hearthspace_properties(Palette::default(), PropertyStyle::Opaque)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyStyle {
    Glass,
    Opaque,
}

/// Cache the returned properties in an Arc and replace that Arc only when the
/// palette changes. WindowView applies it live without rebuilding widget IDs.
pub fn hearthspace_properties(palette: Palette, style: PropertyStyle) -> DefaultProperties {
    let focus_ring = match style {
        PropertyStyle::Glass => palette.focus_ring_on_glass,
        PropertyStyle::Opaque => palette.focus_ring,
    };
    let mut properties = default_property_set();
    properties.insert::<settings_material::widget::MaterialWidget, _>(palette);
    properties.insert::<icons::IconWidget, _>(ContentColor::new(palette.text));
    crate::palette::classes::properties(&mut properties, &palette);

    properties.insert::<Button, _>(Background::Color(Color::TRANSPARENT));
    properties.insert::<Button, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<Button, _>(BorderColor {
        color: Color::TRANSPARENT,
    });
    properties.insert::<Button, _>(CornerRadius {
        radius: RADIUS_CONTROL.px(),
    });
    properties.insert::<Button, _>(ContentColor::new(palette.text));
    properties.insert::<Button, _>(Padding::from_vh(7.0.px(), 10.0.px()));
    button_surface::properties(&mut properties, &palette, focus_ring);

    properties.insert::<Label, _>(ContentColor::new(palette.text));

    properties.insert::<Checkbox, _>(ContentColor::new(palette.text));
    properties.insert::<Checkbox, _>(Background::Color(palette.surface_control));
    properties.insert::<Checkbox, _>(BorderColor {
        color: palette.border,
    });
    properties.insert::<Checkbox, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<Checkbox, _>(CornerRadius {
        radius: RADIUS_DETAIL.px(),
    });
    properties.insert::<Checkbox, _>(CheckmarkColor::new(palette.accent_strong));
    properties.insert::<Checkbox, _>(CheckmarkStrokeWidth { width: 2.0 });
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            BorderColor {
                color: palette.accent,
            },
        );
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: focus_ring },
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(palette.accent_soft),
        );
        properties.insert_stack::<Checkbox>(stack);
    }

    properties.insert::<RadioButton, _>(ContentColor::new(palette.text));
    properties.insert::<RadioButton, _>(Background::Color(palette.surface_control));
    properties.insert::<RadioButton, _>(BorderColor {
        color: palette.border,
    });
    properties.insert::<RadioButton, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<RadioButton, _>(CheckmarkColor::new(palette.accent_strong));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            BorderColor {
                color: palette.accent,
            },
        );
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: focus_ring },
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(palette.accent_soft),
        );
        properties.insert_stack::<RadioButton>(stack);
    }

    properties.insert::<TextArea<true>, _>(ContentColor::new(palette.text));
    properties.insert::<TextArea<false>, _>(ContentColor::new(palette.text));

    properties.insert::<TextInput, _>(Background::Color(palette.surface_control));
    properties.insert::<TextInput, _>(BorderColor {
        color: palette.border,
    });
    properties.insert::<TextInput, _>(CaretColor {
        color: palette.text,
    });
    properties.insert::<TextInput, _>(PlaceholderColor::new(palette.text_muted));
    properties.insert::<TextInput, _>(CornerRadius {
        radius: RADIUS_CONTROL.px(),
    });
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: focus_ring },
        );
        properties.insert_stack::<TextInput>(stack);
    }
    properties.insert::<Slider, _>(TrackColor {
        active: palette.accent,
        inactive: palette.control_track,
    });
    properties.insert::<Slider, _>(ThumbColor(palette.control_thumb));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: focus_ring },
        );
        properties.insert_stack::<Slider>(stack);
    }

    properties
}
