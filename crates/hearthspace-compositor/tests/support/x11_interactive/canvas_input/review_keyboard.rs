//! Keyboard thaw/replay must not replace the shared logical pointer ancestry.
use super::*;

const CLICK_FENCE: &str = "pointer-button-down 272\npointer-button-up 272\n";

fn pointer_root_fixture() -> Windows {
    let w = fixture();
    let keys = EventMask::KEY_PRESS | EventMask::KEY_RELEASE;
    select(&w.client.connection, w.client.window, keys);
    select(&w.observer, w.other, keys);
    // Establish both compositor keyboard focus and the DIX pointer on A.
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 260 220\n{CLICK_FENCE}"),
    );
    let mut initial = Observed::default();
    initial.pointer_fence(&w);
    xwm_barrier(&w.compositor, &w.observer, w.other, w.other_id);
    w.observer
        .set_input_focus(
            InputFocus::POINTER_ROOT,
            u32::from(InputFocus::POINTER_ROOT),
            x11rb::CURRENT_TIME,
        )
        .unwrap()
        .check()
        .unwrap();
    assert_pointer_root(&w);
    w
}

fn assert_pointer_root(w: &Windows) {
    let focus = w.observer.get_input_focus().unwrap().reply().unwrap();
    assert_eq!(focus.focus, u32::from(InputFocus::POINTER_ROOT));
    let a_frame = frame(w, w.client.window);
    assert_pointer(w, w.client.root, a_frame, [60, 20]);
    assert_pointer(w, a_frame, x11rb::NONE, [60, 20]);
    assert_pointer(w, frame(w, w.other), x11rb::NONE, [60, 20]);
}

fn raise_b(w: &Windows) {
    raise_neighbor_backing(w);
    assert_pointer_root(w);
}

fn assert_key_coordinates(events: &Events) {
    for (_, event) in &events.keys {
        assert_eq!([event.event_x, event.event_y], [60, 20], "{event:?}");
        assert_eq!([event.root_x, event.root_y], [60, 20], "{event:?}");
        assert_eq!(event.child, x11rb::NONE, "{event:?}");
    }
}

#[test]
fn x11_pending_keyboard_thaw_preserves_pointer_root_after_foreign_restack() {
    let _guard = headless_test_guard();
    let w = pointer_root_fixture();
    // Freeze only the keyboard through an independent pointer grab. There is
    // no keyboard grab to override PointerRoot delivery when it is thawed.
    let grab = w
        .client
        .connection
        .grab_pointer(
            false,
            w.client.window,
            buttons(),
            GrabMode::ASYNC,
            GrabMode::SYNC,
            x11rb::NONE,
            x11rb::NONE,
            x11rb::CURRENT_TIME,
        )
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(grab.status, GrabStatus::SUCCESS);
    let mut seen = Observed::default();
    seen.synchronize(&w);

    // Two balanced keys are pending before the unfrozen core click. One
    // write, with no per-command acknowledgement wait or X synchronization.
    batch(
        &w.compositor,
        &format!("{KEY_MARKER}{KEY_MARKER}{CLICK_FENCE}"),
    );
    seen.pointer_fence(&w);
    assert!(seen.primary.keys.is_empty(), "{seen:?}");
    assert!(seen.observer.keys.is_empty(), "{seen:?}");
    raise_b(&w);

    // No further pointer input may repair the shared sprite before either
    // the queued recipients or QueryPointer are checked.
    w.client
        .connection
        .allow_events(Allow::ASYNC_KEYBOARD, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    seen.synchronize(&w);
    seen.primary.assert_keys(w.client.root, w.client.window, 4);
    assert!(seen.observer.keys.is_empty(), "{seen:?}");
    assert_key_coordinates(&seen.primary);
    assert_pointer_root(&w);
    w.client
        .connection
        .ungrab_pointer(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
}

#[test]
fn x11_saved_replay_keyboard_preserves_pointer_root_after_foreign_restack() {
    let _guard = headless_test_guard();
    let w = pointer_root_fixture();
    // CB intercepts A's key without selecting A's normal key events. The
    // saved activation must replay to CA, bypassing this passive grab.
    w.observer
        .grab_key(
            false,
            w.client.window,
            ModMask::ANY,
            38_u8,
            GrabMode::ASYNC,
            GrabMode::SYNC,
        )
        .unwrap()
        .check()
        .unwrap();
    let mut seen = Observed::default();
    seen.synchronize(&w);
    batch(&w.compositor, &format!("{KEY_MARKER}{CLICK_FENCE}"));
    seen.pointer_fence(&w);
    assert!(seen.primary.keys.is_empty(), "{seen:?}");
    seen.observer.assert_keys(w.client.root, w.client.window, 1);
    assert_key_coordinates(&seen.observer);
    let intercepted_time = seen.observer.keys[0].1.time;
    raise_b(&w);

    w.observer
        .allow_events(Allow::REPLAY_KEYBOARD, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    seen.synchronize(&w);
    seen.primary.assert_keys(w.client.root, w.client.window, 2);
    // CB retains only its intercepted press; neither the replayed press nor
    // the queued release may leak to B or reactivate the same passive grab.
    seen.observer.assert_keys(w.client.root, w.client.window, 1);
    assert_eq!(seen.primary.keys[0].1.time, intercepted_time);
    assert_key_coordinates(&seen.primary);
    assert_pointer_root(&w);
    w.observer
        .ungrab_key(38_u8, w.client.window, ModMask::ANY)
        .unwrap()
        .check()
        .unwrap();
}
