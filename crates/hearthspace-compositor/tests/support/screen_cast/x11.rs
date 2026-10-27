//! Pixel evidence distinguishes X root placement from the local backing buffer.
use super::super::navigation::wait_windows;
use super::*;
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::xproto::{
        AtomEnum, ConnectionExt, CreateGCAux, CreateWindowAux, PropMode, Rectangle, WindowClass,
    },
    wrapper::ConnectionExt as _,
};

fn wait_pixels(client: &mut Client, session: &client::Session, expected: impl Fn(i32, i32) -> u32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let capture = client.capture(session);
        if capture
            .pixels()
            .iter()
            .enumerate()
            .all(|(i, p)| *p == expected(i as i32 % capture.size.0, i as i32 / capture.size.0))
        {
            return;
        }
        if Instant::now() >= deadline {
            assert_pixels(&capture, &expected);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn x11_window_capture_at_nonzero_root_position_has_exact_local_pixels_and_owned_menu() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(display) = query(&compositor, "status")["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline, "Xwayland startup timed out");
        thread::sleep(Duration::from_millis(20));
    };
    let (connection, screen_index) = x11rb::connect(Some(&display)).unwrap();
    let screen = &connection.setup().roots[screen_index];
    let window = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            window,
            screen.root,
            123,
            87,
            320,
            200,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new().background_pixel(0xff0000),
        )
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"X11 capture at a nonzero origin",
        )
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"capture-x11\0CaptureX11\0",
        )
        .unwrap();
    connection.map_window(window).unwrap();
    connection.flush().unwrap();
    let windows = wait_windows(&compositor, |w| w.len() == 1 && w[0]["mapped"] == true);
    let id = windows[0]["id"].as_u64().unwrap();
    action(&compositor, format!("move-window {id} 300 200"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let origin = loop {
        let origin = connection
            .translate_coordinates(window, screen.root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        if origin.dst_x == 300 && origin.dst_y == 200 {
            break origin;
        }
        assert!(
            Instant::now() < deadline,
            "nonzero root placement never applied"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let gc = connection.generate_id().unwrap();
    connection
        .create_gc(gc, window, &CreateGCAux::new().foreground(0x0000ff))
        .unwrap();
    connection
        .poly_fill_rectangle(
            window,
            gc,
            &[Rectangle {
                x: 250,
                y: 160,
                width: 50,
                height: 20,
            }],
        )
        .unwrap();
    connection.flush().unwrap();
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("CaptureX11");
    let session = client.session(&handle, false);
    assert_eq!(client.state.sessions[&session.id].size, Some(SIZE));
    let base = |x, y| {
        if (250..300).contains(&x) && (160..180).contains(&y) {
            0xff0000ff
        } else {
            0xffff0000
        }
    };
    wait_pixels(&mut client, &session, base);

    let popup = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            popup,
            screen.root,
            origin.dst_x + 10,
            origin.dst_y + 20,
            30,
            20,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0x00ff00)
                .override_redirect(1),
        )
        .unwrap();
    connection
        .change_property32(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[window],
        )
        .unwrap();
    connection.map_window(popup).unwrap();
    connection.flush().unwrap();
    wait_pixels(&mut client, &session, |x, y| {
        if (10..40).contains(&x) && (20..40).contains(&y) {
            0xff00ff00
        } else {
            base(x, y)
        }
    });
    client.sync();
    assert_eq!(
        client.state.toplevels.iter().filter(|t| !t.closed).count(),
        1,
        "override-redirect menus are not selectable toplevels"
    );
    connection.destroy_window(popup).unwrap();
    connection.flush().unwrap();
    action(&compositor, "zoom-to 0.5");
    action(&compositor, format!("move-window {id} 1400 -600"));
    wait_pixels(&mut client, &session, base);
    assert_eq!(client.state.sessions[&session.id].size, Some(SIZE));

    // GTK frame extents are local insets, not global coordinates either.
    let extents = connection
        .intern_atom(false, b"_GTK_FRAME_EXTENTS")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            extents,
            AtomEnum::CARDINAL,
            &[4, 6, 8, 10],
        )
        .unwrap();
    connection.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        client.sync();
        if client.state.sessions[&session.id].size == Some((310, 182)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "CSD geometry constraints did not update"
        );
        thread::sleep(Duration::from_millis(20));
    }
    wait_pixels(&mut client, &session, |x, y| base(x + 4, y + 8));
    connection.destroy_window(window).unwrap();
    connection.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !client.state.sessions[&session.id].stopped {
        client.sync();
        assert!(
            Instant::now() < deadline,
            "X11 close did not stop its capture session"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
