//! X stacking must follow input even when both clicks arrive in one dispatch.
use super::*;
use smithay::reexports::x11rb::protocol::xproto::{ConfigureWindowAux, StackMode};

#[path = "deferred_grabs.rs"]
mod deferred_grabs;
#[path = "target_cache.rs"]
mod target_cache;
#[path = "xwm_barrier.rs"]
mod xwm_barrier;
use xwm_barrier::xwm_barrier;

struct Windows {
    compositor: HeadlessCompositor,
    client: Client,
    observer: RustConnection,
    other: Window,
    other_id: u64,
    viewport: Value,
}

fn drain_buttons(
    connection: &RustConnection,
    recipient: Window,
    events: &mut Vec<(bool, ButtonPressEvent)>,
) {
    while let Some(event) = connection.poll_for_event().unwrap() {
        let (pressed, event) = match event {
            Event::ButtonPress(event) => (true, event),
            Event::ButtonRelease(event) => (false, event),
            Event::Error(error) => panic!("X11 batch fixture error: {error:?}"),
            _ => continue,
        };
        assert_eq!(event.event, recipient, "batch click reached the wrong XID");
        events.push((pressed, event));
    }
}

fn assert_click(events: &[(bool, ButtonPressEvent)], local: [i16; 2], origin: (i32, i32)) {
    assert_eq!(events.len(), 2, "exactly one balanced click: {events:?}");
    assert!(events[0].0 && !events[1].0, "press must precede release");
    for (_, event) in events {
        assert_eq!(event.detail, 1);
        assert!(event.same_screen);
        assert_eq!([event.event_x, event.event_y], local);
        assert_eq!(
            [i32::from(event.root_x), i32::from(event.root_y)],
            [
                origin.0 + i32::from(local[0]),
                origin.1 + i32::from(local[1])
            ]
        );
    }
}

fn fixture() -> Windows {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = Client::connect(&compositor);
    let status = query(&compositor, "status");
    let (observer, _) = x11rb::connect(status["xwayland_display"].as_str()).unwrap();
    let other = window(&observer, [600, 300], [320, 240], false);
    observer.map_window(other).unwrap();
    observer.flush().unwrap();
    let other_id = wait_windows(&compositor, |windows| {
        windows.len() == 2 && windows.iter().all(|w| w["mapped"] == true)
    })
    .into_iter()
    .find(|w| w["id"] != client.id)
    .unwrap()["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {} 200 200", client.id));
    action(&compositor, format!("move-window {other_id} 600 300"));
    action(&compositor, "pan-to 0 0");
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [200, 200, 320, 240], RED);
    wait_pixel(&compositor, 680, 340, BLUE);
    let viewport = query(&compositor, "status")["viewport"].clone();
    assert_eq!(viewport["x"], 0);
    assert_eq!(viewport["y"], 0);
    assert_eq!(viewport["scale"], 1.0);
    let origin = client.origin();
    let other_origin = observer
        .translate_coordinates(other, client.root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(origin, (0, 0));
    assert_eq!(
        (other_origin.dst_x, other_origin.dst_y),
        (0, 0),
        "the visibly separate clients must share their backing root rectangle"
    );
    Windows {
        compositor,
        client,
        observer,
        other,
        other_id,
        viewport,
    }
}

