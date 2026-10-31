//! X11 menus are root-positioned override-redirect windows, not managed dialogs.
use super::{
    navigation::{action, query, wait_windows},
    *,
};
use smithay::reexports::x11rb::{
    self,
    connection::Connection,
    protocol::{
        Event,
        xproto::{AtomEnum, ConnectionExt, CreateWindowAux, EventMask, PropMode, WindowClass},
    },
    wrapper::ConnectionExt as _,
};

fn red_bounds(compositor: &HeadlessCompositor) -> Option<(i32, i32, i32, i32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
    let mut reader = decoder.read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    let mut bounds = None;
    for (i, pixel) in data[..info.buffer_size()]
        .chunks_exact(info.color_type.samples())
        .enumerate()
    {
        if pixel[..3] != [220, 20, 60] {
            continue;
        }
        let x = (i % info.width as usize) as i32;
        let y = (i / info.width as usize) as i32;
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((left, top, right, bottom)) => {
                (x.min(left), y.min(top), x.max(right), y.max(bottom))
            }
        });
    }
    bounds.map(|(x, y, right, bottom)| (x, y, right - x + 1, bottom - y + 1))
}

fn wait_red(compositor: &HeadlessCompositor, expected: (i32, i32, i32, i32)) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = red_bounds(compositor);
        if actual.is_some_and(|a| {
            (a.0 - expected.0).abs() <= 1
                && (a.1 - expected.1).abs() <= 1
                && (a.2 - expected.2).abs() <= 2
                && (a.3 - expected.3).abs() <= 2
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "X11 popup pixels must follow parent content, expected {expected:?}, actual {actual:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn override_redirect_menu_follows_parent_content_through_pan_zoom_and_maximize() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(display) = query(&compositor, "status")["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    };
    let (connection, screen_index) = x11rb::connect(Some(&display)).unwrap();
    let screen = &connection.setup().roots[screen_index];
    let parent = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            parent,
            screen.root,
            80,
            100,
            500,
            300,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0x204080)
                .event_mask(EventMask::STRUCTURE_NOTIFY),
        )
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            parent,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Popup owner",
        )
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            parent,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"popup-fixture\0PopupFixture\0",
        )
        .unwrap();
    connection.map_window(parent).unwrap();
    connection.flush().unwrap();
    let initial = wait_windows(&compositor, |w| w.len() == 1 && w[0]["mapped"] == true);
    let id = initial[0]["id"].as_u64().unwrap();
    action(&compositor, format!("move-window {id} 300 200"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let origin = loop {
        let point = connection
            .translate_coordinates(parent, screen.root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        if (point.dst_x, point.dst_y) == (0, 0) {
            break point;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    };
    let popup = connection.generate_id().unwrap();
    connection
        .create_window(
            screen.root_depth,
            popup,
            screen.root,
            origin.dst_x + 10,
            origin.dst_y + 24,
            160,
            100,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0xdc143c)
                .override_redirect(1)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Popup menu",
        )
        .unwrap();
    connection
        .change_property32(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[parent],
        )
        .unwrap();
    connection.map_window(popup).unwrap();
    connection.flush().unwrap();
    wait_red(&compositor, (310, 268, 160, 100));
    assert_eq!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "menus must not enter window navigation"
    );
    // Property notifications must not turn a menu into a server-decorated app.
    connection
        .change_property8(
            PropMode::REPLACE,
            popup,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Updated menu",
        )
        .unwrap();
    connection.flush().unwrap();
    action(&compositor, "pan-to 100 80");
    wait_red(&compositor, (210, 188, 160, 100));
    action(&compositor, "zoom-to 0.5");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let v = query(&compositor, "status")["viewport"].clone();
        if v["animating"] == false && v["scale"].as_f64() == Some(0.5) {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    action(&compositor, "pan-to 100 80");
    wait_red(&compositor, (105, 94, 80, 50));
    action(&compositor, "pointer-motion-abs 125 109");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut pressed = false;
    loop {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::ButtonPress(e) if e.event == popup => {
                    assert!((e.event_x - 40).abs() <= 1 && (e.event_y - 30).abs() <= 1);
                    pressed = true;
                }
                Event::ButtonRelease(e) if e.event == popup => {
                    assert!(pressed);
                    break;
                }
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            Instant::now() < deadline,
            "menu must receive transformed click"
        );
    }
    connection.unmap_window(popup).unwrap();
    connection.flush().unwrap();
    action(&compositor, format!("maximize-window {id} true"));
    let max = wait_windows(&compositor, |w| {
        w[0]["maximized"] == true && w[0]["width"] == 1232 && w[0]["height"] == 608
    })[0]
        .clone();
    assert_eq!(max["x"], 24);
    assert_eq!(max["y"], 96);
    // Maximization changes the displayed content origin and size, while the
    // managed X backing stays at (0,0). Menu offsets remain client-local and
    // their displayed pixels still include the scaled compositor titlebar.
    let expected_origin = (0, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    let origin = loop {
        let origin = connection
            .translate_coordinates(parent, screen.root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        if (origin.dst_x, origin.dst_y) == expected_origin {
            break origin;
        }
        assert!(
            Instant::now() < deadline,
            "X11 parent must receive its maximized configure"
        );
        thread::sleep(Duration::from_millis(20));
    };
    connection
        .configure_window(
            popup,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .x(i32::from(origin.dst_x) + 10)
                .y(i32::from(origin.dst_y) + 24),
        )
        .unwrap();
    connection.map_window(popup).unwrap();
    connection.flush().unwrap();
    wait_red(
        &compositor,
        (
            max["x"].as_i64().unwrap() as i32 + 5,
            max["y"].as_i64().unwrap() as i32
                + max["titlebar_height"].as_i64().unwrap() as i32
                + 12,
            80,
            50,
        ),
    );
}
