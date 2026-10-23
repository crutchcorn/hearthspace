use super::*;
use xilem::masonry::{
    accesskit::{Action, ActionRequest, NodeId, TreeId},
    core::{Modifiers, PointerButton, WindowEvent},
};

fn node<V: WidgetView<SettingsApp, Widget: Sized>>(
    ui: &Ui<V>,
    role: Role,
    name: &str,
) -> Option<NodeId> {
    let mut pending = vec![ui.harness.access_tree().state().root()];
    while let Some(node) = pending.pop() {
        pending.extend(node.children());
        if node.role() == role && node.label().as_deref() == Some(name) {
            // Consumer's public u128 representation stores the local id in
            // its upper half; this harness has only the root AccessKit tree.
            return Some(NodeId((u128::from(node.id()) >> 64) as u64));
        }
    }
    None
}

pub(super) fn button<V: WidgetView<SettingsApp, Widget: Sized>>(ui: &Ui<V>, name: &str) -> NodeId {
    node(ui, Role::Button, name).unwrap_or_else(|| panic!("Missing modal action: {name}"))
}

fn action<V: WidgetView<SettingsApp, Widget: Sized>>(ui: &mut Ui<V>, name: &str, action: Action) {
    ui.harness.process_access_event(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: button(ui, name),
        data: None,
    });
}

pub(super) fn focus<V: WidgetView<SettingsApp, Widget: Sized>>(ui: &mut Ui<V>, name: &str) {
    action(ui, name, Action::Focus);
}

pub(super) fn activate<V: WidgetView<SettingsApp, Widget: Sized>>(ui: &mut Ui<V>, name: &str) {
    action(ui, name, Action::Click);
}

fn outline<V: WidgetView<SettingsApp, Widget: Sized>>(
    ui: &mut Ui<V>,
    name: &str,
    rgb: [u8; 3],
) -> bool {
    let id = button(ui, name);
    let mut pending = vec![ui.harness.access_tree().state().root()];
    let bounds = loop {
        let node = pending.pop().unwrap();
        pending.extend(node.children());
        if NodeId((u128::from(node.id()) >> 64) as u64) == id {
            break node.bounding_box().unwrap();
        }
    };
    ui.harness.render().enumerate_pixels().any(|(x, y, pixel)| {
        let (x, y) = (f64::from(x), f64::from(y));
        let outside = x < bounds.x0 || x > bounds.x1 || y < bounds.y0 || y > bounds.y1;
        outside
            && x >= bounds.x0 - 4.0
            && x <= bounds.x1 + 4.0
            && y >= bounds.y0 - 4.0
            && y <= bounds.y1 + 4.0
            && pixel.0[..3] == rgb
    })
}

#[test]
fn confirmation_is_centered_modal_traps_tab_and_escape_keeps_the_model() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.ai.sender = Some(sender);
    state.ai.receive(Response::Finished {
        operation: Operation::Refresh,
        snapshot: Some(ready()),
        error: None,
    });
    let mut ui = Ui::new(state, view, (680, 490));
    let toggle = switch(&ui);
    ui.harness.focus_on(Some(toggle));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    let dialog_id = node(&ui, Role::Dialog, "Turn off AI?").unwrap();
    let mut pending = vec![ui.harness.access_tree().state().root()];
    let (bounds, modal) = loop {
        let node = pending.pop().expect("modal dialog in access tree");
        pending.extend(node.children());
        if NodeId((u128::from(node.id()) >> 64) as u64) == dialog_id {
            break (node.bounding_box().unwrap(), node.data().is_modal());
        }
    };
    assert!(modal);
    assert!((bounds.x1 - bounds.x0 - 380.0).abs() < 1.0);
    assert!(((bounds.x1 + bounds.x0) / 2.0 - 340.0).abs() < 1.0);
    assert!(((bounds.y1 + bounds.y0) / 2.0 - 245.0).abs() < 1.0);
    assert!(outline(&mut ui, "Keep AI", [0xe9, 0x64, 0x2a]));
    for name in ["Turn off and delete", "Keep AI", "Turn off and delete"] {
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        assert_eq!(ui.harness.focused_widget_id().unwrap(), button(&ui, name));
    }
    let TextEvent::Keyboard(mut backwards_tab) = TextEvent::key_down(Key::Named(NamedKey::Tab))
    else {
        unreachable!()
    };
    backwards_tab.modifiers = Modifiers::SHIFT;
    ui.harness
        .process_text_event(TextEvent::Keyboard(backwards_tab));
    assert_eq!(
        ui.harness.focused_widget_id().unwrap(),
        button(&ui, "Keep AI")
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
    ui.dispatch(&view);
    assert!(node(&ui, Role::Dialog, "Turn off AI?").is_none());
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(requests.try_recv().is_err());
    assert!(ui.state.ai.enabled());
}

#[test]
fn backdrop_dismisses_without_activating_page_and_modal_is_removed_with_category() {
    fn page(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
        sized_box(if state.selected_category == SettingsCategory::Ai {
            crate::ai_view::view(state)
        } else {
            xilem::view::label("Another Settings page").boxed()
        })
    }
    let (_dir, mut state) = state();
    state.ai.snapshot = Some(ready());
    let mut ui = Ui::new(state, page, (360, 420));
    let toggle = switch(&ui);
    ui.harness
        .mouse_click_on(toggle, Some(PointerButton::Primary));
    ui.dispatch(&page);
    assert!(node(&ui, Role::Dialog, "Turn off AI?").is_some());
    assert_eq!(
        ui.harness.focused_widget_id().unwrap(),
        button(&ui, "Keep AI")
    );
    assert!(!outline(&mut ui, "Keep AI", [0xe9, 0x64, 0x2a]));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
    assert!(outline(&mut ui, "Turn off and delete", [0xbc, 0x3b, 0x39]));
    ui.harness.mouse_move((4.0, 4.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&page);
    assert!(!ui.state.ai.confirm_off);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(ui.state.ai.enabled());
    ui.state.ai.confirm_off = true;
    ui.rebuild(&page);
    ui.dispatch(&page);
    ui.harness
        .process_window_event(WindowEvent::Resize((440, 580).into()));
    assert!(node(&ui, Role::Dialog, "Turn off AI?").is_some());
    ui.state.select_category(SettingsCategory::System);
    ui.rebuild(&page);
    ui.dispatch(&page);
    assert!(node(&ui, Role::Dialog, "Turn off AI?").is_none());
}
