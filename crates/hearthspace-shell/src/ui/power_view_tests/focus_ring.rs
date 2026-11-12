//! Both buttons need their complete painted ring inside the scroll viewport.
use super::*;

#[test]
fn confirmation_preserves_all_four_sides_of_cancel_and_logoff_focus_rings() {
    let mut state = state();
    state.outputs[0].power_confirmation = Some(Confirmation::LogOff);
    let mut ui = Ui::new(state, view, (420, 230));
    for name in ["Cancel", "Log off"] {
        let id = ui.button(name);
        assert_eq!(ui.harness.focused_widget_id(), Some(id));
        key(&mut ui, NamedKey::ArrowLeft, &view);
        let widget = ui.harness.get_widget_with_id(id);
        let ctx = widget.ctx();
        let b = ctx.window_transform().transform_rect_bbox(ctx.border_box());
        let image = ui.harness.render();
        let rgb = ui
            .state
            .appearance
            .palette
            .focus_ring
            .to_rgba8()
            .to_u8_array();
        let sides = [
            (
                "top",
                masonry::kurbo::Rect::new(b.x0 + 8., b.y0 - 4., b.x1 - 8., b.y0),
            ),
            (
                "bottom",
                masonry::kurbo::Rect::new(b.x0 + 8., b.y1, b.x1 - 8., b.y1 + 4.),
            ),
            (
                "left",
                masonry::kurbo::Rect::new(b.x0 - 4., b.y0 + 8., b.x0, b.y1 - 8.),
            ),
            (
                "right",
                masonry::kurbo::Rect::new(b.x1, b.y0 + 8., b.x1 + 4., b.y1 - 8.),
            ),
        ];
        for (side, region) in sides {
            assert!(
                image.enumerate_pixels().any(|(x, y, p)| {
                    region.contains((f64::from(x), f64::from(y))) && p.0[..3] == rgb[..3]
                }),
                "{name} focus ring is clipped on the {side}: {b:?}"
            );
        }
        ui.harness.press_tab_key(false);
        ui.dispatch(&view);
    }
    assert!(ui.state.outputs[0].power_confirmation.is_some());
}
