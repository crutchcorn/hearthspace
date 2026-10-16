use super::*;
use crate::compositor::input::favorite_drawing::model::Arm;

fn ready() -> FavoriteDrawing {
    let mut drawing = FavoriteDrawing::default();
    drawing
        .arm(Arm {
            generation: 10,
            window: 2,
            output: None,
            bounds: [0.0, 0.0, 300.0, 200.0],
            geometry: [0.0, 0.0, 1.0, 300.0, 200.0],
        })
        .unwrap();
    drawing
}

#[test]
fn direct_touch_uses_one_complete_stroke_without_pointer_events() {
    let now = Instant::now();
    let mut drawing = ready();
    assert!(drawing.touch_down("screen", Some(0).into(), Some([20.0, 20.0]), now));
    assert!(drawing.touch_motion("screen", Some(0).into(), Some([40.0, 40.0]), now));
    assert!(drawing.touch_up("screen", Some(0).into(), false, now));
    assert!(drawing.contacts.is_empty());
    let events = drawing.events();
    assert_eq!(
        events.iter().map(|event| event.phase).collect::<Vec<_>>(),
        ["begin", "update", "end"]
    );
    assert!(drawing.accept(10, events[0].stroke, now).is_ok());
}

#[test]
fn second_contact_cancels_then_all_contacts_are_consumed_through_release() {
    let now = Instant::now();
    let mut drawing = ready();
    drawing.touch_down("screen", Some(0).into(), Some([20.0, 20.0]), now);
    // A second finger outside the drawing area still belongs to this chord.
    assert!(drawing.touch_down("screen", Some(1).into(), None, now));
    assert!(drawing.touch_motion("screen", Some(0).into(), Some([40.0, 40.0]), now));
    assert!(drawing.touch_up("screen", Some(0).into(), false, now));
    drawing.clear();
    assert!(drawing.touch_down("screen", Some(2).into(), None, now));
    assert!(drawing.touch_up("screen", Some(1).into(), false, now));
    assert!(drawing.touch_up("screen", Some(2).into(), false, now));
    assert!(drawing.contacts.is_empty());
    assert_eq!(
        drawing
            .events()
            .iter()
            .map(|event| event.phase)
            .collect::<Vec<_>>(),
        ["begin", "cancel"]
    );
    assert!(!drawing.touch_down("screen", Some(3).into(), None, now));
}

#[test]
fn foreign_contacts_and_device_cancellation_do_not_leave_a_stuck_drawing() {
    let now = Instant::now();
    let mut drawing = ready();
    drawing.touch_down("screen", Some(0).into(), Some([20.0, 20.0]), now);
    assert!(!drawing.touch_motion("other", Some(0).into(), Some([40.0, 40.0]), now));
    assert!(!drawing.touch_up("other", Some(0).into(), false, now));
    assert!(!drawing.touch_removed("other"));
    assert!(drawing.touch_up("screen", None.into(), true, now));
    assert!(drawing.contacts.is_empty());
    let events = drawing.events();
    assert_eq!(events.last().unwrap().phase, "cancel");
    assert!(drawing.accept(10, events[0].stroke, now).is_err());
    assert!(drawing.touch_down("screen", Some(0).into(), Some([20.0, 20.0]), now));
    assert!(drawing.touch_removed("screen"));
    assert!(drawing.contacts.is_empty());
}

#[test]
fn invalid_or_excess_contacts_are_bounded_and_cannot_complete() {
    let now = Instant::now();
    let mut drawing = ready();
    assert!(!drawing.touch_down("screen", Some(0).into(), None, now));
    drawing.touch_down("screen", Some(0).into(), Some([20.0, 20.0]), now);
    for slot in 1..1000 {
        assert!(drawing.touch_down("screen", Some(slot).into(), None, now));
    }
    assert_eq!(drawing.contacts.len(), 64);
    for slot in 0..1000 {
        drawing.touch_up("screen", Some(slot).into(), false, now);
    }
    assert!(drawing.contacts.is_empty());
    assert_eq!(
        drawing
            .events()
            .iter()
            .map(|event| event.phase)
            .collect::<Vec<_>>(),
        ["begin", "cancel"]
    );
}