fn batch(compositor: &HeadlessCompositor, commands: &str) {
    let mut stream = UnixStream::connect(&compositor.command_socket_path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(commands.as_bytes()).unwrap();
    for _ in commands.lines() {
        assert_eq!(read_line(&mut stream), "ok\n");
    }
}

#[test]
fn x11_batched_clicks_route_to_each_visible_window_despite_overlapping_roots() {
    let _guard = headless_test_guard();
    let Windows {
        compositor,
        mut client,
        observer,
        other,
        other_id,
        viewport,
    } = fixture();
    let origin = client.origin();
    client.clear_events();
    let mut other_events = Vec::new();
    drain_buttons(&observer, other, &mut other_events);
    assert!(other_events.is_empty());

    // One socket write, with no X roundtrip, acknowledgement wait, or delay
    // between motion/press/release for A and motion/press/release for B.
    batch(
        &compositor,
        concat!(
            "pointer-motion-abs 260 220\n",
            "pointer-button-down 272\n",
            "pointer-button-up 272\n",
            "pointer-motion-abs 680 340\n",
            "pointer-button-down 272\n",
            "pointer-button-up 272\n",
        ),
    );

    let mut client_events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        drain_buttons(&client.connection, client.window, &mut client_events);
        drain_buttons(&observer, other, &mut other_events);
        if client_events.iter().any(|(pressed, _)| !*pressed)
            && other_events.iter().any(|(pressed, _)| !*pressed)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "both recipients must receive a balanced batch: primary={client_events:?}, neighbor={other_events:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
    // Drain any extra events before checking exact counts, after the entire
    // batch has already run. These roundtrips cannot order the input sequence.
    client
        .connection
        .get_input_focus()
        .unwrap()
        .reply()
        .unwrap();
    observer.get_input_focus().unwrap().reply().unwrap();
    drain_buttons(&client.connection, client.window, &mut client_events);
    drain_buttons(&observer, other, &mut other_events);
    assert_click(&client_events, [60, 20], origin);
    assert_click(&other_events, [80, 40], origin);
    assert_eq!(geometry(&compositor, client.id), [200, 200, 320, 240]);
    assert_eq!(geometry(&compositor, other_id), [600, 300, 320, 240]);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
}

#[test]
fn x11_foreign_popup_map_remap_and_restack_cannot_steal_cached_top_client_click() {
    let _guard = headless_test_guard();
    let Windows {
        compositor,
        mut client,
        observer,
        other,
        other_id,
        viewport,
    } = fixture();
    let origin = client.origin();
    // Root point (60,20) lies in this popup, but its foreign parent's canvas
    // anchor displays it at (640,300), far from A's click at (260,220).
    let popup = window(&observer, [40, 0], [120, 80], true);
    observer
        .change_property32(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[other],
        )
        .unwrap();
    for phase in ["map", "remap", "external restack"] {
        if phase == "remap" {
            observer.unmap_window(popup).unwrap();
            observer.flush().unwrap();
            wait_pixel(&compositor, 650, 310, BLUE);
        }
        // Establish A as the cached top managed window before the foreign X
        // operation. Synchronization here isolates cache invalidation from the
        // separate batched-input ordering regression above.
        motion(&compositor, 260, 220);
        xwm_barrier(&observer, other);
        client.clear_events();
        if phase == "external restack" {
            observer
                .configure_window(
                    popup,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )
                .unwrap();
        } else {
            observer.map_window(popup).unwrap();
        }
        observer.flush().unwrap();
        xwm_barrier(&observer, other);
        wait_pixel(&compositor, 650, 310, GREEN);
        target_cache::assert_popup_stack(&client, &observer, popup, true);
        batch(
            &compositor,
            concat!(
                "pointer-motion-abs 260 220\n",
                "pointer-button-down 272\n",
                "pointer-button-up 272\n",
            ),
        );
        let mut client_events = Vec::new();
        let mut popup_events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            drain_buttons(&client.connection, client.window, &mut client_events);
            drain_buttons(&observer, popup, &mut popup_events);
            assert!(
                popup_events.is_empty(),
                "foreign popup stole A's click after {phase}: {popup_events:?}"
            );
            if client_events.iter().any(|(pressed, _)| !*pressed) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "A did not receive its click after {phase}: {client_events:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        client
            .connection
            .get_input_focus()
            .unwrap()
            .reply()
            .unwrap();
        observer.get_input_focus().unwrap().reply().unwrap();
        drain_buttons(&client.connection, client.window, &mut client_events);
        drain_buttons(&observer, popup, &mut popup_events);
        assert!(
            popup_events.is_empty(),
            "foreign popup received trailing input after {phase}"
        );
        assert_click(&client_events, [60, 20], origin);
        assert_eq!(geometry(&compositor, client.id), [200, 200, 320, 240]);
        assert_eq!(geometry(&compositor, other_id), [600, 300, 320, 240]);
        assert_eq!(query(&compositor, "status")["viewport"], viewport);
    }
    observer.destroy_window(popup).unwrap();
    observer.flush().unwrap();
    wait_pixel(&compositor, 650, 310, BLUE);
}
