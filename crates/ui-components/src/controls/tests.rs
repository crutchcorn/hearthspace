use super::*;
use xilem::masonry::{
    accesskit::{Role, Toggled},
    core::{
        NewWidget, TextEvent,
        keyboard::{Key, NamedKey},
    },
    testing::TestHarness,
    widgets::{SliderMoved, SwitchToggled},
};

fn choice(kind: ControlKind, role: Role) {
    let mut h = TestHarness::create_with_size(
        crate::hearthspace_opaque_properties(),
        NewWidget::new(ControlWidget::<SwitchToggled>::new(kind, "Bluetooth", 1.)),
        (40, 24),
    );
    let id = h.root_id();
    assert_eq!(h.access_node(id).unwrap().role(), role);
    assert_eq!(h.access_node(id).unwrap().toggled(), Some(Toggled::True));
    h.press_tab_key(false);
    let before = h.render();
    h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
    h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    assert_eq!(
        h.pop_action::<SwitchToggled>(),
        Some((SwitchToggled(false), id))
    );
    h.edit_root_widget(|mut w| w.ctx.set_disabled(true));
    h.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
    h.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    assert!(h.pop_action_erased().is_none());
    assert_ne!(h.render(), before);
}
#[test]
fn shared_switch_keeps_native_keyboard_action_and_disabled_semantics() {
    choice(ControlKind::Switch, Role::Switch);
}
#[test]
fn shared_checkbox_exposes_checkbox_semantics_and_keyboard_action() {
    choice(ControlKind::Checkbox, Role::CheckBox);
}
#[test]
fn meter_clamps_values_and_progress_is_not_interactive() {
    let h = TestHarness::create_with_size(
        crate::hearthspace_opaque_properties(),
        NewWidget::new(ControlWidget::<SwitchToggled>::new(
            ControlKind::Meter,
            "Battery charge",
            150.,
        )),
        (256, 8),
    );
    assert_eq!(
        h.access_node(h.root_id()).unwrap().numeric_value(),
        Some(100.)
    );
    let mut h = TestHarness::create_with_size(
        crate::hearthspace_opaque_properties(),
        NewWidget::new(ControlWidget::<SwitchToggled>::new(
            ControlKind::Progress,
            "Model download",
            f64::NAN,
        )),
        (256, 8),
    );
    assert_eq!(
        h.access_node(h.root_id()).unwrap().role(),
        Role::ProgressIndicator
    );
    assert_eq!(
        h.access_node(h.root_id()).unwrap().numeric_value(),
        Some(0.)
    );
    h.press_tab_key(false);
    assert_eq!(h.focused_widget_id(), None);
}
#[test]
fn slider_uses_native_steps_and_endpoints() {
    let mut h = TestHarness::create_with_size(
        crate::hearthspace_opaque_properties(),
        NewWidget::new(ControlWidget::<SliderMoved>::new(
            ControlKind::Slider,
            "Sound",
            65.,
        )),
        (236, 32),
    );
    let id = h.root_id();
    h.press_tab_key(false);
    h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
    assert_eq!(
        h.pop_action::<SliderMoved>(),
        Some((SliderMoved { value: 66. }, id))
    );
    h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::End)));
    assert_eq!(
        h.pop_action::<SliderMoved>(),
        Some((SliderMoved { value: 100. }, id))
    );
    h.edit_root_widget(|mut w| w.ctx.set_disabled(true));
    h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Home)));
    assert!(h.pop_action_erased().is_none());
}

#[test]
fn disabling_a_retained_slider_repaints_its_unchanged_value() {
    let mut h = TestHarness::create_with_size(
        crate::hearthspace_opaque_properties(),
        NewWidget::new(ControlWidget::<SliderMoved>::new(
            ControlKind::Slider,
            "Sound",
            65.,
        )),
        (236, 32),
    );
    let enabled = h.render();
    h.edit_root_widget(|mut w| w.ctx.set_disabled(true));
    assert_ne!(
        enabled,
        h.render(),
        "disabled opacity must not reuse the enabled scene"
    );
    h.edit_root_widget(|mut w| w.ctx.set_disabled(false));
    assert_eq!(enabled, h.render());
}
