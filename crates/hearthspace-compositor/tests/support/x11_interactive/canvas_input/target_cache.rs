//! Stationary scrolling must respect the visible target after foreign restacks.
//!
//! A foreign CirculateWindow(root) is intercepted by SubstructureRedirect as
//! CirculateRequest, even for an OR child. Our XWM does not implement that
//! request, so these integration cases do not cover the vendor's defensive
//! CirculateNotify cache invalidation.
use super::*;
use crate::remote_input::client::RemoteClient;
use smithay::reexports::x11rb::protocol::xproto::{MapState, WindowClass};

// Keep the existing batch fixture unchanged. Only the backend-axis case needs
// the real virtual-pointer protocol enabled to enter the physical input arm.
fn remote_fixture() -> Windows {
    let mut compositor =
        HeadlessCompositor::spawn_with_remote_options("1280x800", None, None, 1, true);
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
    assert_eq!(client.origin(), (0, 0));
    let origin = observer
        .translate_coordinates(other, client.root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!((origin.dst_x, origin.dst_y), (0, 0));
    let viewport = query(&compositor, "status")["viewport"].clone();
    assert_eq!(viewport["x"], 0);
    assert_eq!(viewport["y"], 0);
    assert_eq!(viewport["scale"], 1.0);
    Windows {
        compositor,
        client,
        observer,
        other,
        other_id,
        viewport,
    }
}

fn foreign_popup(windows: &Windows) -> Window {
    // Covers A's root point (60,20), but displays inside B at (640,300).
    let popup = window(&windows.observer, [40, 0], [120, 80], true);
    windows
        .observer
        .change_property32(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[windows.other],
        )
        .unwrap();
    popup
}

fn settle(windows: &Windows) {
    xwm_barrier(&windows.observer, windows.other);
}

fn hover_primary(windows: &mut Windows) {
    motion(&windows.compositor, 260, 220);
    settle(windows);
    windows.client.clear_events();
}

pub(super) fn assert_popup_stack(
    client: &Client,
    observer: &RustConnection,
    popup: Window,
    above_primary: bool,
) {
    let frame = observer
        .query_tree(client.window)
        .unwrap()
        .reply()
        .unwrap()
        .parent;
    assert_ne!(frame, client.root, "A must have a managed frame");
    let children = observer
        .query_tree(client.root)
        .unwrap()
        .reply()
        .unwrap()
        .children;
    let frame_index = children.iter().position(|id| *id == frame).unwrap();
    let popup_index = children.iter().position(|id| *id == popup).unwrap();
    // QueryTree orders siblings bottom-to-top, including unmapped WM helper
    // windows. Only the popup's ordering against A's real frame matters here.
    assert_eq!(
        popup_index > frame_index,
        above_primary,
        "popup {popup} vs A frame {frame}, root stack={children:?}"
    );
    let origin = client.origin();
    let point = (origin.0 + 60, origin.1 + 20);
    for window in [frame, popup] {
        let attributes = observer
            .get_window_attributes(window)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(attributes.map_state, MapState::VIEWABLE, "{window}");
        assert_eq!(attributes.class, WindowClass::INPUT_OUTPUT, "{window}");
        let geometry = observer.get_geometry(window).unwrap().reply().unwrap();
        let origin = observer
            .translate_coordinates(window, client.root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        let (x, y) = (i32::from(origin.dst_x), i32::from(origin.dst_y));
        assert!(
            (x..x + i32::from(geometry.width)).contains(&point.0)
                && (y..y + i32::from(geometry.height)).contains(&point.1),
            "viewable window {window} must cover A's input point {point:?}: origin=({x},{y}), geometry={geometry:?}"
        );
    }
}

fn assert_received(windows: &mut Windows, popup: Window, button: u8) {
    let mut primary = Vec::new();
    let mut foreign = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        drain_buttons(
            &windows.client.connection,
            windows.client.window,
            &mut primary,
        );
        drain_buttons(&windows.observer, popup, &mut foreign);
        assert!(foreign.is_empty(), "foreign popup stole input: {foreign:?}");
        if primary.iter().any(|(pressed, _)| !*pressed) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "A did not receive input: {primary:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
    // These roundtrips follow delivery; none can repair the pre-input stack.
    windows
        .client
        .connection
        .get_input_focus()
        .unwrap()
        .reply()
        .unwrap();
    windows.observer.get_input_focus().unwrap().reply().unwrap();
    drain_buttons(
        &windows.client.connection,
        windows.client.window,
        &mut primary,
    );
    drain_buttons(&windows.observer, popup, &mut foreign);
    assert!(
        foreign.is_empty(),
        "foreign popup received trailing input: {foreign:?}"
    );
    let origin = windows.client.origin();
    if button == 1 {
        assert_click(&primary, [60, 20], origin);
    } else {
        // Continuous axis units may produce several core X wheel clicks.
        // Every delivered wheel event must form a balanced pair on A only.
        let pairs = primary.chunks_exact(2);
        assert!(
            !primary.is_empty() && pairs.remainder().is_empty(),
            "{primary:?}"
        );
        for pair in pairs {
            assert!(pair[0].0 && !pair[1].0, "{pair:?}");
            for (_, event) in pair {
                assert_eq!(event.detail, button);
                assert!(event.same_screen);
                assert_eq!((event.event_x, event.event_y), (60, 20));
                assert_eq!(
                    (i32::from(event.root_x), i32::from(event.root_y)),
                    (origin.0 + 60, origin.1 + 20)
                );
            }
        }
    }
    assert_eq!(
        geometry(&windows.compositor, windows.client.id),
        [200, 200, 320, 240]
    );
    assert_eq!(
        geometry(&windows.compositor, windows.other_id),
        [600, 300, 320, 240]
    );
    assert_eq!(
        query(&windows.compositor, "status")["viewport"],
        windows.viewport
    );
}

fn stationary_axis(backend: bool) {
    let _guard = headless_test_guard();
    let mut windows = if backend { remote_fixture() } else { fixture() };
    let mut remote = backend.then(|| {
        RemoteClient::connect(
            &windows
                .compositor
                .runtime_dir
                .path()
                .join(format!("hearthspace-test-{}", std::process::id())),
        )
    });
    let popup = foreign_popup(&windows);
    for phase in ["map", "restack"] {
        hover_primary(&mut windows);
        if phase == "map" {
            windows.observer.map_window(popup).unwrap();
        } else {
            windows
                .observer
                .configure_window(
                    popup,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )
                .unwrap();
        }
        settle(&windows);
        wait_pixel(&windows.compositor, 650, 310, GREEN);
        assert_popup_stack(&windows.client, &windows.observer, popup, true);
        // No motion or button request after the foreign stack change. The
        // virtual-pointer axis enters the physical InputEvent::PointerAxis arm.
        if let Some(remote) = &mut remote {
            remote.axis(0.0, -120.0);
        } else {
            action(&windows.compositor, "axis 0 -120");
        }
        assert_received(&mut windows, popup, 4);
    }
}

#[test]
fn x11_stationary_synthetic_axis_ignores_foreign_popup_map_and_restack() {
    stationary_axis(false);
}

#[test]
fn x11_stationary_backend_axis_ignores_foreign_popup_map_and_restack() {
    stationary_axis(true);
}
