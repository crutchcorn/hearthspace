//! Dispatch every input phase as the native event loop does.
use crate::ui::{state::ShellState, test_support::Ui};
use masonry::core::{
    PointerButton, PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType,
    PointerUpdate, TextEvent, WidgetId,
    keyboard::{Key, NamedKey},
    pointer::PointerId,
};
use xilem::WidgetView;

pub(super) fn key<V: WidgetView<ShellState>>(
    ui: &mut Ui<V>,
    key: NamedKey,
    build: &impl Fn(&ShellState) -> V,
) where
    V::Widget: Sized,
{
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
    ui.dispatch(build);
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(key)));
    ui.dispatch(build);
}

pub(super) fn click<V: WidgetView<ShellState>>(
    ui: &mut Ui<V>,
    id: WidgetId,
    build: &impl Fn(&ShellState) -> V,
) where
    V::Widget: Sized,
{
    ui.harness.mouse_move_to(id);
    ui.dispatch(build);
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.dispatch(build);
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(build);
}

pub(super) fn touch<V: WidgetView<ShellState>>(
    ui: &mut Ui<V>,
    id: WidgetId,
    drag: f64,
    build: &impl Fn(&ShellState) -> V,
) where
    V::Widget: Sized,
{
    // ScrollIntoView updates layout before it updates the accessibility bounds.
    ui.dispatch(build);
    let bounds = ui.harness.access_node(id).unwrap().bounding_box().unwrap();
    let pointer = PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    };
    let start = PointerState {
        position: ((bounds.x0 + bounds.x1) / 2.0, (bounds.y0 + bounds.y1) / 2.0).into(),
        pressure: 0.5,
        ..Default::default()
    };
    let end = PointerState {
        position: (start.position.x, start.position.y - drag).into(),
        ..start.clone()
    };
    ui.harness
        .process_pointer_event(PointerEvent::Down(PointerButtonEvent {
            pointer,
            state: start,
            button: None,
        }));
    ui.dispatch(build);
    if drag != 0.0 {
        ui.harness
            .process_pointer_event(PointerEvent::Move(PointerUpdate {
                pointer,
                current: end.clone(),
                coalesced: vec![],
                predicted: vec![],
            }));
        ui.dispatch(build);
    }
    ui.harness
        .process_pointer_event(PointerEvent::Up(PointerButtonEvent {
            pointer,
            state: end,
            button: None,
        }));
    ui.dispatch(build);
}
