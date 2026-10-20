//! Decorative theme samples mirror the experiment's miniature glass windows.
use super::*;
use xilem::masonry::{
    imaging::{BlurredRoundedRect, ClipRef, Composite},
    kurbo::Affine,
};

pub(super) fn theme(palette: components::Palette, dark: bool) -> impl WidgetView<SettingsApp> {
    let palette = components::Palette::new(hearthspace_ipc::Appearance {
        color_mode: if dark {
            ColorMode::Dark
        } else {
            ColorMode::Light
        },
        ..palette.appearance
    });
    canvas(move |_: &mut SettingsApp, _, scene, size| {
        let mut painter: Painter<'_> = Painter::new(scene);
        let field = if dark {
            Color::from_rgb8(0x14, 0x22, 0x25)
        } else {
            palette.app_background
        };
        let dot = if dark {
            Color::from_rgba8(0xd5, 0xf1, 0xe5, 0x1c)
        } else {
            Color::from_rgba8(0x58, 0x6f, 0x68, 0x26)
        };
        let bounds = Rect::from_origin_size((0.0, 0.0), size).inset(-0.5);
        let frame = bounds.to_rounded_rect(7.0);
        painter.push_clip(ClipRef::fill(frame));
        painter.fill(frame, field).draw();
        components::settings_material::dot_pattern(
            &mut painter,
            Rect::from_origin_size((0.0, 0.0), size).inset(-1.0),
            9.0,
            0.6,
            0.8,
            dot,
        );
        let width = size.width - 2.0;
        let main = Rect::new(
            1.0 + width * 0.13,
            13.0,
            1.0 + width * 0.69,
            size.height - 13.0,
        );
        let panel = Rect::new(
            1.0 + width * 0.57,
            31.0,
            1.0 + width * 0.88,
            size.height - 6.0,
        );
        window(&mut painter, main, dark, field);
        let ink = if dark {
            Color::from_rgba8(0xd9, 0xe7, 0xe9, 0x75)
        } else {
            Color::from_rgba8(0x59, 0x66, 0x68, 0x42)
        };
        for (top, width, alpha) in [(12.0, 0.42, 1.0), (21.0, 0.63, 0.45)] {
            painter
                .fill(
                    Rect::new(
                        main.x0 + 10.0,
                        main.y0 + top,
                        main.x0 + 10.0 + (main.width() - 2.0) * width,
                        main.y0 + top + 3.0,
                    )
                    .to_rounded_rect(2.0),
                    ink.multiply_alpha(alpha),
                )
                .draw();
        }
        window(&mut painter, panel, dark, field);
        painter.pop_clip();
        painter
            .stroke(
                frame,
                &Stroke::new(1.0),
                Color::from_rgba8(255, 255, 255, if dark { 0x12 } else { 0x40 }),
            )
            .draw();
    })
    .alt_text("")
}

fn window(painter: &mut Painter<'_>, rect: Rect, dark: bool, field: Color) {
    let shape = rect.inset(-0.5).to_rounded_rect(6.0);
    let shadow = if dark {
        Color::from_rgba8(0x11, 0x1b, 0x27, 0x26)
    } else {
        Color::from_rgba8(0x40, 0x58, 0x5a, 0x1a)
    };
    painter.blurred_rounded_rect(BlurredRoundedRect {
        transform: Affine::translate((0.0, 5.0)),
        rect,
        color: shadow,
        radius: 6.0,
        std_dev: 8.0,
        composite: Composite::default(),
    });
    // The only backdrop in this illustrative sample is the dotted field.
    // Its blur resolves to the field's solid color at this tiny scale.
    painter.fill(shape, field).draw();
    painter
        .fill(
            shape,
            if dark {
                Color::from_rgba8(0x20, 0x28, 0x2d, 0x91)
            } else {
                Color::from_rgba8(255, 255, 255, 0x99)
            },
        )
        .draw();
    let edge = Color::from_rgba8(255, 255, 255, if dark { 0x29 } else { 0xa6 });
    painter.stroke(shape, &Stroke::new(1.0), edge).draw();
    painter.push_clip(ClipRef::fill(shape));
    painter
        .stroke(
            (rect + xilem::masonry::kurbo::Vec2::new(0.0, 1.0)).to_rounded_rect(6.0),
            &Stroke::new(1.0),
            Color::from_rgba8(255, 255, 255, if dark { 0x1f } else { 0xa6 }),
        )
        .draw();
    painter.pop_clip();
}
