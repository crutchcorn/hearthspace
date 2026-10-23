//! Catalog artwork and copy shared by the grid and list presentation.
use super::{Entry, ShellState};
use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_ui_components::icons::{Icon, icon};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        properties::LineBreaking,
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, image, label, sized_box},
};

pub(super) fn view(
    state: &ShellState,
    entry: &Entry,
    selected: bool,
    grid: bool,
) -> Box<AnyWidgetView<ShellState>> {
    let (name, description, artwork) = match entry {
        Entry::App(app) => (
            app.name.clone(),
            app.comment
                .clone()
                .or_else(|| app.generic_name.clone())
                .unwrap_or_default(),
            super::super::dock::app_image(state, app),
        ),
        Entry::Window(window) => (
            window.title.clone(),
            format!("Running · {}", window.app_id),
            super::super::dock::window_image(state, &window.app_id),
        ),
    };
    let artwork: Box<AnyWidgetView<ShellState>> = if let Some(image_data) = artwork {
        image(image_data).decorative(true).boxed()
    } else {
        icon(Icon::AppsGrid, if grid { 30.0 } else { 25.0 })
            .theme_color(if selected { Tone::Strong } else { Tone::Muted })
            .boxed()
    };
    let artwork = sized_box(artwork)
        .fixed_width(if grid { 54.0 } else { 44.0 }.px())
        .fixed_height(if grid { 49.0 } else { 44.0 }.px())
        .padding(if grid { 9.0 } else { 8.0 }.px())
        .theme_background(if selected {
            Tone::AccentSoft
        } else {
            Tone::Control
        })
        .theme_border(if selected {
            Tone::AccentBorder
        } else {
            Tone::ChromeBorder
        })
        .border_width(1.0.px())
        .corner_radius(10.0.px());
    let title = label(name)
        .text_size(if grid { 12.0 } else { 14.0 })
        .text_alignment(if grid {
            xilem::TextAlign::Center
        } else {
            xilem::TextAlign::Start
        })
        .width(Dim::Stretch)
        .line_break_mode(LineBreaking::Clip);
    if grid {
        flex_col((artwork, title))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(7.0.px())
            .boxed()
    } else {
        flex_row((
            artwork,
            flex_col((
                title,
                label(description)
                    .text_size(11.0)
                    .width(Dim::Stretch)
                    .line_break_mode(LineBreaking::Clip)
                    .theme_color(Tone::Muted),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(5.0.px())
            .flex(1.0),
            icon(Icon::ArrowUpRight, 18.0).theme_color(Tone::Muted),
        ))
        .gap(14.0.px())
        .boxed()
    }
}
