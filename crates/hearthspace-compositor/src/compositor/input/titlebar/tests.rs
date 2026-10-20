use super::*;
use crate::compositor::input::touch_tracker::{TouchAction, TouchTracker};

fn target(id: u64, button: TitlebarButton) -> ControlTarget {
    ControlTarget {
        window_id: id,
        button,
        bounds: Rectangle::new((100.0, 50.0).into(), (32.0, 32.0).into()),
    }
}

#[test]
fn controls_activate_once_only_when_released_on_the_original_identity_and_geometry() {
    let original = target(7, TitlebarButton::Close);
    let mut input = TitlebarInput::default();
    input.press(original);
    assert!(input.held);
    assert_eq!(input.release(Some(original)), Some(original));
    assert!(!input.held);
    assert_eq!(input.release(Some(original)), None);

    let mut moved = original;
    moved.bounds.loc.x += 1.0;
    for release in [
        None,
        Some(target(8, TitlebarButton::Close)),
        Some(target(7, TitlebarButton::Maximize)),
        Some(moved),
    ] {
        input.press(original);
        assert_eq!(input.release(release), None);
        assert!(!input.held);
    }
}

#[test]
fn cancellation_swallows_the_owned_release_without_closing_or_maximizing() {
    let original = target(7, TitlebarButton::Maximize);
    let mut input = TitlebarInput::default();
    input.press(original);
    input.cancel();
    assert!(input.held, "cancellation must retain release ownership");
    assert_eq!(input.release(Some(original)), None);
    assert!(!input.held);

    input.title_down(7, (100.0, 50.0).into(), InputTime::from_millis(10));
    input.cancel();
    assert!(
        input.title_held,
        "a cancelled empty-titlebar press must retain release ownership"
    );
    input.title_up(InputTime::from_millis(20));
    assert!(!input.title_held);
    assert!(
        !input.title_down(7, (100.0, 50.0).into(), InputTime::from_millis(30)),
        "a cancelled titlebar click must not count toward a double click"
    );
}

#[test]
fn touch_chord_and_device_loss_releases_cannot_activate_the_pressed_control() {
    for removal in [false, true] {
        let mut touch = TouchTracker::default();
        touch.down("screen", Some(0).into(), (100.0, 50.0).into());
        touch.frame("screen");
        touch.motion("screen", Some(0).into(), (110.0, 50.0).into());
        let mut input = TitlebarInput::default();
        let original = target(7, TitlebarButton::Close);
        for (_, action) in touch.frame("screen") {
            if matches!(action, TouchAction::Press(_)) {
                input.press(original);
            }
        }
        assert!(input.held);
        assert!(touch.has_device_input("screen"));
        assert!(!touch.has_device_input("other screen"));
        input.cancel();
        let actions = if removal {
            touch.remove_device("screen")
        } else {
            touch.down("screen", Some(1).into(), (115.0, 55.0).into());
            touch.frame("screen")
        };
        assert!(
            actions
                .iter()
                .any(|(_, action)| *action == TouchAction::Release)
        );
        assert_eq!(input.release(Some(original)), None);
    }
}

#[test]
fn titlebar_double_click_requires_nearby_clicks_on_the_same_window_without_dragging() {
    let point = (100.0, 50.0).into();
    let mut input = TitlebarInput::default();
    assert!(!input.title_down(7, point, InputTime::from_millis(10)));
    input.title_up(InputTime::from_millis(20));
    assert!(input.title_down(7, (103.0, 54.0).into(), InputTime::from_millis(100)));
    assert!(input.held, "the second release remains compositor-owned");
    assert_eq!(input.release(None), None);

    for (window, point, time) in [
        (8, point, 100),
        (7, (106.0, 50.0).into(), 100),
        (7, point, 421),
        (7, point, 19),
    ] {
        let mut input = TitlebarInput::default();
        input.title_down(7, (100.0, 50.0).into(), InputTime::from_millis(10));
        input.title_up(InputTime::from_millis(20));
        assert!(!input.title_down(window, point, InputTime::from_millis(time)));
    }

    let mut input = TitlebarInput::default();
    input.title_down(7, point, InputTime::from_millis(10));
    input.motion((106.0, 50.0).into());
    input.motion(point);
    input.title_up(InputTime::from_millis(20));
    assert!(!input.title_down(7, point, InputTime::from_millis(100)));
}
