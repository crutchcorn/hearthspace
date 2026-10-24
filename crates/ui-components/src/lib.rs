use palette::classes::{self, class};
use xilem::{
    TextAlign, WidgetView,
    masonry::{
        core::ArcStr,
        layout::AsUnit,
        parley::{fontique::GenericFamily, style::FontWeight},
        properties::Padding,
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, label, sized_box},
};

pub mod gesture_pad;
pub use gesture_pad::GesturePad;
mod touch_scroll;
pub use touch_scroll::{TouchScroll, touch_portal};
mod semantic_button;
pub use semantic_button::{
    SemanticButton, SemanticButtonView, semantic_button, semantic_radio_button,
};
mod button_surface;
pub use button_surface::{ButtonSurface, ButtonSurfaceView, button_surface};
mod theme;
pub use theme::*;
mod palette;
pub use palette::Palette;
mod properties;
pub use properties::{
    PropertyStyle, hearthspace_light_properties, hearthspace_opaque_properties,
    hearthspace_properties,
};
#[cfg(test)]
mod focus_tests;
pub mod icons;
pub mod settings_material;
pub use settings_material::{SettingsButton, SettingsSurface, settings_button, settings_surface};

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
    class(
        label(text).font(GenericFamily::SystemUi).text_size(13.0),
        classes::MUTED,
    )
}

pub fn app_background<State: 'static, V: WidgetView<State>>(
    content: V,
) -> impl WidgetView<State> + use<State, V> {
    class(sized_box(content).padding(20.0.px()), classes::BACKGROUND)
}

pub fn sidebar_panel<State: 'static, V: WidgetView<State>>(
    content: V,
) -> impl WidgetView<State> + use<State, V> {
    class(
        sized_box(content)
            .border_width(1.0.px())
            .corner_radius(RADIUS_TILE.px())
            .padding(10.0.px()),
        classes::SIDEBAR,
    )
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
    class(
        sized_box(
            flex_col((section_title(title), content))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(12.0.px()),
        )
        .border_width(1.0.px())
        .corner_radius(RADIUS_TILE.px())
        .padding(16.0.px()),
        classes::SECTION,
    )
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
                    .text_alignment(TextAlign::Start),
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
                .text_alignment(TextAlign::Center),
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
                .text_size(13.0),
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
                    .text_alignment(TextAlign::Start),
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
                .text_alignment(TextAlign::Center),
            callback,
        )
        .corner_radius(RADIUS_CONTROL.px()),
        ButtonSurface::Control,
        selected,
    )
}

#[cfg(test)]
mod window_theme_tests;
