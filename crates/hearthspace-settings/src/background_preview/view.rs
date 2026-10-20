use hearthspace_ui_components as components;
use xilem::{
    Color, WidgetView,
    masonry::{
        imaging::Painter,
        kurbo::{Affine, Rect, Stroke},
        layout::AsUnit,
    },
    view::{canvas, sized_box},
};

use crate::{
    background_thumbnail::{HEIGHT, WIDTH},
    state::SettingsApp,
};

pub(crate) fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let image = state
        .background_preview
        .image_for(state.settings.background_image_path.as_deref());
    sized_box(
        canvas(move |_: &mut SettingsApp, _, scene, size| {
            let mut painter: Painter<'_> = Painter::new(scene);
            let shape = Rect::from_origin_size((0.5, 0.5), (size.width - 1.0, size.height - 1.0))
                .to_rounded_rect(components::RADIUS_CONTROL);
            painter.fill(shape, components::APP_BACKGROUND).draw();
            painter.push_fill_clip(shape);
            if let Some(image) = image.as_ref() {
                painter.draw_image(
                    image,
                    Affine::scale_non_uniform(
                        size.width / f64::from(WIDTH),
                        size.height / f64::from(HEIGHT),
                    ),
                );
            } else {
                components::settings_material::dot_pattern(
                    &mut painter,
                    Rect::from_origin_size((0.0, 0.0), size).inset(-1.0),
                    12.0,
                    1.0,
                    1.2,
                    Color::from_rgba8(0x58, 0x6f, 0x68, 38),
                );
            }
            painter.pop_clip();
            painter
                .stroke(shape, &Stroke::new(1.0), components::BORDER)
                .draw();
            components::settings_material::glass_edge(
                &mut painter,
                Rect::from_origin_size((0.0, 0.0), size),
                components::RADIUS_CONTROL,
                false,
            );
        })
        .alt_text(""),
    )
    .fixed_width(84.0.px())
    .fixed_height(64.0.px())
}
