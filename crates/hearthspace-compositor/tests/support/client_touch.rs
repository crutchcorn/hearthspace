//! Opt-in native touch on the real SHM client; existing clients stay pointer-only.
use super::{InteractiveClient, State as ClientState, events::assert_buttons};
use crate::{
    HeadlessCompositor,
    canvas_zoom::wait_scale,
    headless_test_guard,
    navigation::{action, query, wait_windows},
};
use serde_json::Value;
use wayland_client::{Connection, Dispatch, QueueHandle, protocol::wl_touch};
use wl_touch::Event;

#[path = "client_touch/axis.rs"]
mod axis;
#[path = "client_touch/cancellation.rs"]
mod cancellation;
#[path = "client_touch/delivery.rs"]
mod delivery;
#[path = "client_touch/interactive.rs"]
mod interactive;
#[path = "client_touch/ownership.rs"]
mod ownership;

#[derive(Default)]
pub(super) struct State {
    touch: Option<wl_touch::WlTouch>,
    events: Vec<Event>,
    down_serial: Option<u32>,
    up_serial: Option<u32>,
}

impl InteractiveClient {
    /// Bind before injecting input; connecting alone preserves pointer fallback.
    pub(crate) fn enable_touch(&mut self) {
        if self.state.touch.touch.is_none() {
            self.state.touch.touch = Some(
                self.state
                    .seat
                    .as_ref()
                    .expect("client seat")
                    .get_touch(&self.queue.handle(), ()),
            );
        }
        self.sync();
    }

    /// Keep serials, surfaces, contact IDs and frames for protocol-authority tests.
    /// Down serials can also be passed to the existing move/resize/GTK methods.
    pub(crate) fn take_touch_events(&mut self) -> Vec<Event> {
        self.sync();
        std::mem::take(&mut self.state.touch.events)
    }

    /// Most recently observed down, retained after draining events or lifting.
    pub(crate) fn touch_down_serial(&mut self) -> u32 {
        self.sync();
        self.state
            .touch
            .down_serial
            .expect("client received native touch down")
    }

    pub(crate) fn touch_up_serial(&mut self) -> u32 {
        self.sync();
        self.state
            .touch
            .up_serial
            .expect("client received native touch up")
    }
}

impl Dispatch<wl_touch::WlTouch, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_touch::WlTouch,
        event: Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match &event {
            Event::Down { serial, .. } => state.touch.down_serial = Some(*serial),
            Event::Up { serial, .. } => state.touch.up_serial = Some(*serial),
            _ => {}
        }
        state.touch.events.push(event);
    }
}

fn fixture(inset: (i32, i32), scale: f64) -> (HeadlessCompositor, InteractiveClient) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect_with_inset(&socket, inset);
    for window in wait_windows(&compositor, |windows| windows.len() == 2) {
        assert_eq!(window["titlebar_height"], 0, "fixture must use CSD");
        let x = if window["title"] == "Interactive fixture 0" {
            100
        } else {
            900
        };
        action(&compositor, format!("move-window {} {x} 150", window["id"]));
    }
    action(
        &compositor,
        format!("focus-window {}", window(&compositor)["id"]),
    );
    action(&compositor, format!("zoom-to {scale}"));
    wait_scale(&compositor, scale);
    action(&compositor, "pan-to 0 0");
    client.take_pointer_events();
    (compositor, client)
}

fn window(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 0")
        .unwrap()
        .clone()
}

/// A point inside the visible xdg geometry, excluding its surface-buffer inset.
fn screen(compositor: &HeadlessCompositor, point: (f64, f64)) -> (f64, f64) {
    let window = window(compositor);
    let scale = query(compositor, "status")["viewport"]["scale"]
        .as_f64()
        .unwrap();
    (
        window["x"].as_f64().unwrap() + point.0 * scale,
        window["y"].as_f64().unwrap() + point.1 * scale,
    )
}

fn down(compositor: &HeadlessCompositor, slot: i32, point: (f64, f64)) {
    action(
        compositor,
        format!("touch-down client-touch {slot} {} {}", point.0, point.1),
    );
}

fn motion(compositor: &HeadlessCompositor, slot: i32, point: (f64, f64)) {
    action(
        compositor,
        format!("touch-motion client-touch {slot} {} {}", point.0, point.1),
    );
}

fn up(compositor: &HeadlessCompositor, slot: i32) {
    action(compositor, format!("touch-up client-touch {slot}"));
}

fn frame(compositor: &HeadlessCompositor) {
    action(compositor, "touch-frame client-touch");
}

fn assert_down(client: &mut InteractiveClient, slot: i32, point: (f64, f64)) -> u32 {
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Down { surface, id, x, y, .. }, Event::Frame]
            if surface == client.surface(0) && *id == slot && (*x, *y) == point),
        "expected native down for slot {slot} at {point:?}, got {events:?}",
    );
    client.touch_down_serial()
}

fn assert_no_pointer_buttons(client: &mut InteractiveClient) {
    assert_buttons(&client.take_pointer_events(), &[]);
}

fn assert_touch_silent(client: &mut InteractiveClient) {
    let mut events = client.take_touch_events();
    events.retain(|event| !matches!(event, Event::Frame));
    assert!(events.is_empty(), "stale touch input escaped: {events:?}");
    assert_no_pointer_buttons(client);
}

fn assert_cancel(client: &mut InteractiveClient) {
    let mut events = client.take_touch_events();
    events.retain(|event| !matches!(event, Event::Frame));
    assert!(
        matches!(events.as_slice(), [Event::Cancel]),
        "expected one cancel without a second down or synthesized up: {events:?}",
    );
    assert_no_pointer_buttons(client);
}

fn fresh_tap(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    inset: (i32, i32),
) -> u32 {
    let local = (60.5, 44.25);
    down(compositor, 0, screen(compositor, local));
    frame(compositor);
    let serial = assert_down(
        client,
        0,
        (local.0 + f64::from(inset.0), local.1 + f64::from(inset.1)),
    );
    up(compositor, 0);
    frame(compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Up { id: 0, .. }, Event::Frame]),
        "fresh tap must finish once: {events:?}",
    );
    assert_ne!(client.touch_up_serial(), serial);
    assert_no_pointer_buttons(client);
    serial
}
