//! Hover must fit the entire caption and its trailing padding inside the native surface.
use super::*;

#[test]
fn notification_caption_keeps_its_padding_and_hover_border_inside_the_surface() {
    for vertical in [false, true] {
        let state = status_state("notifications", vertical);
        let size = surface_size(&state);
        let mut ui = Ui::new(state, view, size);
        let button = control(&ui);
        ui.harness.mouse_move_to(button);
        ui.dispatch(&view);
        caption(&ui, vertical, Some("All clear"));
        let label = ids(&ui.harness)
            .into_iter()
            .find(|id| {
                ui.harness
                    .get_widget_with_id(*id)
                    .downcast::<Label>()
                    .is_some()
            })
            .unwrap();
        let text = bounds(&ui, label);
        let rect = bounds(&ui, button);
        let trailing = if vertical {
            rect.y1 - text.y1
        } else {
            rect.x1 - text.x1
        };
        let expected = 11.0;
        assert!(
            trailing >= expected - 0.01,
            "caption needs {expected}px trailing room; got {trailing}px"
        );
    }
}
