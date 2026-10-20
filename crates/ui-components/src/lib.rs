use xilem::{
    Color, TextAlign, WidgetView,
    masonry::{
        core::{ArcStr, DefaultProperties, PropertyStack, Selector},
        layout::AsUnit,
        parley::{fontique::GenericFamily, style::FontWeight},
        properties::{
            Background, BorderColor, BorderWidth, CaretColor, CheckmarkColor, CheckmarkStrokeWidth,
            ContentColor, CornerRadius, Padding, PlaceholderColor, ThumbColor, TrackColor,
        },
        theme::default_property_set,
        widgets::{Button, Checkbox, Label, RadioButton, Slider, TextArea, TextInput},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, label, sized_box},
};

mod touch_scroll;
pub use touch_scroll::{TouchScroll, touch_portal};
mod button_surface;
pub use button_surface::{ButtonSurface, ButtonSurfaceView, button_surface};
mod theme;
pub use theme::*;
#[cfg(test)]
mod focus_tests;
pub mod icons;
pub mod settings_material;
pub use settings_material::{SettingsButton, SettingsSurface, settings_button, settings_surface};

/// Shell controls may sit directly on translucent panels.
pub fn hearthspace_light_properties() -> DefaultProperties {
    light_properties(FOCUS_RING_ON_GLASS)
}

/// Opaque app surfaces can use the reference's brighter orange focus edge.
pub fn hearthspace_opaque_properties() -> DefaultProperties {
    light_properties(FOCUS_RING)
}

fn light_properties(focus_ring: Color) -> DefaultProperties {
    let mut properties = default_property_set();

    properties.insert::<Button, _>(Background::Color(Color::TRANSPARENT));
    properties.insert::<Button, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<Button, _>(BorderColor {
        color: Color::TRANSPARENT,
    });
    properties.insert::<Button, _>(CornerRadius {
        radius: RADIUS_CONTROL.px(),
    });
    properties.insert::<Button, _>(ContentColor::new(TEXT));
    properties.insert::<Button, _>(Padding::from_vh(7.0.px(), 10.0.px()));
    button_surface::properties(&mut properties, focus_ring);

    properties.insert::<Label, _>(ContentColor::new(TEXT));

    properties.insert::<Checkbox, _>(ContentColor::new(TEXT));
    properties.insert::<Checkbox, _>(Background::Color(SURFACE_CONTROL));
    properties.insert::<Checkbox, _>(BorderColor { color: BORDER });
    properties.insert::<Checkbox, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<Checkbox, _>(CornerRadius {
        radius: RADIUS_DETAIL.px(),
    });
    properties.insert::<Checkbox, _>(CheckmarkColor::new(ACCENT_STRONG));
    properties.insert::<Checkbox, _>(CheckmarkStrokeWidth { width: 2.0 });
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            BorderColor { color: ACCENT },
        );
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: focus_ring },
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(ACCENT_SUBTLE),
        );
        properties.insert_stack::<Checkbox>(stack);
    }

    properties.insert::<RadioButton, _>(ContentColor::new(TEXT));
    properties.insert::<RadioButton, _>(Background::Color(SURFACE_CONTROL));
    properties.insert::<RadioButton, _>(BorderColor { color: BORDER });
    properties.insert::<RadioButton, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<RadioButton, _>(CheckmarkColor::new(ACCENT_STRONG));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            BorderColor { color: ACCENT },
        );
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: focus_ring },
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(ACCENT_SUBTLE),
        );
        properties.insert_stack::<RadioButton>(stack);
    }

    properties.insert::<TextArea<true>, _>(ContentColor::new(TEXT));
    properties.insert::<TextArea<false>, _>(ContentColor::new(TEXT));

    properties.insert::<TextInput, _>(Background::Color(SURFACE_CONTROL));
    properties.insert::<TextInput, _>(BorderColor { color: BORDER });
    properties.insert::<TextInput, _>(CaretColor { color: TEXT });
    properties.insert::<TextInput, _>(PlaceholderColor::new(TEXT_MUTED));
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
        active: ACCENT,
        inactive: CONTROL_TRACK,
    });
    properties.insert::<Slider, _>(ThumbColor(CONTROL_THUMB));
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

pub fn page_title(text: impl Into<ArcStr>) -> xilem::view::Label {
    label(text)
        .font(GenericFamily::SystemUi)
        .text_size(24.0)
        .weight(FontWeight::BOLD)
}

pub fn section_title(text: impl Into<ArcStr>) -> xilem::view::Label {
    label(text)
        .font(GenericFamily::SystemUi)
        .text_size(17.0)
        .weight(FontWeight::BOLD)
}

