//! Settings-specific card layout; colors and controls come from the shared UI.
use hearthspace_ui_components as components;
use xilem::{
    WidgetView,
    masonry::{layout::AsUnit, parley::style::FontWeight, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, flex_col, label, sized_box},
};

use crate::state::SettingsApp;

mod toggle;
pub(crate) use toggle::toggle;
mod restore_focus;
pub(crate) use restore_focus::{focus_on_mount, restore_focus_after_close};
mod disclosure;
pub(crate) use disclosure::disclosure;
mod photo_fit;
pub(crate) use photo_fit::photo_fit;

pub(crate) fn note(text: impl Into<String>) -> impl WidgetView<SettingsApp> {
    label(text.into())
        .text_size(11.0)
        .color(components::TEXT_MUTED)
        .line_break_mode(LineBreaking::WordWrap)
}

pub(crate) fn section<V: WidgetView<SettingsApp>>(
    title: impl Into<String>,
    content: V,
) -> impl WidgetView<SettingsApp> {
    flex_col((
        label(title.into())
            .text_size(11.0)
            .weight(FontWeight::MEDIUM),
        content,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(12.0.px())
}

pub(crate) fn card<V: WidgetView<SettingsApp>>(content: V) -> impl WidgetView<SettingsApp> {
    sized_box(content)
        .background_color(components::SURFACE_CONTROL)
        .border(components::CHROME_BORDER, 1.0.px())
        .corner_radius(components::RADIUS_TILE.px())
        .padding(14.0.px())
}
