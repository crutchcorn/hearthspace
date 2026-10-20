use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, Color, WidgetView,
    masonry::{
        imaging::Painter,
        kurbo::{Circle, Rect, RoundedRect, Stroke},
        layout::AsUnit,
    },
    style::Style,
    view::{
        CrossAxisAlignment, button, canvas, flex_col, flex_item, flex_row, label, sized_box, zstack,
    },
};

use crate::{page_components::section, state::SettingsApp};

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
    .gap(24.0.px())
    .boxed()
}

fn mode_card(dark: bool) -> impl WidgetView<SettingsApp> {
    components::button_surface(
        button(
            flex_col((
                sized_box(theme_preview(dark)).fixed_height(74.0.px()),
                flex_row((
                    icon(if dark { Icon::Moon } else { Icon::Sun }, 17.0).color(if dark {
                        components::TEXT_MUTED
                    } else {
                        components::ACCENT
                    }),
                    flex_item(
                        label(if dark { "Dark" } else { "Light" }).text_size(12.0),
                        1.0,
                    ),
                    label(if dark { "Unavailable" } else { "Current" })
                        .text_size(10.0)
                        .color(components::TEXT_MUTED),
                    (!dark).then(|| {
                        sized_box(icon(Icon::Check, 11.0).color(components::ACCENT_INK))
                            .fixed_width(16.0.px())
                            .fixed_height(16.0.px())
                            .background_color(components::ACCENT)
                            .corner_radius(8.0.px())
                    }),
                ))
                .gap(7.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
            |_: &mut SettingsApp| {},
        )
        .disabled(dark),
        components::ButtonSurface::Control,
        !dark,
    )
    .corner_radius(components::RADIUS_TILE.px())
    .padding(12.0.px())
}

fn accent_card(name: &'static str, color: Color, selected: bool) -> impl WidgetView<SettingsApp> {
    let swatch = canvas(move |_: &mut SettingsApp, _, scene, _| {
        let mut painter = Painter::new(scene);
        painter.fill(Circle::new((15.0, 15.0), 14.5), color).draw();
    })
    .alt_text("");
    components::button_surface(
        button(
            sized_box(
                flex_row((
                    zstack((
                        sized_box(swatch)
                            .fixed_width(30.0.px())
                            .fixed_height(30.0.px()),
                        selected.then(|| icon(Icon::Check, 17.0).color(components::ACCENT_INK)),
                    )),
                    flex_item(
                        flex_col((
                            label(name).text_size(12.0),
                            label(if selected { "Default" } else { "Unavailable" })
                                .text_size(10.0)
                                .color(components::TEXT_MUTED),
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .gap(3.0.px()),
                        1.0,
                    ),
                ))
                .gap(9.0.px()),
            )
            .fixed_height(46.0.px()),
            |_: &mut SettingsApp| {},
        )
        .disabled(!selected),
        components::ButtonSurface::Control,
        selected,
    )
    .corner_radius(components::RADIUS_TILE.px())
    .padding(10.0.px())
}

fn theme_preview(dark: bool) -> impl WidgetView<SettingsApp> {
    canvas(move |_: &mut SettingsApp, _, scene, size| {
        let mut painter = Painter::new(scene);
        let field = if dark {
            Color::from_rgb8(0x14, 0x22, 0x25)
        } else {
            components::APP_BACKGROUND
        };
        let window = if dark {
            Color::from_rgb8(0x20, 0x2b, 0x2e)
        } else {
            components::SURFACE
        };
        let detail = if dark {
            Color::from_rgba8(0xa6, 0xb6, 0xb8, 0x55)
        } else {
            Color::from_rgba8(0x58, 0x6f, 0x68, 0x40)
        };
        let bounds = Rect::from_origin_size((0.0, 0.0), size);
        painter.fill(bounds.to_rounded_rect(7.0), field).draw();
        for x in (6..size.width.max(0.0) as i32).step_by(8) {
            for y in (6..size.height.max(0.0) as i32).step_by(8) {
                painter
                    .fill(Circle::new((f64::from(x), f64::from(y)), 0.45), detail)
                    .draw();
            }
        }
        let main = RoundedRect::new(size.width * 0.13, 13.0, size.width * 0.69, 61.0, 6.0);
        painter.fill(main, window).draw();
        painter.stroke(main, &Stroke::new(1.0), detail).draw();
        for (y, width) in [(25.0, 0.22), (34.0, 0.33)] {
            painter
                .fill(
                    Rect::new(size.width * 0.19, y, size.width * (0.19 + width), y + 3.0)
                        .to_rounded_rect(1.5),
                    detail,
                )
                .draw();
        }
        let panel = RoundedRect::new(size.width * 0.56, 32.0, size.width * 0.87, 69.0, 6.0);
        painter.fill(panel, window).draw();
        painter.stroke(panel, &Stroke::new(1.0), detail).draw();
    })
    .alt_text("")
}
