//! Temporary admission retains the selected tool and the initiating button's lifetime.
use super::*;
use std::{io::ErrorKind, os::unix::net::UnixDatagram};

#[rstest::rstest]
#[case::select("select")]
#[case::search_armed("search")]
#[case::group_armed("group")]
fn meta_pan_retains_tool_and_window_geometry_after_meta_releases_first(#[case] selected: &str) {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, format!("canvas-tool {selected}"));
    let before = query(&compositor, "windows")["windows"].clone();
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 530.));
    assert_eq!(offset(&compositor), [-50, -30]);
    assert_eq!(tool(&compositor), selected);
    action(&compositor, "key-up 125");
    motion(&compositor, (600., 550.));
    assert_eq!(offset(&compositor), [-100, -50]);
    finish(&compositor);
    motion(&compositor, (650., 600.));
    assert_eq!(offset(&compositor), [-100, -50]);
    assert_eq!(tool(&compositor), selected);
    assert_eq!(query(&compositor, "canvas-search-status")["active"], false);
    assert_eq!(query(&compositor, "group-status")["lasso"], Value::Null);
    assert_silent(&mut client, "temporary pan sent pointer input to an app");
    let after = query(&compositor, "windows")["windows"].clone();
    for original in before.as_array().unwrap() {
        let current = after
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["id"] == original["id"])
            .unwrap();
        for field in [
            "canvas_x",
            "canvas_y",
            "content_width",
            "content_height",
            "focused",
        ] {
            assert_eq!(current[field], original[field], "pan changed {field}");
        }
    }
    if selected == "select" {
        client.take_keyboard_events();
        keys(&compositor, &[30]);
        assert_keys(&mut client, 30);
    }
}

#[rstest::rstest]
#[case::zoomed_out(0.8, [6340, 3640])]
#[case::zoomed_in(2., [5992, 3616])]
fn meta_pan_at_zoom_ignores_caps_lock(#[case] scale: f64, #[case] expected: [i64; 2]) {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, format!("zoom-to {scale}"));
    crate::canvas_zoom::wait_scale(&compositor, scale);
    action(&compositor, "pan-to 5760 3600");
    keys(&compositor, &[58]);
    action(&compositor, "key-down 125");
    start(&compositor, (640., 400.));
    client.take_pointer_events();
    motion(&compositor, (176., 368.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), expected);
    assert_eq!(viewport(&compositor)["scale"], scale);
    assert_eq!(viewport(&compositor)["animating"], false);
    assert_eq!(tool(&compositor), "select");
    assert_silent(&mut client, "zoomed temporary pan leaked client input");
}

#[test]
fn meta_pan_button_first_release_stops_motion_and_does_not_toggle_apps_list() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    let mut shell = crate::shell_panels::connect(&compositor);
    shell.add(
        hearthspace_common::SHELL_BAR_APP_ID.into(),
        (1280, 48),
        (0, 0),
        [40, 180, 100, 255],
    );
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events.set_nonblocking(true).unwrap();
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    finish(&compositor);
    motion(&compositor, (600., 600.));
    assert_eq!(offset(&compositor), [-50, -50]);
    action(&compositor, "key-up 125");
    assert_eq!(tool(&compositor), "select");
    assert_buttons(&client.take_pointer_events(), &[]);
    assert_eq!(
        crate::shell_events::next_action(&events)
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock,
        "pan's Meta release emitted a shell action"
    );
    // Prove the live launcher bridge works: a subsequent bare tap still opens it.
    keys(&compositor, &[125]);
    crate::shell_events::receive(&events, "open-search");
}

#[rstest::rstest]
#[case::control(29)]
#[case::alt(56)]
#[case::shift(42)]
fn meta_pan_requires_an_unmodified_meta_chord_at_the_fresh_press(#[case] extra: u32) {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, format!("key-down {extra}"));
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, format!("key-up {extra}"));
    motion(&compositor, (600., 600.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(
        offset(&compositor),
        [0, 0],
        "relaxing a chord adopted a held drag"
    );
    assert_eq!(tool(&compositor), "select");
    assert_buttons(&client.take_pointer_events(), &[]);
    action(&compositor, "key-down 125");
    start(&compositor, (600., 600.));
    motion(&compositor, (620., 620.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [-20, -20]);
}
