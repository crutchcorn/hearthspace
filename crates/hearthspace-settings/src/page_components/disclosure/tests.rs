use super::*;
use hearthspace_ui_components as components;
use xilem::masonry::{
    core::{
        NewWidget, TextEvent,
        keyboard::{Key, NamedKey},
    },
    testing::TestHarness,
    widgets::Label,
};

#[test]
fn disclosure_paints_orange_focus_inside_its_bounds_and_keeps_native_activation() {
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        NewWidget::new(DisclosureButton::new(
            "Photo fit".into(),
            true,
            NewWidget::new(Label::new("Fit")),
        )),
        (160, 44),
    );
    let id = harness.root_id();
    let idle = harness.render();
    harness.press_tab_key(false);
    assert_eq!(harness.focused_widget_id(), Some(id));
    assert_eq!(
        harness.access_node(id).unwrap().data().is_expanded(),
        Some(true)
    );
    let focused = harness.render();
    let [r, g, b, _] = components::FOCUS_RING.to_rgba8().to_u8_array();
    assert!(
        idle.pixels()
            .zip(focused.pixels())
            .any(|(before, after)| { before != after && after.0[..3] == [r, g, b] })
    );
    harness.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    assert!(harness.pop_action::<ButtonPress>().is_some());
    harness.mouse_click_on(id, Some(PointerButton::Primary));
    assert!(harness.pop_action::<ButtonPress>().is_some());
    assert_eq!(harness.focused_widget_id(), Some(id));
}
