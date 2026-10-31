//! Deferred X11 input must retain its target without bypassing grab arbitration.
//! These specify the target-preservation contract, including on servers that
//! currently fail it; no fallback or ignored test hides incorrect delivery.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::{
    Allow, ButtonIndex, GrabMode, GrabStatus, ModMask,
};

#[path = "deferred_grabs_support.rs"]
mod support;
use support::*;

#[test]
fn x11_sync_passive_replay_preserves_child_and_queued_neighbor_targets() {
    let _guard = headless_test_guard();
    let windows = fixture();
    let a = windows.client.window;
    let b = windows.other;
    let child = mapped_child(&windows);
    grab_keyboard(&windows, GrabMode::ASYNC);
    // A foreign client intercepts A's press. Replaying must bypass this grab
    // but still descend into A's child, whose root rectangle also overlaps B.
    windows
        .observer
        .grab_button(
            false,
            a,
            button_mask(),
            GrabMode::SYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            ButtonIndex::M1,
            ModMask::ANY,
        )
        .unwrap()
        .check()
        .unwrap();
    let mut events = Observed::default();
    events.synchronize(&windows);
    events.assert_buttons(&windows, &[], &[]);

    batch(&windows.compositor, AB_WITH_KEY_MARKER);
    events.wait_for_ingress(&windows, 1);
    let intercepted = (true, a, [60, 20], [60, 20]);
    events.assert_buttons(&windows, &[], &[intercepted]);
    let intercepted_time = events.observer.buttons[0].1.time;

    windows
        .observer
        .allow_events(Allow::REPLAY_POINTER, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    events.synchronize(&windows);
    events.assert_buttons(
        &windows,
        &[
            (true, child, [20, 20], [60, 20]),
            (false, child, [20, 20], [60, 20]),
        ],
        &[
            intercepted,
            (true, b, [80, 40], [80, 40]),
            (false, b, [80, 40], [80, 40]),
        ],
    );
    assert_eq!(events.primary.buttons[0].1.time, intercepted_time);
    assert_scene(&windows);
    windows
        .observer
        .ungrab_button(ButtonIndex::M1, a, ModMask::ANY)
        .unwrap()
        .check()
        .unwrap();
    release_grabs(&windows);
}

#[test]
fn x11_sync_explicit_async_pointer_preserves_foreign_grab_ownership() {
    let _guard = headless_test_guard();
    let windows = fixture();
    let b = windows.other;
    grab_keyboard(&windows, GrabMode::ASYNC);
    let grab = windows
        .observer
        .grab_pointer(
            false,
            b,
            button_mask(),
            GrabMode::SYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            x11rb::CURRENT_TIME,
        )
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(grab.status, GrabStatus::SUCCESS);
    let mut events = Observed::default();
    events.synchronize(&windows);
    events.assert_buttons(&windows, &[], &[]);

    batch(&windows.compositor, AB_WITH_KEY_MARKER);
    events.wait_for_ingress(&windows, 1);
    events.assert_buttons(&windows, &[], &[]);

    // An explicit synchronous grab starts frozen without a delivered event.
    // ReplayPointer is a no-op here, unlike the passive-grab case above.
    windows
        .observer
        .allow_events(Allow::REPLAY_POINTER, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    events.synchronize(&windows);
    events.assert_buttons(&windows, &[], &[]);

    windows
        .observer
        .allow_events(Allow::ASYNC_POINTER, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    events.synchronize(&windows);
    // AsyncPointer thaws processing without releasing this explicit grab.
    // A's input legitimately reaches B because owner_events is false.
    events.assert_buttons(
        &windows,
        &[],
        &[
            (true, b, [60, 20], [60, 20]),
            (false, b, [60, 20], [60, 20]),
            (true, b, [80, 40], [80, 40]),
            (false, b, [80, 40], [80, 40]),
        ],
    );
    assert_scene(&windows);
    release_grabs(&windows);
}

#[test]
fn x11_frozen_destroyed_target_does_not_rebind_to_reused_popup_xid() {
    let _guard = headless_test_guard();
    let windows = fixture();
    let b = windows.other;
    let popup = windows.client.connection.generate_id().unwrap();
    map_popup(&windows, popup);
    // Freeze the pointer through B's keyboard grab, with no pointer grab.
    // Destroying P cannot release this independent grab or drain its queue.
    grab_keyboard(&windows, GrabMode::SYNC);
    let mut events = Observed::default();
    events.synchronize(&windows);
    events.assert_buttons(&windows, &[], &[]);

    // The first click now targets P, anchored over A at canvas (200,200).
    batch(&windows.compositor, AB_WITH_KEY_MARKER);
    events.wait_for_ingress(&windows, 1);
    events.assert_buttons(&windows, &[], &[]);
    windows
        .client
        .connection
        .destroy_window(popup)
        .unwrap()
        .check()
        .unwrap();
    wait_pixel(&windows.compositor, 260, 220, RED);
    // Reuse the exact client-owned surface-root XID, not a managed client's
    // XID whose XWM-owned frame could have had a different saved identity.
    map_popup(&windows, popup);
    events.synchronize(&windows);
    events.assert_buttons(&windows, &[], &[]);

    windows
        .observer
        .allow_events(Allow::ASYNC_POINTER, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    events.synchronize(&windows);
    let neighbor = [
        (true, b, [80, 40], [80, 40]),
        (false, b, [80, 40], [80, 40]),
    ];
    // Neither replacement P nor its transient parent A may inherit old input.
    events.assert_buttons(&windows, &[], &neighbor);

    batch(&windows.compositor, A_WITH_KEY_MARKER);
    events.wait_for_ingress(&windows, 2);
    // Fresh input to the replacement must still work, with no stale press or
    // release left over from the destroyed target's balanced sequence.
    events.assert_buttons(
        &windows,
        &[
            (true, popup, [60, 20], [60, 20]),
            (false, popup, [60, 20], [60, 20]),
        ],
        &neighbor,
    );
    assert_scene(&windows);
    release_grabs(&windows);
    windows
        .client
        .connection
        .destroy_window(popup)
        .unwrap()
        .check()
        .unwrap();
}