pub fn field_label(text: impl Into<ArcStr>) -> xilem::view::Label {
    label(text).font(GenericFamily::SystemUi).text_size(13.0)
}

pub fn muted_text<State: 'static, T: Into<ArcStr>>(
    text: T,
) -> impl WidgetView<State> + use<State, T> {
    label(text)
        .font(GenericFamily::SystemUi)
        .text_size(13.0)
        .color(TEXT_MUTED)
}

pub fn app_background<State: 'static, V: WidgetView<State>>(
    content: V,
) -> impl WidgetView<State> + use<State, V> {
    sized_box(content)
        .background_color(APP_BACKGROUND)
        .padding(20.0.px())
}

pub fn sidebar_panel<State: 'static, V: WidgetView<State>>(
    content: V,
) -> impl WidgetView<State> + use<State, V> {
    sized_box(content)
        .background_color(SURFACE)
        .border(BORDER, 1.0.px())
        .corner_radius(RADIUS_TILE.px())
        .padding(10.0.px())
}

pub fn scroll_area<State: 'static, V: WidgetView<State>>(
    content: V,
) -> impl WidgetView<State> + use<State, V> {
    touch_portal(content)
        .constrain_horizontal(true)
        .must_fill(true)
}

pub fn settings_section<State: 'static, V: WidgetView<State>, T: Into<ArcStr>>(
    title: T,
    content: V,
) -> impl WidgetView<State> + use<State, V, T> {
    sized_box(
        flex_col((section_title(title), content))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
    )
    .background_color(PAPER)
    .border(BORDER, 1.0.px())
    .corner_radius(RADIUS_TILE.px())
    .padding(16.0.px())
}

pub fn sidebar_item<State: 'static, Action: 'static, T, F>(
    text: T,
    selected: bool,
    callback: F,
) -> impl WidgetView<State, Action> + use<State, Action, T, F>
where
    T: Into<ArcStr>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    button_surface(
        button(
            sized_box(
                label(text)
                    .font(GenericFamily::SystemUi)
                    .text_size(13.0)
                    .text_alignment(TextAlign::Start)
                    .color(TEXT),
            )
            .fixed_width(160.0.px()),
            callback,
        )
        .corner_radius(RADIUS_CONTROL.px()),
        ButtonSurface::Bare,
        selected,
    )
}

pub fn action_button<State: 'static, Action: 'static, T, F>(
    text: T,
    callback: F,
) -> impl WidgetView<State, Action> + use<State, Action, T, F>
where
    T: Into<ArcStr>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    button_surface(
        button(
            label(text)
                .font(GenericFamily::SystemUi)
                .text_size(13.0)
                .text_alignment(TextAlign::Center)
                .color(TEXT),
            callback,
        )
        .corner_radius(RADIUS_CONTROL.px()),
        ButtonSurface::Control,
        false,
    )
}

pub fn compact_action_button<State: 'static, Action: 'static, T, F>(
    text: T,
    callback: F,
) -> impl WidgetView<State, Action> + use<State, Action, T, F>
where
    T: Into<ArcStr>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    button_surface(
        button(
            label(text)
                .font(GenericFamily::SystemUi)
                .text_alignment(TextAlign::Center)
                .text_size(13.0)
                .color(TEXT),
            callback,
        )
        .corner_radius(RADIUS_CONTROL.px())
        .padding(Padding::from_vh(5.0.px(), 9.0.px())),
        ButtonSurface::Subtle,
        false,
    )
}

pub fn launcher_item<State: 'static, Action: 'static, T, F>(
    text: T,
    width: f64,
    callback: F,
) -> impl WidgetView<State, Action> + use<State, Action, T, F>
where
    T: Into<ArcStr>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    button_surface(
        button(
            sized_box(
                label(text)
                    .font(GenericFamily::SystemUi)
                    .text_size(13.0)
                    .text_alignment(TextAlign::Start)
                    .color(TEXT),
            )
            .fixed_width(width.px()),
            callback,
        )
        .corner_radius(RADIUS_CONTROL.px()),
        ButtonSurface::Bare,
        false,
    )
}

pub fn segmented_option<State: 'static, Action: 'static, T, F>(
    text: T,
    selected: bool,
    callback: F,
) -> impl WidgetView<State, Action> + use<State, Action, T, F>
where
    T: Into<ArcStr>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    button_surface(
        button(
            label(text)
                .font(GenericFamily::SystemUi)
                .text_size(13.0)
                .text_alignment(TextAlign::Center)
                .color(TEXT),
            callback,
        )
        .corner_radius(RADIUS_CONTROL.px()),
        ButtonSurface::Control,
        selected,
    )
}
