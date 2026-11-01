//! Wayland focus boundaries must not invent XI2 raw device motion.
use super::*;

fn assert_crossing(seen: &Observed, w: &Windows, entered: bool, point: [i16; 2]) {
    assert_eq!(seen.primary.crossings.len(), 1, "{seen:?}");
    let (actual_entered, event) = &seen.primary.crossings[0];
    assert_eq!((*actual_entered, event.event), (entered, w.client.window));
    assert_eq!(event.root, w.client.root);
    assert_eq!(event.mode, NotifyMode::NORMAL);
    assert_eq!(event.response_type & 0x80, 0);
    assert_eq!([event.event_x, event.event_y], point, "{event:?}");
    assert_eq!([event.root_x, event.root_y], point, "{event:?}");
    assert!(seen.observer.crossings.is_empty(), "{seen:?}");
    assert!(seen.primary.buttons.is_empty(), "{seen:?}");
    assert!(seen.observer.buttons.is_empty(), "{seen:?}");
}

#[test]
fn x11_xi2_focus_boundaries_emit_no_raw_motion_but_ordinary_motion_does() {
    let _guard = headless_test_guard();
    let w = fixture();
    let crossings = EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW;
    select(&w.client.connection, w.client.window, crossings);
    select(&w.observer, w.other, crossings);
    let device = xi_pointer(&w);
    keyboard_marker(&w);
    // Establish an unfocused pointer before selecting raw events. The key
    // release is the Wayland/MI ingress fence, even over the empty canvas.
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 1100 650\n{KEY_MARKER}"),
    );
    Observed::default().key_fence(&w);
    // XISelectEvents accepts raw selection only on a root. Select the core
    // master alone so a slave/master pair cannot look like duplicate motion.
    w.observer
        .xinput_xi_select_events(
            w.client.root,
            &[xi::EventMask {
                deviceid: device,
                mask: vec![xi::XIEventMask::RAW_MOTION],
            }],
        )
        .unwrap()
        .check()
        .unwrap();

    // The absolute command changes Smithay's focus from None to A: it sends
    // wl_pointer.enter, with no wl_pointer.motion or relative-pointer delta.
    // Xwayland's boundary MotionNotify must retain POINTER_NORAW.
    let mut entered = Observed::default();
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 260 220\n{KEY_MARKER}"),
    );
    entered.key_fence(&w);
    assert_crossing(&entered, &w, true, [60, 20]);
    assert!(entered.observer.raw_motion.is_empty(), "{entered:?}");
    assert!(entered.primary.raw_motion.is_empty(), "{entered:?}");
    assert_pointer(&w, w.client.root, frame(&w, w.client.window), [60, 20]);

    // Positive control: the same selection must receive a real ordinary
    // wl_pointer.motion within A. This command carries absolute coordinates,
    // so its raw valuators are (80,40), not a fabricated relative delta.
    let mut moved = Observed::default();
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 280 240\n{KEY_MARKER}"),
    );
    moved.key_fence(&w);
    assert!(moved.primary.crossings.is_empty(), "{moved:?}");
    assert!(moved.observer.crossings.is_empty(), "{moved:?}");
    assert!(moved.primary.raw_motion.is_empty(), "{moved:?}");
    assert_eq!(moved.observer.raw_motion.len(), 1, "{moved:?}");
    let raw = &moved.observer.raw_motion[0];
    assert_eq!(raw.deviceid, device, "{raw:?}");
    assert_ne!(raw.sourceid, device, "{raw:?}");
    assert_eq!(raw.event_type, xi::RAW_MOTION_EVENT);
    assert_eq!(raw.response_type & 0x80, 0);
    // Xwayland pads this mask to MAX_VALUATORS (36); only X/Y may be set.
    assert_eq!(raw.valuator_mask.first(), Some(&0b11), "{raw:?}");
    assert!(
        raw.valuator_mask[1..].iter().all(|word| *word == 0),
        "{raw:?}"
    );
    assert_eq!(moved.primary.motions.len(), 1, "{moved:?}");
    let motion = &moved.primary.motions[0];
    assert_eq!(motion.event, w.client.window, "{motion:?}");
    assert_eq!([motion.event_x, motion.event_y], [80, 40]);
    assert_eq!([motion.root_x, motion.root_y], [80, 40]);
    assert_eq!(motion.response_type & 0x80, 0);
    assert!(moved.observer.motions.is_empty(), "{moved:?}");
    let values: Vec<_> = raw
        .axisvalues_raw
        .iter()
        .map(|value| (value.integral, value.frac))
        .collect();
    assert_eq!(values, vec![(80, 0), (40, 0)], "{raw:?}");
    assert_pointer(&w, w.client.root, frame(&w, w.client.window), [80, 40]);

    // None replaces A without any relative-device motion. Observe the leave
    // and the following balanced key before asserting the absence of raws.
    let mut left = Observed::default();
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 1100 650\n{KEY_MARKER}"),
    );
    left.key_fence(&w);
    assert_crossing(&left, &w, false, [80, 40]);
    assert!(left.observer.raw_motion.is_empty(), "{left:?}");
    assert!(left.primary.raw_motion.is_empty(), "{left:?}");
    assert_pointer(&w, w.client.root, x11rb::NONE, [80, 40]);
    w.observer
        .ungrab_keyboard(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
}
