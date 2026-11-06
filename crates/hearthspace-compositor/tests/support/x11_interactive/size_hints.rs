//! Cell-sized X11 buffers and the canvas's opposite-edge resize anchor.
use super::*;
use smithay::reexports::x11rb::{
    protocol::xproto::{AtomEnum, ConnectionExt, PropMode},
    wrapper::ConnectionExt as _,
};

fn hints(compositor: &HeadlessCompositor, client: &mut Client, values: &[u32]) {
    client
        .connection
        .change_property32(
            PropMode::REPLACE,
            client.window,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            values,
        )
        .unwrap()
        .check()
        .unwrap();
    client.barrier(compositor);
}

#[test]
fn x11_terminal_cell_resize_preserves_opposite_edges_and_output_maximization() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    let mut values = [0; 18];
    values[0] = 16 | 64 | 256;
    values[5..7].copy_from_slice(&[260, 120]);
    values[9..11].copy_from_slice(&[9, 17]);
    values[15..17].copy_from_slice(&[5, 2]);
    hints(&compositor, &mut client, &values);
    wait_geometry(&compositor, &mut client, [100, 150, 320, 240], RED);

    let point = press(&compositor, &mut client);
    client.request(&compositor, 4, 1, point);
    motion(
        &compositor,
        i32::from(point.0) + 54,
        i32::from(point.1) + 36,
    );
    action(&compositor, "pointer-button-up 272");
    wait_geometry(&compositor, &mut client, [100, 150, 374, 274], RED);
    let actual = client
        .connection
        .get_geometry(client.window)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!([actual.width, actual.height], [374, 274]);

    let point = press(&compositor, &mut client);
    client.request(&compositor, 0, 1, point);
    motion(
        &compositor,
        i32::from(point.0) - 33,
        i32::from(point.1) - 42,
    );
    action(&compositor, "pointer-button-up 272");
    wait_geometry(&compositor, &mut client, [64, 116, 410, 308], RED);
    let actual = client
        .connection
        .get_geometry(client.window)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!([actual.width, actual.height], [410, 308]);

    // Output-sized states must not leave a cell-sized gap at the display edge.
    action(&compositor, format!("maximize-window {} true", client.id));
    let window = wait_windows(&compositor, |windows| {
        windows.iter().any(|w| {
            w["id"] == client.id
                && w["maximized"] == true
                && w["content_width"] == w["maximized_canvas_bounds"]["width"]
                && w["content_height"] == w["maximized_canvas_bounds"]["height"]
        })
    })
    .into_iter()
    .find(|w| w["id"] == client.id)
    .unwrap();
    client.barrier(&compositor);
    let actual = client
        .connection
        .get_geometry(client.window)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(
        u64::from(actual.width),
        window["maximized_canvas_bounds"]["width"]
    );
    assert_eq!(
        u64::from(actual.height),
        window["maximized_canvas_bounds"]["height"]
    );
    assert_ne!(
        (actual.width - 5) % 9,
        0,
        "fixture must distinguish grid rounding"
    );

    action(&compositor, format!("fullscreen-window {} true", client.id));
    wait_windows(&compositor, |windows| {
        windows.iter().any(|w| {
            w["id"] == client.id
                && w["fullscreen"] == true
                && w["content_width"] == 1280
                && w["content_height"] == 800
        })
    });
    client.barrier(&compositor);
    let actual = client
        .connection
        .get_geometry(client.window)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!([actual.width, actual.height], [1280, 800]);
}

#[test]
fn x11_truncated_or_invalid_size_hints_do_not_break_resize_or_input() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    hints(&compositor, &mut client, &[16 | 32 | 64 | 256]);
    responsive_click(&compositor, &mut client);
    let mut values = [0; 18];
    values[0] = 16 | 32 | 64 | 256;
    values[5..7].copy_from_slice(&[u32::MAX, 0]);
    values[7..11].copy_from_slice(&[0, u32::MAX, u32::MAX, 0]);
    values[15..17].copy_from_slice(&[u32::MAX, u32::MAX]);
    hints(&compositor, &mut client, &values);
    let point = press(&compositor, &mut client);
    client.request(&compositor, 4, 1, point);
    motion(
        &compositor,
        i32::from(point.0) + 54,
        i32::from(point.1) + 36,
    );
    action(&compositor, "pointer-button-up 272");
    wait_geometry(&compositor, &mut client, [100, 150, 374, 276], GREEN);
    responsive_click(&compositor, &mut client);
}
