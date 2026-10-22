//! Compositor-owned controls never initialize hidden client decoration frames.
use super::navigation::{action, query};
use super::shell_panels::connect;
use super::*;
use hearthspace_common::{
    NOTIFICATIONS_APP_ID, SHELL_OUTPUT_WATCHER_APP_ID, ShellCorner, desktop_item_app_id,
    shell_corner_app_id,
};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;

#[test]
fn native_shell_surfaces_negotiate_compositor_decoration_without_application_titlebars() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let color = [40, 180, 110, 255];
    for (app_id, size) in [
        (SHELL_OUTPUT_WATCHER_APP_ID.to_owned(), (1, 1)),
        (desktop_item_app_id("app:startup.desktop"), (100, 108)),
        (shell_corner_app_id(ShellCorner::Apps, None), (150, 54)),
        (NOTIFICATIONS_APP_ID.to_owned(), (390, 280)),
    ] {
        let window = client.add_decorated(app_id.clone(), size, color, Mode::ClientSide);
        assert_eq!(client.decoration_mode(window), Mode::ServerSide, "{app_id}");
        assert_eq!(
            client.size(window),
            size,
            "no native frame geometry for {app_id}"
        );
        client.request_decoration(window, Mode::ClientSide);
        assert_eq!(client.decoration_mode(window), Mode::ServerSide, "{app_id}");
    }

    for (app_id, mode, titlebar_height) in [
        ("dev.hearthspace.NormalCsdTest", Mode::ClientSide, 0),
        ("dev.hearthspace.NormalSsdTest", Mode::ServerSide, 44),
    ] {
        let window = client.add_decorated(app_id.into(), (320, 200), color, mode);
        assert_eq!(client.decoration_mode(window), mode);
        let status = query(&compositor, "windows");
        let mapped = status["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["app_id"] == app_id)
            .unwrap();
        assert_eq!(mapped["titlebar_height"], titlebar_height);
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
