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
