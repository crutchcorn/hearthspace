//! Keyboard activation belongs to the button that received the original press.
use super::SemanticButton;
use xilem::masonry::{
    core::{
        TextEvent, Widget,
        keyboard::{Code, Key, NamedKey},
    },
    testing::TestHarness,
    widgets::{Button, ButtonPress, Flex, Label},
};

fn semantic() -> SemanticButton {
    SemanticButton::new(Label::new("Activate").prepare(), "Activate", None, false)
}

fn repeat_down(key: Key) -> TextEvent {
    let TextEvent::Keyboard(mut event) = TextEvent::key_down(key) else {
        unreachable!()
    };
    event.repeat = true;
    TextEvent::Keyboard(event)
}

fn focus_transfer_cancels_press(first: impl Widget, second: impl Widget) {
    let first = first.prepare();
    let first_id = first.id();
    let second = second.prepare();
    let second_id = second.id();
    let mut harness = TestHarness::create(
        crate::hearthspace_light_properties(),
        Flex::row().with_fixed(first).with_fixed(second).prepare(),
    );
    let enter = Key::Named(NamedKey::Enter);
    harness.focus_on(Some(first_id));
    harness.process_text_event(TextEvent::key_down(enter.clone()));
    harness.focus_on(Some(second_id));
    harness.process_text_event(repeat_down(enter.clone()));
    harness.process_text_event(TextEvent::key_up(enter.clone()));
    assert!(
        harness.pop_action_erased().is_none(),
        "Focus and repeats cannot transfer a press"
    );

    harness.focus_on(Some(first_id));
    harness.process_text_event(TextEvent::key_up(enter.clone()));
    assert!(
        harness.pop_action_erased().is_none(),
        "Returning focus cannot revive the old press"
    );
    harness.process_text_event(TextEvent::key_down(enter.clone()));
    harness.process_text_event(repeat_down(enter.clone()));
    harness.process_text_event(TextEvent::key_up(enter.clone()));
    assert_eq!(
        harness.pop_action::<ButtonPress>(),
        Some((ButtonPress { button: None }, first_id))
    );
    harness.process_text_event(TextEvent::key_up(enter));
    assert!(
        harness.pop_action_erased().is_none(),
        "A press can activate only once"
    );
}

#[test]
fn native_button_focus_transfer_cancels_keyboard_activation() {
    focus_transfer_cancels_press(Button::with_text("First"), Button::with_text("Second"));
}

#[test]
fn semantic_button_focus_transfer_cancels_keyboard_activation() {
    focus_transfer_cancels_press(semantic(), semantic());
}

fn disabling_cancels_press(widget: impl Widget) {
    let mut harness = TestHarness::create(crate::hearthspace_light_properties(), widget.prepare());
    let id = harness.root_id();
    let space = Key::Character(" ".into());
    harness.focus_on(Some(id));
    harness.process_text_event(TextEvent::key_down(space.clone()));
    harness.edit_root_widget(|mut button| button.ctx.set_disabled(true));
    assert_eq!(harness.focused_widget_id(), None);
    harness.edit_root_widget(|mut button| button.ctx.set_disabled(false));
    harness.focus_on(Some(id));
    harness.process_text_event(TextEvent::key_up(space.clone()));
    assert!(harness.pop_action_erased().is_none());
    harness.process_text_event(TextEvent::key_down(space.clone()));
    harness.process_text_event(TextEvent::key_up(space));
    assert_eq!(
        harness.pop_action::<ButtonPress>(),
        Some((ButtonPress { button: None }, id))
    );
}

#[test]
fn native_button_disabling_cancels_keyboard_activation() {
    disabling_cancels_press(Button::with_text("Activate"));
}

#[test]
fn semantic_button_disabling_cancels_keyboard_activation() {
    disabling_cancels_press(semantic());
}

fn window_blur_cancels_press(widget: impl Widget) {
    let mut harness = TestHarness::create(crate::hearthspace_light_properties(), widget.prepare());
    let id = harness.root_id();
    let enter = Key::Named(NamedKey::Enter);
    harness.focus_on(Some(id));
    harness.process_text_event(TextEvent::key_down(enter.clone()));
    harness.process_text_event(TextEvent::WindowFocusChange(false));
    harness.process_text_event(TextEvent::WindowFocusChange(true));
    harness.process_text_event(TextEvent::key_up(enter.clone()));
    assert!(harness.pop_action_erased().is_none());
    harness.process_text_event(TextEvent::key_down(enter.clone()));
    harness.process_text_event(TextEvent::key_up(enter));
    assert_eq!(
        harness.pop_action::<ButtonPress>(),
        Some((ButtonPress { button: None }, id))
    );
}

#[test]
fn native_button_window_blur_cancels_keyboard_activation() {
    window_blur_cancels_press(Button::with_text("Activate"));
}

#[test]
fn semantic_button_window_blur_cancels_keyboard_activation() {
    window_blur_cancels_press(semantic());
}

#[test]
fn native_button_requires_matching_logical_and_physical_activation_keys() {
    let mut harness = TestHarness::create(
        crate::hearthspace_light_properties(),
        Button::with_text("Activate").prepare(),
    );
    let id = harness.root_id();
    harness.focus_on(Some(id));
    let enter = Key::Named(NamedKey::Enter);
    harness.process_text_event(TextEvent::key_up(enter.clone()));
    assert!(harness.pop_action_erased().is_none());
    harness.process_text_event(TextEvent::key_down(enter.clone()));
    harness.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    assert!(harness.pop_action_erased().is_none());
    harness.process_text_event(TextEvent::key_up(enter.clone()));
    assert!(harness.pop_action_erased().is_none());

    let TextEvent::Keyboard(mut down) = TextEvent::key_down(enter.clone()) else {
        unreachable!()
    };
    down.code = Code::Enter;
    let TextEvent::Keyboard(mut up) = TextEvent::key_up(enter) else {
        unreachable!()
    };
    up.code = Code::NumpadEnter;
    harness.process_text_event(TextEvent::Keyboard(down.clone()));
    harness.process_text_event(TextEvent::Keyboard(up.clone()));
    assert!(harness.pop_action_erased().is_none());
    up.code = Code::Enter;
    harness.process_text_event(TextEvent::Keyboard(down));
    harness.process_text_event(TextEvent::Keyboard(up));
    assert_eq!(
        harness.pop_action::<ButtonPress>(),
        Some((ButtonPress { button: None }, id))
    );
}
