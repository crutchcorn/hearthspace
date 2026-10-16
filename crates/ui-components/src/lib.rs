use xilem::{
    Color, TextAlign, WidgetView,
    masonry::{
        core::{ArcStr, DefaultProperties, PropertyStack, Selector},
        layout::AsUnit,
        parley::{fontique::GenericFamily, style::FontWeight},
        properties::{
            Background, BorderColor, BorderWidth, CaretColor, CheckmarkColor, CheckmarkStrokeWidth,
            ContentColor, CornerRadius, Padding, PlaceholderColor,
        },
        theme::default_property_set,
        widgets::{Button, Checkbox, Label, RadioButton, TextArea, TextInput},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, label, sized_box},
};

mod touch_scroll;
pub use touch_scroll::touch_portal;

pub const APP_BACKGROUND: Color = Color::from_rgb8(0xf6, 0xf7, 0xf9);
pub const SURFACE: Color = Color::from_rgb8(0xff, 0xff, 0xff);
pub const SURFACE_SUBTLE: Color = Color::from_rgb8(0xf0, 0xf2, 0xf5);
pub const BORDER: Color = Color::from_rgb8(0xd8, 0xde, 0xe8);
pub const TEXT: Color = Color::from_rgb8(0x1f, 0x24, 0x2a);
pub const TEXT_MUTED: Color = Color::from_rgb8(0x68, 0x72, 0x80);
pub const ACCENT: Color = Color::from_rgb8(0x2f, 0x76, 0xc2);
pub const ACCENT_SUBTLE: Color = Color::from_rgb8(0xe5, 0xf0, 0xfb);
pub const BUTTON_HOVER: Color = Color::from_rgb8(0xe8, 0xec, 0xf2);
pub const BUTTON_ACTIVE: Color = Color::from_rgb8(0xd9, 0xe2, 0xee);
pub const DANGER_SUBTLE: Color = Color::from_rgb8(0xfd, 0xee, 0xed);

pub fn hearthspace_light_properties() -> DefaultProperties {
    let mut properties = default_property_set();

    properties.insert::<Button, _>(Background::Color(Color::TRANSPARENT));
    properties.insert::<Button, _>(BorderWidth { width: 0.0.px() });
    properties.insert::<Button, _>(CornerRadius { radius: 6.0.px() });
    properties.insert::<Button, _>(ContentColor::new(TEXT));
    properties.insert::<Button, _>(Padding::from_vh(7.0.px(), 10.0.px()));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            Background::Color(BUTTON_HOVER),
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(BUTTON_ACTIVE),
        );
        properties.insert_stack::<Button>(stack);
    }

    properties.insert::<Label, _>(ContentColor::new(TEXT));

    properties.insert::<Checkbox, _>(ContentColor::new(TEXT));
    properties.insert::<Checkbox, _>(Background::Color(SURFACE));
    properties.insert::<Checkbox, _>(BorderColor { color: BORDER });
    properties.insert::<Checkbox, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<Checkbox, _>(CornerRadius { radius: 4.0.px() });
    properties.insert::<Checkbox, _>(CheckmarkColor::new(ACCENT));
    properties.insert::<Checkbox, _>(CheckmarkStrokeWidth { width: 2.0 });
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            BorderColor { color: ACCENT },
        );
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: ACCENT },
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(ACCENT_SUBTLE),
        );
        properties.insert_stack::<Checkbox>(stack);
    }

    properties.insert::<RadioButton, _>(ContentColor::new(TEXT));
    properties.insert::<RadioButton, _>(Background::Color(SURFACE));
    properties.insert::<RadioButton, _>(BorderColor { color: BORDER });
    properties.insert::<RadioButton, _>(BorderWidth { width: 1.0.px() });
    properties.insert::<RadioButton, _>(CheckmarkColor::new(ACCENT));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            BorderColor { color: ACCENT },
        );
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: ACCENT },
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(ACCENT_SUBTLE),
        );
        properties.insert_stack::<RadioButton>(stack);
    }

    properties.insert::<TextArea<true>, _>(ContentColor::new(TEXT));
    properties.insert::<TextArea<false>, _>(ContentColor::new(TEXT));

    properties.insert::<TextInput, _>(Background::Color(SURFACE));
    properties.insert::<TextInput, _>(BorderColor { color: BORDER });
    properties.insert::<TextInput, _>(CaretColor { color: TEXT });
    properties.insert::<TextInput, _>(PlaceholderColor::new(TEXT_MUTED));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_focused(true),
            BorderColor { color: ACCENT },
        );
        properties.insert_stack::<TextInput>(stack);
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
    label(text).font(GenericFamily::SystemUi).text_size(14.0)
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
        .corner_radius(8.0.px())
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
    .background_color(SURFACE)
    .border(BORDER, 1.0.px())
    .corner_radius(8.0.px())
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
    let background = if selected {
        ACCENT_SUBTLE
    } else {
        Color::TRANSPARENT
    };
    let border = if selected { ACCENT } else { Color::TRANSPARENT };
    let text_color = if selected { ACCENT } else { TEXT };

    button(
        sized_box(
            label(text)
                .font(GenericFamily::SystemUi)
                .text_alignment(TextAlign::Start)
                .color(text_color),
        )
        .fixed_width(160.0.px()),
        callback,
    )
    .background_color(background)
    .border(border, 1.0.px())
    .corner_radius(6.0.px())
}

pub fn action_button<State: 'static, Action: 'static, T, F>(
    text: T,
    callback: F,
) -> impl WidgetView<State, Action> + use<State, Action, T, F>
where
    T: Into<ArcStr>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    button(
        label(text)
            .font(GenericFamily::SystemUi)
            .text_alignment(TextAlign::Center)
            .color(TEXT),
        callback,
    )
    .background_color(SURFACE)
    .border(BORDER, 1.0.px())
    .corner_radius(6.0.px())
}

pub fn compact_action_button<State: 'static, Action: 'static, T, F>(
    text: T,
    callback: F,
) -> impl WidgetView<State, Action> + use<State, Action, T, F>
where
    T: Into<ArcStr>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    button(
        label(text)
            .font(GenericFamily::SystemUi)
            .text_alignment(TextAlign::Center)
            .text_size(13.0)
            .color(TEXT),
        callback,
    )
    .background_color(SURFACE_SUBTLE)
    .border(BORDER, 1.0.px())
    .corner_radius(6.0.px())
    .padding(Padding::from_vh(5.0.px(), 9.0.px()))
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
    button(
        sized_box(
            label(text)
                .font(GenericFamily::SystemUi)
                .text_alignment(TextAlign::Start)
                .color(TEXT),
        )
        .fixed_width(width.px()),
        callback,
    )
    .background_color(SURFACE)
    .border(Color::TRANSPARENT, 1.0.px())
    .corner_radius(6.0.px())
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
    let background = if selected { ACCENT_SUBTLE } else { SURFACE };
    let border = if selected { ACCENT } else { BORDER };
    let text_color = if selected { ACCENT } else { TEXT };

    button(
        label(text)
            .font(GenericFamily::SystemUi)
            .text_alignment(TextAlign::Center)
            .color(text_color),
        callback,
    )
    .background_color(background)
    .border(border, 1.0.px())
    .corner_radius(6.0.px())
}
