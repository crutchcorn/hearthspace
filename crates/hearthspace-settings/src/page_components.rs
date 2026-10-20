//! Settings-specific card layout; colors and controls come from the shared UI.
use hearthspace_ui_components as components;
use xilem::{
    WidgetView,
    masonry::{
        layout::AsUnit,
        parley::style::{FontWeight, LineHeight},
        properties::LineBreaking,
    },
    style::Style,
    view::{CrossAxisAlignment, flex_col},
};

use crate::{state::SettingsApp, typography::label};

mod toggle;
pub(crate) use toggle::toggle;
mod toggle_row;
pub(crate) use toggle_row::toggle_row;
mod restore_focus;
pub(crate) use restore_focus::restore_focus_after_close;
mod disclosure;
pub(crate) use disclosure::disclosure;
mod photo_fit;
pub(crate) use photo_fit::photo_fit;
mod select;
pub(crate) use select::select;
mod progress;
pub(crate) use progress::progress;

pub(crate) fn note(text: impl Into<String>) -> impl WidgetView<SettingsApp> {
    label(text.into())
        .text_size(11.0)
        .line_height(LineHeight::FontSizeRelative(1.5))
        .color(components::settings_material::MUTED)
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
