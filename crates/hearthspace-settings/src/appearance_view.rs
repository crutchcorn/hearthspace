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
        CrossAxisAlignment, MainAxisAlignment, button, canvas, flex_col, flex_item, flex_row,
        sized_box, zstack,
    },
};

use crate::{page_components::section, state::SettingsApp, typography::label};

mod preview;
mod swatch;

pub(crate) fn view(_: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    flex_col((
        section(
            "Color mode",
            flex_row((
                flex_item(mode_card(false), 1.0),
                flex_item(mode_card(true), 1.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
        section(
            "Accent color",
            flex_col((
                flex_row((
                    flex_item(accent_card("Orange", components::ACCENT, true), 1.0),
                    flex_item(
                        accent_card("Rose", Color::from_rgb8(0xb7, 0x4e, 0x69), false),
                        1.0,
                    ),
                    flex_item(
                        accent_card("Violet", Color::from_rgb8(0x79, 0x57, 0xb5), false),
                        1.0,
                    ),
                ))
                .gap(8.0.px()),
                flex_row((
                    flex_item(
                        accent_card("Blue", Color::from_rgb8(0x36, 0x72, 0xb5), false),
                        1.0,
                    ),
                    flex_item(
                        accent_card("Teal", Color::from_rgb8(0x23, 0x7e, 0x78), false),
                        1.0,
                    ),
                    flex_item(
                        accent_card("Olive", Color::from_rgb8(0x6f, 0x7b, 0x38), false),
                        1.0,
                    ),
                ))
                .gap(8.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(25.0.px())
    .boxed()
}

fn mode_card(dark: bool) -> impl WidgetView<SettingsApp> {
    components::settings_button(
        button(
            flex_col((
                sized_box(preview::theme(dark)).fixed_height(74.0.px()),
                flex_row((
                    icon(if dark { Icon::Moon } else { Icon::Sun }, 18.0).color(if dark {
                        components::settings_material::MUTED
                    } else {
                        components::settings_material::ACCENT_DECORATIVE
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
                            .background_color(if dark {
                                Color::TRANSPARENT
                            } else {
                                components::ACCENT
                            })
                            .border(
                                if dark {
                                    components::BORDER
                                } else {
                                    components::ACCENT
                                },
                                1.0.px(),
                            )
                            .corner_radius(8.0.px()),
                        (!dark).then(|| {
                            icon(Icon::Check, 11.0)
                                .color(components::ACCENT_INK)
                                .label("Current")
                        }),
                    )))
                    .fixed_width(16.0.px())
                    .fixed_height(16.0.px()),
                ))
                .gap(9.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
            |_: &mut SettingsApp| {},
        )
        .disabled(dark)
        .corner_radius(components::RADIUS_TILE.px())
        .padding(12.0.px()),
        components::SettingsButton::Card,
        !dark,
    )
}

fn accent_card(name: &'static str, color: Color, selected: bool) -> impl WidgetView<SettingsApp> {
    components::settings_button(
        button(
            sized_box(
                flex_row((
                    zstack((
                        sized_box(swatch::Swatch(color))
                            .fixed_width(29.0.px())
                            .fixed_height(29.0.px()),
                        selected.then(|| {
                            icon(Icon::Check, 17.0)
                                .color(components::ACCENT_INK)
                                .label("Current")
                        }),
                    )),
                    flex_item(
                        flex_col((
                            label(name).text_size(11.0).weight(FontWeight::MEDIUM),
                            selected.then(|| {
                                label("Default")
                                    .text_size(9.0)
                                    .color(components::settings_material::MUTED)
                            }),
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
            |_: &mut SettingsApp| {},
        )
        .disabled(!selected)
        .corner_radius(components::RADIUS_TILE.px())
        .padding(10.0.px()),
        components::SettingsButton::Card,
        selected,
    )
}
