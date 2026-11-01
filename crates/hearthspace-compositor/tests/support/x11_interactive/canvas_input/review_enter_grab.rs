use super::*;

#[test]
fn x11_enter_grab_preserves_core_crossings_and_focus_after_previous_window_closes() {
    let _guard = headless_test_guard();
    let w = fixture();
    // Keep both leaves within B's one Wayland surface. Crossing two managed
    // surfaces would first publish a normal leave to the X root, missing the
    // retained-old-window path in ActivateEnterGrab.
    let c = &w.client.connection;
    let screen = &c.setup().roots[0];
    let leaf = c.generate_id().unwrap();
    let crossings = EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW;
    c.create_window(
        screen.root_depth,
        leaf,
        w.other,
        20,
        20,
        40,
        40,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new()
            .background_pixel(0x20c060)
            .event_mask(crossings),
    )
    .unwrap()
    .check()
    .unwrap();
    c.map_window(leaf).unwrap().check().unwrap();
    assert_eq!(c.query_tree(leaf).unwrap().reply().unwrap().parent, w.other);
    select(&w.observer, w.other, crossings);
    wait_pixel(&w.compositor, 630, 330, [32, 192, 96]);
    let device = xi_pointer(&w);
    keyboard_marker(&w);
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 630 330\n{KEY_MARKER}"),
    );
    let mut seen = Observed::default();
    seen.key_fence(&w);
    assert!(
        seen.primary
            .crossings
            .iter()
            .any(|(entered, e)| { *entered && e.event == leaf && e.mode == NotifyMode::NORMAL }),
        "initial crossing must establish the child: {seen:?}"
    );
    assert_pointer(&w, w.other, leaf, [30, 30]);

    let result = w
        .observer
        .xinput_xi_passive_grab_device(
            x11rb::CURRENT_TIME,
            w.other,
            x11rb::NONE,
            0,
            device,
            xi::GrabType::ENTER,
            xi::GrabMode22::ASYNC,
            GrabMode::ASYNC,
            xi::GrabOwner::NO_OWNER,
            &[u32::from(xi::XIEventMask::ENTER | xi::XIEventMask::LEAVE)],
            &[u32::from(xi::ModifierMask::ANY)],
        )
        .unwrap()
        .reply()
        .unwrap();
    assert!(
        result.modifiers.is_empty(),
        "enter grab rejected: {result:?}"
    );
    seen = Observed::default();
    batch(
        &w.compositor,
        &format!("pointer-motion-abs 680 340\n{KEY_MARKER}"),
    );
    seen.key_fence(&w);
    assert!(
        seen.observer.xi_crossings.iter().any(|(entered, e)| {
            *entered
                && e.event == w.other
                && e.mode == xi::NotifyMode::PASSIVE_GRAB
                && e.detail == xi::NotifyDetail::INFERIOR
                && e.deviceid == device
                && e.sourceid == device
        }),
        "XI2 enter grab must actually activate before checking core crossings: {seen:?}"
    );
    for (events, entered, recipient, detail, local) in [
        (&seen.primary, false, leaf, NotifyDetail::ANCESTOR, [60, 20]),
        (
            &seen.observer,
            true,
            w.other,
            NotifyDetail::INFERIOR,
            [80, 40],
        ),
    ] {
        assert_eq!(events.crossings.len(), 1, "{seen:?}");
        let (actual_entered, event) = events.crossings[0];
        assert_eq!((actual_entered, event.event), (entered, recipient));
        assert_eq!((event.mode, event.detail), (NotifyMode::GRAB, detail));
        assert_eq!([event.event_x, event.event_y], local);
        assert_eq!([event.root_x, event.root_y], [80, 40]);
        assert_eq!((event.root, event.child), (w.client.root, x11rb::NONE));
        assert_eq!(event.response_type & 0x80, 0);
        // Core crossing events pack focus (bit 0) and same-screen (bit 1).
        assert_ne!(event.same_screen_focus & (1 << 1), 0);
    }
    assert_pointer(&w, w.other, x11rb::NONE, [80, 40]);

    // The core crossings above must update PointerWindows before the old
    // child is destroyed. QueryPointer alone observes a different sprite field.
    c.destroy_window(leaf).unwrap().check().unwrap();
    for focus in [w.other, u32::from(InputFocus::POINTER_ROOT)] {
        w.observer
            .set_input_focus(InputFocus::POINTER_ROOT, focus, x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            w.observer.get_input_focus().unwrap().reply().unwrap().focus,
            focus
        );
    }
    assert_pointer(&w, w.client.root, frame(&w, w.other), [80, 40]);
    w.observer
        .xinput_xi_ungrab_device(x11rb::CURRENT_TIME, device)
        .unwrap()
        .check()
        .unwrap();
    w.observer
        .ungrab_keyboard(x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    seen.synchronize(&w);
    seen = Observed::default();
    batch(
        &w.compositor,
        "pointer-button-down 272\npointer-button-up 272\n",
    );
    seen.wait(&w, |events| events.observer.buttons.len() >= 2);
    seen.observer.assert_buttons(
        w.client.root,
        &[(true, w.other, [80, 40]), (false, w.other, [80, 40])],
    );
    assert!(seen.primary.buttons.is_empty(), "{seen:?}");
    assert_eq!(geometry(&w.compositor, w.client.id), [200, 200, 320, 240]);
    assert_eq!(geometry(&w.compositor, w.other_id), [600, 300, 320, 240]);
    assert_eq!(query(&w.compositor, "status")["viewport"], w.viewport);
}
