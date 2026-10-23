//! Cancel before XWayland commits the resized buffer, then verify its anchor.
use super::{client::Crossing, *};
use smithay::reexports::x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, EventMask, Window,
};

struct DeferredResize {
    windows: [(Window, EventMask); 2],
}

impl DeferredResize {
    fn begin(client: &Client) -> Self {
        let frame = client
            .connection
            .query_tree(client.window)
            .unwrap()
            .reply()
            .unwrap()
            .parent;
        assert_ne!(frame, client.root, "fixture must have a managed X11 frame");
        // ResizeRedirect is public X11: ConfigureWindow emits ResizeRequest
        // instead of changing size when another connection selects this mask.
        // Hold both windows so XWayland cannot produce a resized buffer yet.
        // Unlike _XWAYLAND_ALLOW_COMMITS, this needs no privileged WM access.
        let windows = [frame, client.window].map(|window| {
            let mask = client
                .connection
                .get_window_attributes(window)
                .unwrap()
                .reply()
                .unwrap()
                .your_event_mask;
            client
                .connection
                .change_window_attributes(
                    window,
                    &ChangeWindowAttributesAux::new().event_mask(mask | EventMask::RESIZE_REDIRECT),
                )
                .unwrap()
                .check()
                .unwrap();
            (window, mask)
        });
        Self { windows }
    }

    fn wait_requested(&self, client: &mut Client, expected: [i32; 2]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            client.pump();
            if self.windows.iter().all(|(window, _)| {
                client.resize_requests.iter().any(|event| {
                    event.window == *window
                        && [i32::from(event.width), i32::from(event.height)] == expected
                })
            }) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "X11 resize was not requested for both windows: expected={expected:?}, events={:?}",
                client.resize_requests
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn apply(self, compositor: &HeadlessCompositor, client: &mut Client, size: [i32; 2]) {
        for (window, mask) in self.windows {
            client
                .connection
                .change_window_attributes(
                    window,
                    &ChangeWindowAttributesAux::new().event_mask(mask),
                )
                .unwrap()
                .check()
                .unwrap();
        }
        // A normal client ConfigureWindow request goes through the real XWM.
        // The pending resize must survive until its resulting buffer commits.
        client
            .connection
            .configure_window(
                client.window,
                &ConfigureWindowAux::new()
                    .width(size[0] as u32)
                    .height(size[1] as u32),
            )
            .unwrap();
        client.barrier(compositor);
    }
}

fn x11_size(client: &Client, window: Window) -> [i32; 2] {
    let geometry = client
        .connection
        .get_geometry(window)
        .unwrap()
        .reply()
        .unwrap();
    [i32::from(geometry.width), i32::from(geometry.height)]
}

fn cancel_before_buffer_commit(direction: u32, expected: [i32; 4]) {
    let (compositor, mut client) = fixture();
    let initial = geometry(&compositor, client.id);
    let point = press(&compositor, &mut client);
    let deferred = DeferredResize::begin(&client);
    client.request(&compositor, direction, 1, point);
    client.wait_crossing(Crossing::Leave);
    client.clear_events();
    motion(
        &compositor,
        i32::from(point.0) - 40,
        i32::from(point.1) - 30,
    );
    // Observe actual WM resize requests, keeping their geometry unapplied.
    let size = [expected[2], expected[3]];
    deferred.wait_requested(&mut client, size);
    client.request(&compositor, 11, 0, (0, 0));
    assert_eq!(
        geometry(&compositor, client.id),
        initial,
        "cancellation must precede the resized buffer commit"
    );
    for (window, _) in deferred.windows {
        assert_eq!(x11_size(&client, window), [initial[2], initial[3]]);
    }
    let requests_before_motion = client.resize_requests.len();
    motion(&compositor, 700, 550);
    client.barrier(&compositor);
    assert_eq!(
        client.resize_requests.len(),
        requests_before_motion,
        "canceled resize kept configuring"
    );
    assert_eq!(client.motions, 0, "held drag motion reached the app");
    assert!(
        !client.crossings.contains(&Crossing::Enter),
        "cancellation restored pointer focus before the held sequence drained"
    );

    deferred.apply(&compositor, &mut client, size);
    wait_geometry(&compositor, &mut client, expected, RED);
    let [x, y, width, height] = geometry(&compositor, client.id);
    assert_eq!(
        [x + width, y + height],
        [initial[0] + initial[2], initial[1] + initial[3]],
        "late resized buffer moved the opposite edges"
    );
    action(&compositor, "pointer-button-up 272");
    motion(&compositor, 710, 560);
    client.barrier(&compositor);
    assert!(
        !client.crossings.contains(&Crossing::Enter),
        "release restored stale pointer focus outside the app"
    );
    motion(&compositor, expected[0] + 60, expected[1] + 20);
    client.wait_crossing(Crossing::Enter);
    // XWayland repairs its held-button state on re-entry, not on Wayland leave.
    // Drain that release before checking the next independent click.
    client.wait_release();
    responsive_click(&compositor, &mut client);
    wait_geometry(&compositor, &mut client, expected, GREEN);
}

#[test]
fn x11_csd_top_resize_cancel_keeps_bottom_edge_after_late_buffer() {
    let _guard = headless_test_guard();
    cancel_before_buffer_commit(1, [100, 120, 320, 270]);
}

#[test]
fn x11_csd_left_resize_cancel_keeps_right_edge_after_late_buffer() {
    let _guard = headless_test_guard();
    cancel_before_buffer_commit(7, [60, 150, 360, 240]);
}

#[test]
fn x11_csd_top_left_resize_cancel_keeps_opposite_corner_after_late_buffer() {
    let _guard = headless_test_guard();
    cancel_before_buffer_commit(0, [60, 120, 360, 270]);
}
