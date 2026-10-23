use super::*;
use hearthspace_ipc::Settings;
use xilem::masonry::{
    core::keyboard::{Key, NamedKey},
    testing::TestHarness,
};

#[test]
fn clicking_blank_preview_repaints_keyboard_focus_without_changing_selection() {
    let monitors = Settings::default().monitors;
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_opaque_properties(),
        MonitorLayoutWidget::new(monitors.clone(), Some("primary".into())).prepare(),
        (400, 225),
    );
    let blank = Point::new(20.0, 20.0);
    let widget = harness.root_widget();
    assert!(
        widget
            .hit_monitor(blank, widget.layout_metrics(Size::new(400.0, 225.0)))
            .is_none()
    );
    harness.mouse_move(blank);
    let idle = harness.render();
    harness.press_tab_key(false);
    let focused = harness.render();
    assert!(
        focused != idle,
        "Tab must paint the selected monitor's focus ring"
    );
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert_eq!(harness.focused_widget_id(), Some(harness.root_id()));
    assert!(
        harness.render() == idle,
        "a blank-canvas click must immediately repaint away the keyboard ring"
    );
    assert_eq!(
        harness.root_widget().selected_monitor_id.as_deref(),
        Some("primary")
    );
    assert_eq!(harness.root_widget().monitors, monitors);
    assert!(harness.pop_action::<MonitorEditorAction>().is_none());
    harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Shift)));
    assert!(
        harness.render() == focused,
        "keyboard input must restore the same visible focus ring"
    );
}
