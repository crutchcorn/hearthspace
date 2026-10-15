use super::tray_widget::Appearance;
use crate::tray::TrayToggle;
use hearthspace_ui_components as components;
use xilem::masonry::{
    core::PaintCtx,
    imaging::Painter,
    kurbo::{Affine, Circle, Line, RoundedRect, Stroke},
};
pub(super) fn paint(appearance: &Appearance, ctx: &mut PaintCtx<'_>, painter: &mut Painter<'_>) {
    let bounds = ctx.content_box();
    if appearance.separator {
        painter.fill(bounds, components::BORDER).draw();
        return;
    }
    let selected = appearance.selected || ctx.is_focus_target() || ctx.is_hovered();
    if selected {
        painter
            .fill(
                RoundedRect::from_rect(bounds, 6.0),
                components::ACCENT_SUBTLE,
            )
            .draw();
    }
    if ctx.is_focus_target() {
        painter
            .stroke(
                RoundedRect::from_rect(bounds.inset(-1.0), 6.0),
                &Stroke::new(1.0),
                components::ACCENT,
            )
            .draw();
    }
    let color = if appearance.enabled {
        components::TEXT
    } else {
        components::TEXT_MUTED
    };
    if appearance.icon {
        if let Some(image) = &appearance.image {
            let scale = 20.0 / (image.image.width.max(image.image.height).max(1) as f64);
            let translation = (
                (bounds.width() - image.image.width as f64 * scale) / 2.0,
                (bounds.height() - image.image.height as f64 * scale) / 2.0,
            );
            painter.draw_image(image, Affine::translate(translation) * Affine::scale(scale));
        } else {
            for x in [11.0, 23.0] {
                for y in [11.0, 23.0] {
                    painter.fill(Circle::new((x, y), 3.0), color).draw();
                }
            }
        }
        if appearance.attention {
            painter
                .fill(Circle::new((28.0, 6.0), 3.0), components::ACCENT)
                .draw();
        }
    } else {
        let stroke = Stroke::new(1.7);
        match appearance.toggle {
            Some(TrayToggle::Check(true)) => {
                painter
                    .stroke(Line::new((6.0, 17.0), (10.0, 21.0)), &stroke, color)
                    .draw();
                painter
                    .stroke(Line::new((10.0, 21.0), (18.0, 12.0)), &stroke, color)
                    .draw();
            }
            Some(TrayToggle::Radio(true)) => {
                painter.fill(Circle::new((12.0, 17.0), 4.0), color).draw();
            }
            Some(TrayToggle::Indeterminate) => {
                painter
                    .stroke(Line::new((7.0, 17.0), (17.0, 17.0)), &stroke, color)
                    .draw();
            }
            _ => {}
        }
        if appearance.submenu {
            let x = bounds.width() - 12.0;
            painter
                .stroke(Line::new((x - 3.0, 13.0), (x + 1.0, 17.0)), &stroke, color)
                .draw();
            painter
                .stroke(Line::new((x + 1.0, 17.0), (x - 3.0, 21.0)), &stroke, color)
                .draw();
        }
    }
}
