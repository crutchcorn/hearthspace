use hearthspace_ipc::{AccentColor, ColorMode};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, Color, WidgetView,
    masonry::{
        imaging::Painter,
        kurbo::{Rect, Stroke},
        layout::AsUnit,
        parley::style::FontWeight,
    },
    style::Style,
    view::{
        CrossAxisAlignment, MainAxisAlignment, canvas, flex_col, flex_item, flex_row, sized_box,
        zstack,
    },
};

use crate::{
    page_components::{disclosure, section},
    state::SettingsApp,
    typography::label,
};

mod preview;
mod swatch;

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let appearance = state.settings.appearance;
    let rows = AccentColor::ALL
        .chunks(3)
        .map(|colors| {
            flex_row(
                colors
                    .iter()
                    .map(|&color| {
                        flex_item(
                            accent_card(palette, color, color == appearance.accent_color),
                            1.0,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .gap(8.0.px())
        })
        .collect::<Vec<_>>();
    flex_col((
        section(
            "Color mode",
            flex_row(ColorMode::ALL.map(|mode| {
                flex_item(mode_card(palette, mode, mode == appearance.color_mode), 1.0)
            }))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
        section(
            "Accent color",
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(8.0.px()),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(25.0.px())
    .boxed()
}

fn mode_card(
    palette: components::Palette,
    mode: ColorMode,
    selected: bool,
) -> impl WidgetView<SettingsApp> {
    let dark = mode == ColorMode::Dark;
    components::settings_button(
        disclosure(
            if dark { "Dark" } else { "Light" },
            false,
            false,
            flex_col((
                sized_box(preview::theme(palette, dark)).fixed_height(74.0.px()),
                flex_row((
                    icon(if dark { Icon::Moon } else { Icon::Sun }, 18.0).color(if selected {
                        palette.accent_decorative
                    } else {
                        palette.muted
                    }),
                    flex_item(
                        label(if dark { "Dark" } else { "Light" })
                            .text_size(12.0)
                            .weight(FontWeight::MEDIUM),
                        1.0,
                    ),
                    sized_box(zstack((
                        sized_box(label(""))
                            .fixed_width(16.0.px())
                            .fixed_height(16.0.px())
                            .background_color(if selected {
                                palette.accent
                            } else {
                                Color::TRANSPARENT
                            })
                            .border(
                                if selected {
                                    palette.accent
                                } else {
                                    palette.border
                                },
                                1.0.px(),
                            )
                            .corner_radius(8.0.px()),
                        selected.then(|| icon(Icon::Check, 11.0).color(palette.accent_ink)),
                    )))
                    .fixed_width(16.0.px())
                    .fixed_height(16.0.px()),
                ))
                .gap(9.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
            move |state: &mut SettingsApp| crate::appearance::set_mode(state, mode),
        )
        .selected(selected)
        .corner_radius(components::RADIUS_TILE.px())
        .padding(12.0.px()),
        components::SettingsButton::Card,
        selected,
    )
}

fn accent_card(
    palette: components::Palette,
    accent: AccentColor,
    selected: bool,
) -> impl WidgetView<SettingsApp> {
    let [red, green, blue] = accent.rgb();
    components::settings_button(
        disclosure(
            accent.name(),
            false,
            false,
            sized_box(
                flex_row((
                    zstack((
                        sized_box(swatch::Swatch(Color::from_rgb8(red, green, blue)))
                            .fixed_width(29.0.px())
                            .fixed_height(29.0.px()),
                        selected.then(|| {
                            icon(Icon::Check, 17.0).color(Color::from_rgb8(255, 250, 245))
                        }),
                    )),
                    flex_item(
                        flex_col((
                            label(accent.name())
                                .text_size(11.0)
                                .weight(FontWeight::MEDIUM),
                            (accent == AccentColor::Orange)
                                .then(|| label("Default").text_size(9.0).color(palette.muted)),
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .gap(3.0.px()),
                        1.0,
                    ),
                ))
                .gap(9.0.px()),
            )
            .fixed_height(46.0.px()),
            move |state: &mut SettingsApp| crate::appearance::set_accent(state, accent),
        )
        .selected(selected)
        .corner_radius(components::RADIUS_TILE.px())
        .padding(10.0.px()),
        components::SettingsButton::Card,
        selected,
    )
}
