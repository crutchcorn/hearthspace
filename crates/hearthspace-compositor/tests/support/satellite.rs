//! Adoption gates use real Satellite, stock Xwayland, and independent X clients.
#[path = "satellite/cancellation.rs"]
mod cancellation;

use super::{
    navigation::{action, query, wait_windows},
    *,
};
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            AtomEnum, ButtonPressEvent, ConnectionExt, CreateWindowAux, EventMask, PropMode,
            Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

struct WindowClient {
    connection: RustConnection,
    window: Window,
    root: Window,
    id: u64,
}

impl WindowClient {
    fn new(compositor: &HeadlessCompositor, display: &str, title: &str, color: u32) -> Self {
        let (connection, screen) = x11rb::connect(Some(display)).unwrap();
        let screen = &connection.setup().roots[screen];
        let root = screen.root;
        let window = connection.generate_id().unwrap();
        connection
            .create_window(
                screen.root_depth,
                window,
                root,
                100,
                100,
                320,
                240,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new().background_pixel(color).event_mask(
                    EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::STRUCTURE_NOTIFY,
                ),
            )
            .unwrap()
            .check()
            .unwrap();
        let motif = connection
            .intern_atom(false, b"_MOTIF_WM_HINTS")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .change_property32(PropMode::REPLACE, window, motif, motif, &[2, 0, 0, 0, 0])
            .unwrap();
        connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )
            .unwrap();
        connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"satellite-fixture\0SatelliteFixture\0",
            )
            .unwrap();
        connection.map_window(window).unwrap().check().unwrap();
        let windows = wait_windows(compositor, |windows| {
            windows
                .iter()
                .any(|w| w["title"] == title && w["mapped"] == true)
        });
        let managed = windows.iter().find(|w| w["title"] == title).unwrap();
        assert_eq!(
            managed["client_type"], "x11",
            "bridge identity must not look like a native application"
        );
        assert_eq!(
            managed["titlebar_height"], 0,
            "fixture must retain its CSD geometry"
        );
        Self {
            connection,
            window,
            root,
            id: managed["id"].as_u64().unwrap(),
        }
    }

    fn drain(&self, events: &mut Vec<(bool, ButtonPressEvent)>) {
        while let Some(event) = self.connection.poll_for_event().unwrap() {
            match event {
                Event::ButtonPress(event) => events.push((true, event)),
                Event::ButtonRelease(event) => events.push((false, event)),
                Event::Error(error) => panic!("X11 fixture error: {error:?}"),
                _ => {}
            }
        }
    }

    fn origin(&self) -> (i16, i16) {
        let reply = self
            .connection
            .translate_coordinates(self.window, self.root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        (reply.dst_x, reply.dst_y)
    }
}

fn fixture() -> (HeadlessCompositor, String) {
    let mut compositor =
        HeadlessCompositor::spawn_with_command("1280x800", None, None, 1, false, |command| {
            command.env("HEARTHSPACE_X11_BACKEND", "satellite");
        });
    drop(compositor.wait_for_socket());
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(display) = query(&compositor, "status")["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline, "Satellite did not become ready");
        thread::sleep(Duration::from_millis(10));
    };
    (compositor, display)
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

fn balanced(client: &WindowClient, events: &[(bool, ButtonPressEvent)], local: (i16, i16)) {
    assert_eq!(
        events.len(),
        2,
        "one balanced click per recipient: {events:?}"
    );
    assert!(events[0].0 && !events[1].0);
    for (_, event) in events {
        assert_eq!(
            event.event, client.window,
            "click reached the wrong X window"
        );
        assert_eq!((event.event_x, event.event_y), local);
        assert_eq!(event.detail, 1);
    }
}

fn wait_click(client: &WindowClient) -> Vec<(bool, ButtonPressEvent)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    loop {
        client.drain(&mut events);
        if events.iter().any(|(press, _)| !press) {
            return events;
        }
        assert!(Instant::now() < deadline, "missing click: {events:?}");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn satellite_batched_clicks_keep_independent_recipients_with_overlapping_x_roots() {
    let _guard = headless_test_guard();
    let (mut compositor, display) = fixture();
    let a = WindowClient::new(&compositor, &display, "Satellite A", 0xdc143c);
    let b = WindowClient::new(&compositor, &display, "Satellite B", 0x204080);
    action(&compositor, format!("move-window {} 200 200", a.id));
    action(&compositor, format!("move-window {} 600 300", b.id));
    action(&compositor, "pan-to 0 0");
    assert_eq!(
        a.origin(),
        b.origin(),
        "backing roots must overlap for this regression"
    );
    a.drain(&mut Vec::new());
    b.drain(&mut Vec::new());
    batch(
        &compositor,
        concat!(
            "pointer-motion-abs 260 220\n",
            "pointer-button-down 272\n",
            "pointer-button-up 272\n",
        ),
    );
    balanced(&a, &wait_click(&a), (60, 20));
    batch(
        &compositor,
        concat!(
            "pointer-motion-abs 680 340\n",
            "pointer-button-down 272\n",
            "pointer-button-up 272\n",
        ),
    );
    balanced(&b, &wait_click(&b), (80, 40));
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
    let mut a_events = Vec::new();
    let mut b_events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        a.drain(&mut a_events);
        b.drain(&mut b_events);
        if a_events.iter().any(|(pressed, _)| !pressed)
            && b_events.iter().any(|(pressed, _)| !pressed)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    // Both input sequences have finished before these checks can add X traffic.
    a.connection.get_input_focus().unwrap().reply().unwrap();
    b.connection.get_input_focus().unwrap().reply().unwrap();
    a.drain(&mut a_events);
    b.drain(&mut b_events);
    eprintln!("Batched recipients: A={a_events:?}, B={b_events:?}");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
    balanced(&a, &a_events, (60, 20));
    balanced(&b, &b_events, (80, 40));
}

fn canvas_click(position: (i32, i32)) {
    let (mut compositor, display) = fixture();
    let a = WindowClient::new(&compositor, &display, "Satellite canvas", 0xdc143c);
    action(
        &compositor,
        format!("move-window {} {} {}", a.id, position.0, position.1),
    );
    action(
        &compositor,
        format!("pan-to {} {}", position.0 - 200, position.1 - 200),
    );
    batch(
        &compositor,
        concat!(
            "pointer-motion-abs 260 220\n",
            "pointer-button-down 272\n",
            "pointer-button-up 272\n",
        ),
    );
    balanced(&a, &wait_click(&a), (60, 20));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn satellite_negative_canvas_position_keeps_client_input_usable() {
    let _guard = headless_test_guard();
    canvas_click((89, -179));
}

#[test]
fn satellite_far_canvas_position_keeps_client_input_usable() {
    let _guard = headless_test_guard();
    canvas_click((50_000, 60_000));
}
