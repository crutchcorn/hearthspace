//! Real EWMH -> Satellite -> optional Wayland cancellation -> App interaction.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ClientMessageEvent, ConfigureWindowAux,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn rect(compositor: &HeadlessCompositor, client: &WindowClient) -> [i32; 4] {
    let windows = query(compositor, "windows");
    let window = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == client.id)
        .unwrap();
    ["canvas_x", "canvas_y", "content_width", "content_height"]
        .map(|key| window[key].as_i64().unwrap() as i32)
}

fn wait_rect(compositor: &HeadlessCompositor, client: &WindowClient, expected: [i32; 4]) {
    wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == client.id
                && ["canvas_x", "canvas_y", "content_width", "content_height"]
                    .map(|key| window[key].as_i64().unwrap() as i32)
                    == expected
        })
    });
}

fn motion(compositor: &HeadlessCompositor, x: i32, y: i32) {
    action(compositor, format!("pointer-motion-abs {x} {y}"));
}

fn press(compositor: &HeadlessCompositor, client: &WindowClient) {
    let [x, y, _, _] = rect(compositor, client);
    motion(compositor, x + 60, y + 20);
    client.drain(&mut Vec::new());
    action(compositor, "pointer-button-down 272");
    let until = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    loop {
        client.drain(&mut events);
        if events
            .iter()
            .any(|(press, event)| *press && event.detail == 1)
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "missing initiating X11 press: {events:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn request(compositor: &HeadlessCompositor, client: &WindowClient, direction: u32, button: u32) {
    let atom = client
        .connection
        .intern_atom(false, b"_NET_WM_MOVERESIZE")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    client
        .connection
        .ungrab_pointer(x11rb::CURRENT_TIME)
        .unwrap();
    client
        .connection
        .send_event(
            false,
            client.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ClientMessageEvent::new(32, client.window, atom, [0, 0, direction, button, 1]),
        )
        .unwrap();
    // Satellite processes this later property after the EWMH message, then
    // forwards both cancel and title on the same host Wayland connection.
    // Observing the title in App is therefore a protocol-dispatch barrier.
    static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
    let title = format!(
        "Satellite interaction {}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    client
        .connection
        .change_property8(
            PropMode::REPLACE,
            client.window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )
        .unwrap();
    client.connection.flush().unwrap();
    wait_windows(compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == client.id && window["title"] == title)
    });
}

#[test]
fn cancel_move_retains_geometry_drains_secondary_buttons_and_allows_a_new_move() {
    let _guard = headless_test_guard();
    let (mut compositor, display) = fixture();
    let a = WindowClient::new(&compositor, &display, "Satellite move", 0xdc143c);
    let b = WindowClient::new(&compositor, &display, "Satellite neighbor", 0x204080);
    action(&compositor, format!("move-window {} 200 200", a.id));
    action(&compositor, format!("move-window {} 600 400", b.id));
    press(&compositor, &a);
    request(&compositor, &a, 8, 1);
    motion(&compositor, 300, 250);
    wait_rect(&compositor, &a, [240, 230, 320, 240]);
    // A cancel from the other X window must not affect A's operation.
    request(&compositor, &b, 11, 0);
    motion(&compositor, 320, 260);
    wait_rect(&compositor, &a, [260, 240, 320, 240]);
    request(&compositor, &a, 11, 0);
    b.drain(&mut Vec::new());
    action(&compositor, "pointer-button-down 273");
    action(&compositor, "pointer-button-up 272");
    motion(&compositor, 680, 440);
    action(&compositor, "pointer-button-up 273");
    assert_eq!(rect(&compositor, &a), [260, 240, 320, 240]);
    // No press or unmatched release may escape the cancelled sequence.
    b.connection.get_input_focus().unwrap().reply().unwrap();
    let mut neighbor_events = Vec::new();
    b.drain(&mut neighbor_events);
    // The previous serial must not prevent a new independent operation.
    press(&compositor, &a);
    request(&compositor, &a, 8, 1);
    motion(&compositor, 350, 280);
    wait_rect(&compositor, &a, [290, 260, 320, 240]);
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
    let leaked: Vec<_> = neighbor_events
        .iter()
        .map(|(pressed, event)| {
            (
                *pressed,
                event.event,
                event.detail,
                event.event_x,
                event.event_y,
            )
        })
        .collect();
    assert!(
        neighbor_events.is_empty(),
        "held sequence leaked: {leaked:?}"
    );
}

#[test]
fn cancel_top_left_resize_preserves_anchor_for_a_late_xwayland_buffer() {
    let _guard = headless_test_guard();
    let (mut compositor, display) = fixture();
    let a = WindowClient::new(&compositor, &display, "Satellite late resize", 0xdc143c);
    action(&compositor, format!("move-window {} 200 200", a.id));
    // Hold the actual X server resize so no resized buffer exists at cancel.
    let mask = a
        .connection
        .get_window_attributes(a.window)
        .unwrap()
        .reply()
        .unwrap()
        .your_event_mask;
    a.connection
        .change_window_attributes(
            a.window,
            &ChangeWindowAttributesAux::new().event_mask(mask | EventMask::RESIZE_REDIRECT),
        )
        .unwrap()
        .check()
        .unwrap();
    press(&compositor, &a);
    request(&compositor, &a, 0, 1);
    motion(&compositor, 220, 190);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let mut received = false;
        while let Some(event) = a.connection.poll_for_event().unwrap() {
            if let Event::ResizeRequest(event) = event {
                received |= event.window == a.window && (event.width, event.height) == (360, 270);
            }
        }
        if received {
            break;
        }
        assert!(
            Instant::now() < until,
            "Satellite did not request the resized X buffer"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let geometry = a
        .connection
        .get_geometry(a.window)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(
        (geometry.width, geometry.height),
        (320, 240),
        "the X buffer must remain deferred"
    );
    // Satellite applies the requested viewport immediately, before its actual
    // X buffer is resized. Keep displayed geometry and buffer size distinct.
    wait_rect(&compositor, &a, [160, 170, 360, 270]);
    request(&compositor, &a, 11, 0);
    motion(&compositor, 700, 550);
    assert_eq!(rect(&compositor, &a), [160, 170, 360, 270]);
    a.connection
        .change_window_attributes(a.window, &ChangeWindowAttributesAux::new().event_mask(mask))
        .unwrap()
        .check()
        .unwrap();
    a.connection
        .configure_window(a.window, &ConfigureWindowAux::new().width(360).height(270))
        .unwrap()
        .check()
        .unwrap();
    wait_rect(&compositor, &a, [160, 170, 360, 270]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let geometry = a
            .connection
            .get_geometry(a.window)
            .unwrap()
            .reply()
            .unwrap();
        if (geometry.width, geometry.height) == (360, 270) {
            break;
        }
        assert!(Instant::now() < deadline, "late X11 resize was not applied");
        thread::sleep(Duration::from_millis(10));
    }
    action(&compositor, "pointer-button-up 272");
    motion(&compositor, 710, 560);
    assert_eq!(rect(&compositor, &a), [160, 170, 360, 270]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
