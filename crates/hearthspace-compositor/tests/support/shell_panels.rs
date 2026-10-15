//! Real xdg surfaces exercise shell palette geometry and dismissal policy.
use super::canvas_map_client::MapClient;
use super::navigation::{action, query};
use super::*;
use hearthspace_common::{
    LAUNCHER_APP_ID, NOTIFICATIONS_APP_ID, POWER_MENU_APP_ID, shell_bar_app_id_for_output,
};
use serde_json::Value;

const GREEN: [u8; 4] = [0x66, 0xcc, 0, 0xff];

pub(super) fn connect(compositor: &HeadlessCompositor) -> MapClient {
    MapClient::connect(
        &compositor
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id())),
    )
}

fn bounds(compositor: &HeadlessCompositor, kind: &str) -> Value {
    query(compositor, "status")["shell_panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|panel| panel["kind"] == kind)
        .expect("mapped shell panel")
        .clone()
}

pub(super) fn click(compositor: &HeadlessCompositor, x: f64, y: f64) {
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    action(compositor, "pointer-button-up 272");
}

#[test]
fn shell_panels_focus_hit_test_and_close_without_revealing_off_canvas_windows() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.PanelTest".into(),
        (240, 160),
        (0, 0),
        [0xff, 0, 0, 0xff],
    );
    let bar = client.add(
        shell_bar_app_id_for_output(&output),
        (1280, 48),
        (0, 0),
        [0, 0, 0xff, 0xff],
    );
    let id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();

    for (app_id, kind, size) in [
        (LAUNCHER_APP_ID, "search", (500, 360)),
        (NOTIFICATIONS_APP_ID, "notifications", (360, 440)),
        (POWER_MENU_APP_ID, "power", (240, 160)),
    ] {
        let panel = client.add(app_id.into(), size, (8, 6), GREEN);
        assert_eq!(client.focus(), Some(panel), "{kind} initial focus");
        let geometry = bounds(&compositor, kind);
        let x = geometry["x"].as_f64().unwrap();
        let y = geometry["y"].as_f64().unwrap();
        let width = geometry["width"].as_f64().unwrap();
        assert!(y >= 48.0 && y + f64::from(size.1) <= 800.0);
        if kind == "search" {
            assert_eq!(x + width / 2.0, 640.0, "search must be centered");
        } else {
            assert_eq!(x + width, 1268.0, "dropdown must anchor on the right");
        }
        click(&compositor, x + 20.0, y + 20.0);
        assert_eq!(client.presses().last(), Some(&(panel, (28.0, 26.0))));
        click(&compositor, 10.0, 20.0);
        assert_eq!(client.focus(), Some(bar));
        assert!(
            !client.closed().contains(&panel),
            "toggle press must keep panel open"
        );
        action(&compositor, "pan-to 9000 -7000");
        let viewport = query(&compositor, "status")["viewport"].clone();
        assert_eq!(bounds(&compositor, kind), geometry);
        action(&compositor, "key-down 1");
        assert!(client.closed().contains(&panel), "Escape dismisses {kind}");
        client.destroy(panel);
        assert_eq!(client.focus(), Some(normal));
        action(&compositor, "key-up 1");
        assert!(
            client.keys().is_empty(),
            "Escape must not leak to restored client"
        );
        assert_eq!(query(&compositor, "status")["viewport"], viewport);

        let panel = client.add(app_id.into(), size, (0, 0), GREEN);
        click(&compositor, 10.0, 20.0);
        client.destroy(panel); // Shell's active toolbar toggle removes its panel.
        assert_eq!(
            client.focus(),
            Some(normal),
            "{kind} toggle restores application focus"
        );
        assert_eq!(query(&compositor, "status")["viewport"], viewport);

        let panel = client.add(app_id.into(), size, (0, 0), GREEN);
        click(&compositor, 10.0, 750.0);
        assert!(
            client.closed().contains(&panel),
            "outside click dismisses {kind}"
        );
        assert_eq!(client.focus(), None);
        client.destroy(panel);
        assert_eq!(
            client.focus(),
            None,
            "outside click focus must be preserved"
        );

        let panel = client.add(app_id.into(), size, (0, 0), GREEN);
        action(&compositor, format!("focus-window {id}"));
        assert!(
            client.closed().contains(&panel),
            "client activation dismisses {kind}"
        );
        client.destroy(panel);
        assert_eq!(client.focus(), Some(normal));
        assert_eq!(
            query(&compositor, "windows")["windows"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn search_and_dropdowns_fit_scaled_small_outputs_and_lock_hides_their_metadata() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_options("640x480", None, None, 2);
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    for (app_id, kind) in [
        (LAUNCHER_APP_ID, "search"),
        (NOTIFICATIONS_APP_ID, "notifications"),
        (POWER_MENU_APP_ID, "power"),
    ] {
        let panel = client.add(app_id.into(), (500, 440), (0, 0), GREEN);
        let geometry = bounds(&compositor, kind);
        assert_eq!(geometry["x"], 0);
        assert_eq!(geometry["y"], 48);
        assert_eq!(geometry["width"], 320);
        assert_eq!(geometry["height"], 192);
        click(&compositor, 100.0, 100.0);
        assert_eq!(client.presses().last(), Some(&(panel, (100.0, 52.0))));
        let socket = compositor
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id()));
        let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
        locker.wait_locked();
        assert!(client.closed().contains(&panel));
        assert_eq!(
            query(&compositor, "status")["shell_panels"],
            serde_json::json!([])
        );
        client.destroy(panel);
        locker.unlock();
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn search_bridge_preserves_early_typing_and_revokes_cancelled_requests() {
    use std::os::unix::net::UnixDatagram;

    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let request = || {
        action(&compositor, "focus-launcher");
        let mut bytes = [0; 4096];
        let length = events.recv(&mut bytes).unwrap();
        let event: Value = serde_json::from_slice(&bytes[..length]).unwrap();
        assert_eq!(event["action"], "open-search");
        event["token"].as_u64().unwrap()
    };
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.SearchInputTest".into(),
        (240, 160),
        (0, 0),
        [0xff, 0, 0, 0xff],
    );
    client.add(
        hearthspace_common::SHELL_BAR_APP_ID.into(),
        (1280, 48),
        (0, 0),
        [0, 0, 0xff, 0xff],
    );
    let token = request();
    // Deliberately type before the shell has acknowledged or mapped its input.
    super::navigation::keys(&compositor, &[30]);
    super::navigation::keys(&compositor, &[18]);
    assert!(
        client.keys().is_empty(),
        "early typing must not reach the old client or bar"
    );
    action(&compositor, format!("shell-event-accept {token}"));
    let search = client.add(
        hearthspace_common::launcher_app_id_for_request(None, token),
        (500, 300),
        (0, 0),
        GREEN,
    );
    assert_eq!(client.focus(), Some(search));
    assert_eq!(client.keys(), [30, 30, 18, 18]);
    action(&compositor, "key-down 1");
    client.destroy(search);
    action(&compositor, "key-up 1");
    assert_eq!(client.focus(), Some(normal));

    let delivered = client.keys().to_vec();
    let cancelled = request();
    action(&compositor, "key-down 30");
    click(&compositor, 10.0, 750.0);
    action(&compositor, "key-up 30");
    assert!(
        compositor
            .send_text_command(&format!("shell-event-accept {cancelled}"))
            .starts_with("err ")
    );
    assert_eq!(
        client.keys(),
        delivered,
        "cancellation cannot leak queued keys or their releases"
    );

    let accepted = request();
    action(&compositor, format!("shell-event-accept {accepted}"));
    click(&compositor, 10.0, 750.0);
    let stale = client.add(
        hearthspace_common::launcher_app_id_for_request(None, accepted),
        (500, 300),
        (0, 0),
        GREEN,
    );
    assert!(
        client.closed().contains(&stale),
        "outside click must revoke an accepted late map"
    );
    assert_eq!(
        client.focus(),
        None,
        "stale map must not regain keyboard focus"
    );
    client.destroy(stale);
    // A cancelled bridge request must not block a subsequent direct icon open.
    let direct = client.add(LAUNCHER_APP_ID.into(), (500, 300), (0, 0), GREEN);
    assert_eq!(client.focus(), Some(direct));
    assert!(!client.closed().contains(&direct));
    client.destroy(direct);

    let before_lock = request();
    action(&compositor, format!("shell-event-accept {before_lock}"));
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    // A surface may reach the compositor after its open request was accepted.
    let late = client.add(
        hearthspace_common::launcher_app_id_for_request(None, before_lock),
        (500, 300),
        (0, 0),
        GREEN,
    );
    assert!(
        client.closed().contains(&late),
        "lock must revoke a late-mapped search"
    );
    assert_eq!(
        query(&compositor, "status")["shell_panels"],
        serde_json::json!([])
    );
    client.destroy(late);
    locker.unlock();
    assert!(
        compositor
            .send_text_command(&format!("shell-event-accept {before_lock}"))
            .starts_with("err ")
    );
    assert!(
        compositor
            .send_text_command("reset-view")
            .starts_with("err ")
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
