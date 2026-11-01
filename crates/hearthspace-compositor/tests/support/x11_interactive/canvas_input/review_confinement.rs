use super::*;

#[test]
fn x11_live_child_confinement_updates_physical_position_before_relative_warp() {
    let _guard = headless_test_guard();
    let w = fixture();
    let confined = child(&w, [100, 100], [50, 50]);
    wait_pixel(&w.compositor, 320, 320, [32, 192, 96]);
    keyboard_marker(&w);
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 220 220\n{KEY_MARKER}"),
    );
    let mut seen = Observed::default();
    seen.key_fence(&w);
    assert_pointer(&w, w.client.root, frame(&w, w.client.window), [20, 20]);
    let reply = w
        .client
        .connection
        .grab_pointer(
            false,
            w.client.window,
            buttons() | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            confined,
            x11rb::NONE,
            x11rb::CURRENT_TIME,
        )
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(reply.status, GrabStatus::SUCCESS);
    assert_pointer(&w, w.client.window, confined, [100, 100]);
    seen.synchronize(&w);
    seen = Observed::default();

    // WarpPointer's source-window condition reads hotPhys, whereas
    // QueryPointer reads logical hot. This relative warp only executes when
    // physical state was actually moved into the child by the live grab.
    // A logical-only correction leaves it at (20,20), so the warp is ignored.
    w.client
        .connection
        .warp_pointer(confined, x11rb::NONE, 0, 0, 50, 50, 10, 10)
        .unwrap()
        .check()
        .unwrap();
    seen.wait(&w, |events| {
        events
            .primary
            .motions
            .iter()
            .any(|e| e.event == w.client.window && (e.root_x, e.root_y) == (110, 110))
    });
    assert_pointer(&w, w.client.window, confined, [110, 110]);
    assert!(
        seen.primary.buttons.is_empty() && seen.observer.buttons.is_empty(),
        "{seen:?}"
    );
    w.client
        .connection
        .ungrab_pointer(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    w.observer
        .ungrab_keyboard(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(geometry(&w.compositor, w.client.id), [200, 200, 320, 240]);
    assert_eq!(query(&w.compositor, "status")["viewport"], w.viewport);
}

#[test]
fn x11_buffered_neighbor_coordinates_survive_replayed_confined_grab() {
    let _guard = headless_test_guard();
    let w = fixture();
    let confined = child(&w, [0, 0], [50, 80]);
    select(&w.client.connection, confined, buttons());
    wait_pixel(&w.compositor, 220, 220, [32, 192, 96]);
    keyboard_marker(&w);
    w.observer
        .grab_button(
            false,
            w.client.window,
            buttons(),
            GrabMode::SYNC,
            GrabMode::ASYNC,
            confined,
            x11rb::NONE,
            ButtonIndex::M1,
            ModMask::ANY,
        )
        .unwrap()
        .check()
        .unwrap();
    let mut seen = Observed::default();
    seen.synchronize(&w);
    seen = Observed::default();

    // B's x=80 is outside A's confinement [0,50). All native input is
    // submitted before the grab is replayed; do not insert a delivery wait.
    batch(
        &w.compositor,
        &format!(
            "pointer-motion-abs 220 220\npointer-button-down 272\n\
             pointer-button-up 272\npointer-motion-abs 680 340\n\
             pointer-button-down 272\npointer-button-up 272\n{KEY_MARKER}"
        ),
    );
    seen.key_fence(&w);
    assert!(seen.primary.buttons.is_empty(), "{seen:?}");
    seen.observer
        .assert_buttons(w.client.root, &[(true, w.client.window, [20, 20])]);

    w.observer
        .allow_events(Allow::REPLAY_POINTER, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    seen.wait(&w, |events| {
        events.primary.buttons.len() >= 2 && events.observer.buttons.len() >= 3
    });
    seen.primary.assert_buttons(
        w.client.root,
        &[(true, confined, [20, 20]), (false, confined, [20, 20])],
    );
    seen.observer.assert_buttons(
        w.client.root,
        &[
            (true, w.client.window, [20, 20]),
            (true, w.other, [80, 40]),
            (false, w.other, [80, 40]),
        ],
    );
    assert_pointer(&w, w.client.root, frame(&w, w.other), [80, 40]);
    assert_eq!(geometry(&w.compositor, w.client.id), [200, 200, 320, 240]);
    assert_eq!(geometry(&w.compositor, w.other_id), [600, 300, 320, 240]);
    assert_eq!(query(&w.compositor, "status")["viewport"], w.viewport);
    w.observer
        .ungrab_keyboard(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
}
