//! Real X11 mapping and real XDG buffer geometry; no Steam process required.
use super::*;

fn window(
    c: &HeadlessCompositor,
    display: &str,
    fullscreen: bool,
) -> (RustConnection, Window, u64) {
    let (connection, screen) = x11rb::connect(Some(display)).unwrap();
    let screen = &connection.setup().roots[screen];
    let w = connection.generate_id().unwrap();
    let (width, height) = if fullscreen { (400, 300) } else { (1280, 800) };
    connection
        .create_window(
            screen.root_depth,
            w,
            screen.root,
            0,
            0,
            width,
            height,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0x20a060)
                .event_mask(EventMask::STRUCTURE_NOTIFY),
        )
        .unwrap()
        .check()
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            w,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"SteamFixture\0SteamFixture\0",
        )
        .unwrap();
    if fullscreen {
        let state = connection
            .intern_atom(false, b"_NET_WM_STATE")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let fs = connection
            .intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        connection
            .change_property32(PropMode::REPLACE, w, state, AtomEnum::ATOM, &[fs])
            .unwrap();
    }
    connection.map_window(w).unwrap();
    connection.flush().unwrap();
    let windows = wait_windows(c, |rows| {
        rows.iter()
            .any(|w| w["app_id"] == "SteamFixture" && w["mapped"] == true)
    });
    let id = windows
        .iter()
        .find(|w| w["app_id"] == "SteamFixture")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    (connection, w, id)
}
#[test]
fn steam_regressions_output_sized_window_does_not_request_fullscreen_or_block_canvas() {
    let _guard = headless_test_guard();
    let (c, display) = fixture();
    let (_connection, _w, id) = window(&c, &display, false);
    let windows = query(&c, "windows");
    let app = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == id)
        .unwrap();
    assert_eq!(app["fullscreen"], false);
    assert_eq!(app["titlebar_height"], 44);
    action(&c, "zoom-to 0.8");
    super::super::canvas_zoom::wait_scale(&c, 0.8);
    action(&c, format!("resize-window {id} 600 400"));
    wait_windows(&c, |rows| {
        rows.iter()
            .any(|w| w["id"] == id && w["content_width"] == 600 && w["content_height"] == 400)
    });
}
#[test]
fn steam_regressions_explicit_initial_fullscreen_is_still_honored_and_can_be_restored() {
    let _guard = headless_test_guard();
    let (c, display) = fixture();
    let (_connection, _w, id) = window(&c, &display, true);
    wait_windows(&c, |rows| {
        rows.iter()
            .any(|w| w["id"] == id && w["fullscreen"] == true)
    });
    action(&c, format!("fullscreen-window {id} false"));
    wait_windows(&c, |rows| {
        rows.iter()
            .any(|w| w["id"] == id && w["fullscreen"] == false)
    });
}
#[test]
fn steam_regressions_small_updater_geometry_cannot_leave_oversized_server_decorations() {
    use crate::interactive::client::InteractiveClient;
    let _guard = headless_test_guard();
    let (c, _display) = fixture();
    let socket = c
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect_server_chrome(&socket);
    let rows = wait_windows(&c, |rows| rows.len() == 2);
    let id = rows
        .iter()
        .find(|w| w["title"] == "Interactive fixture 0")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    client.set_raw_window_geometry(0, (0, 0, 640, 480));
    let rows = wait_windows(&c, |rows| {
        rows.iter()
            .any(|w| w["id"] == id && w["content_width"] == 240)
    });
    let app = rows.iter().find(|w| w["id"] == id).unwrap();
    assert_eq!(app["width"], 240);
    assert_eq!(app["height"], 204);
    assert_eq!(app["titlebar_height"], 44);
    client.set_raw_window_geometry(0, (10, 8, 200, 120));
    let rows = wait_windows(&c, |rows| {
        rows.iter()
            .any(|w| w["id"] == id && w["content_width"] == 200)
    });
    assert_eq!(
        rows.iter().find(|w| w["id"] == id).unwrap()["content_height"],
        120
    );
}
