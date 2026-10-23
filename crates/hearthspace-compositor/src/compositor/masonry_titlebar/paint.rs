use hearthspace_ui_components::settings_material;
use masonry::{
    imaging::Painter,
    kurbo::{BezPath, Rect, Shape},
    peniko::Gradient,
};
use xilem::Color;

use super::TitlebarState;
use crate::compositor::windows::TitlebarButton;

pub(super) fn action_color(state: &TitlebarState, button: TitlebarButton) -> Color {
    let emphasized = state.hovered == Some(button) || state.pressed == Some(button);
    let color = if emphasized && button == TitlebarButton::Close {
        Color::WHITE
    } else if state.active {
        state.palette().text
    } else {
        state.palette().muted
    };
    color.multiply_alpha(if emphasized { 1.0 } else { 0.8 })
}

pub(super) fn action(
    painter: &mut Painter<'_>,
    rect: Rect,
    state: &TitlebarState,
    button: TitlebarButton,
) {
    if state.hovered != Some(button) && state.pressed != Some(button) {
        return;
    }
    // The reference has no distinct :active rule; a held button keeps its hover
    // material until the pointer leaves its hit rectangle.
    let palette = state.palette();
    settings_material::shadow(
        painter,
        rect,
        10.0,
        palette.control_shadow,
        (0.0, 6.0),
        22.0,
        -5.0,
    );
    let shape = rect.to_rounded_rect(10.0);
    painter
        .fill(
            shape,
            if button == TitlebarButton::Close {
                Color::from_rgb8(0xb8, 0x47, 0x26)
            } else {
                palette.button_hover
            },
        )
        .draw();
    // The two inset components of --control-glow, without adding a border.
    painter.with_fill_clip(shape, |painter| {
        let shifted = shape + masonry::kurbo::Vec2::new(0.0, 1.0);
        let top = BezPath::from_iter(
            rect.path_elements(0.1)
                .chain(shifted.to_path(0.1).reverse_subpaths()),
        );
        painter.fill(top, palette.control_top_light).draw();
        let wash = Gradient::new_radial(rect.center(), 16.0).with_stops([
            (0.0, palette.control_inset_glow.with_alpha(0.0)),
            (1.0, palette.control_inset_glow),
        ]);
        painter.fill(shape, &wash).draw();
    });
}
