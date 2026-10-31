//! XRes credentials supply identity when bootstrap clients omit WM_CLASS.
use super::*;
use smithay::reexports::x11rb::{
    protocol::xproto::{AtomEnum, PropMode},
    wrapper::ConnectionExt as _,
};

#[test]
fn x11_bootstrap_identity_uses_client_process_then_follows_wm_class() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    // Spoof before the first map, so an implementation trusting this property
    // instead of XRes credentials cannot accidentally pass through its cache.
    let mut client = Client::connect_with_identity(&compositor, None, Some(1));
    let executable = std::env::current_exe().unwrap();
    let executable = executable.file_name().unwrap().to_str().unwrap();
    let window = query(&compositor, "windows")["windows"][0].clone();
    assert_eq!(window["app_id"], executable);
    assert_eq!(window["title"], "X11 CSD fixture");

    for (class, expected) in [
        (b"bootstrap-instance\0\0".as_slice(), "bootstrap-instance"),
        (
            b"bootstrap-instance\0ExplicitClass\0".as_slice(),
            "ExplicitClass",
        ),
        (b"\0\0".as_slice(), executable),
    ] {
        client
            .connection
            .change_property8(
                PropMode::REPLACE,
                client.window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                class,
            )
            .unwrap();
        client.barrier(&compositor);
        wait_windows(&compositor, |windows| windows[0]["app_id"] == expected);
    }
}

#[test]
fn x11_cached_frame_syntax_follows_current_class_instance_and_executable_fallback() {
    use hearthspace_common::frame_layout::frame_widget_app_id;

    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = Client::connect_with_identity(&compositor, None, None);
    let initial = query(&compositor, "windows")["windows"][0].clone();
    let output = query(&compositor, "layout-status")["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let wifi = frame_widget_app_id(Some(&output), "wifi");
    let clock = frame_widget_app_id(Some(&output), "clock");

    for (instance, class, target) in [
        (wifi.as_str(), clock.as_str(), Some("clock")),
        (clock.as_str(), wifi.as_str(), Some("wifi")),
        (clock.as_str(), "", Some("clock")),
        ("", "", None),
    ] {
        let property = format!("{instance}\0{class}\0");
        client
            .connection
            .change_property8(
                PropMode::REPLACE,
                client.window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                property.as_bytes(),
            )
            .unwrap();
        client.barrier(&compositor);
        // XWM metadata and layout measurement settle on separate event-loop
        // passes. Observe the real projected target after both have caught up.
        wait_windows(&compositor, |windows| {
            let window = &windows[0];
            assert_eq!(window["id"], initial["id"]);
            if let Some(target) = target {
                let state = query(&compositor, "layout-status");
                let widget = state["outputs"][0]["widgets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|widget| widget["id"] == target)
                    .unwrap();
                window["app_id"] == frame_widget_app_id(Some(&output), target)
                    && window["x"].as_f64().unwrap()
                        == widget["rect"]["x"].as_f64().unwrap().round()
                    && window["y"].as_f64().unwrap()
                        == widget["rect"]["y"].as_f64().unwrap().round()
            } else {
                window["app_id"] == initial["app_id"]
                    && window["x"] == initial["x"]
                    && window["y"] == initial["y"]
            }
        });
    }
}
