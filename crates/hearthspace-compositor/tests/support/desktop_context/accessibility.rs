//! An assistive menu action may focus only its validated, visible shell source.
use super::*;

fn dock(
    compositor: &HeadlessCompositor,
    client: &mut MapClient,
    normal: usize,
) -> (usize, u64, Value) {
    action(
        compositor,
        "desktop-app-register {\"offset\":0,\"ids\":[\"test.desktop\"]}",
    );
    let output = query(compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let app_id = hearthspace_common::frame_layout::frame_widget_app_id(Some(&output), "apps");
    let dock = client.add(app_id.clone(), (300, 54), (0, 0), [0, 255, 0, 255]);
    let normal_id = query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["app_id"] == "context-app")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    action(compositor, format!("focus-window {normal_id}"));
    assert_eq!(client.focus(), Some(normal));
    (
        dock,
        normal_id,
        json!({
            "app_id":"test.desktop", "output_name":output, "source_app_id":app_id,
            "popup_epoch":query(compositor, "layout-status")["popup_epoch"],
            "intent":"accessibility",
            "anchor":{"x":10.,"y":10.,"width":40.,"height":30.}
        }),
    )
}

#[test]
fn dock_accessibility_context_focuses_visible_source_and_restores_it_after_close() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, normal, _) = fixture();
    let (dock, _, request) = dock(&compositor, &mut client, normal);
    // Default and explicitly ordinary requests cannot acquire an unfocused dock.
    let mut ordinary = request.clone();
    ordinary.as_object_mut().unwrap().remove("intent");
    rejected(&compositor, &ordinary);
    assert_eq!(client.focus(), Some(normal));
    ordinary["intent"] = json!("ordinary");
    rejected(&compositor, &ordinary);
    assert_eq!(client.focus(), Some(normal));

    action(&compositor, command(&request));
    assert_eq!(
        client.focus(),
        Some(dock),
        "reserve source focus before the menu maps"
    );
    let menu = context(&compositor);
    assert_eq!(menu["ids"], json!(["app:test.desktop"]));
    let surface = map_menu(&mut client, &menu);
    assert_eq!(client.focus(), Some(surface));
    assert!(!client.closed().contains(&dock));
    close(&compositor, &menu);
    assert_eq!(client.focus(), Some(dock));
    client.destroy(surface);
    assert_eq!(client.focus(), Some(dock));
    assert!(!client.closed().contains(&normal));
    // Once the source actually owns focus, ordinary input still opens normally.
    action(&compositor, command(&ordinary));
    close(&compositor, &context(&compositor));
    finish(&mut compositor);
}

#[test]
fn dock_accessibility_context_rejects_stale_hidden_and_unmapped_sources_without_focus_changes() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, normal, _) = fixture();
    let (dock, normal_id, mut request) = dock(&compositor, &mut client, normal);
    for (field, value) in [
        ("source_app_id", json!("context-app")),
        ("output_name", json!("missing-output")),
        ("popup_epoch", json!(u64::MAX)),
        ("intent", json!("focus")),
        (
            "anchor",
            json!({"x":900.,"y":900.,"width":40.,"height":30.}),
        ),
    ] {
        let mut invalid = request.clone();
        invalid[field] = value;
        rejected(&compositor, &invalid);
        assert_eq!(client.focus(), Some(normal));
    }
    // Mapping is insufficient: fullscreen hides the dock, and an accessibility
    // action must not make it visible merely by taking focus first.
    action(&compositor, format!("fullscreen-window {normal_id} true"));
    client.sync();
    request["popup_epoch"] = query(&compositor, "layout-status")["popup_epoch"].clone();
    let reply = compositor.send_text_command(&command(&request));
    assert!(reply.contains("not visible"), "{reply}");
    assert!(context(&compositor).is_null());
    assert_eq!(client.focus(), Some(normal));
    action(&compositor, format!("fullscreen-window {normal_id} false"));
    client.sync();
    // A real epoch invalidation stays stale even when this source is visible again.
    action(&compositor, "session-resume");
    assert_ne!(
        request["popup_epoch"],
        query(&compositor, "layout-status")["popup_epoch"]
    );
    rejected(&compositor, &request);
    assert_ne!(client.focus(), Some(dock));
    action(&compositor, format!("focus-window {normal_id}"));
    request["popup_epoch"] = query(&compositor, "layout-status")["popup_epoch"].clone();
    client.unmap(dock);
    rejected(&compositor, &request);
    assert_eq!(client.focus(), Some(normal));
    finish(&mut compositor);
}

#[test]
fn dock_accessibility_context_preserves_input_grabs_and_lock_gates() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, normal, _) = fixture();
    let (dock, _, mut request) = dock(&compositor, &mut client, normal);
    action(&compositor, "pointer-motion-abs 650 330");
    action(&compositor, "pointer-button-down 272");
    request["popup_epoch"] = query(&compositor, "layout-status")["popup_epoch"].clone();
    rejected(&compositor, &request);
    assert_eq!(client.focus(), Some(normal));
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().last().unwrap().0, normal);
    assert_eq!(client.releases().last().unwrap().0, normal);
    client.enable_touch();
    action(&compositor, "touch-down accessible-menu 0 650 330");
    action(&compositor, "touch-frame accessible-menu");
    request["popup_epoch"] = query(&compositor, "layout-status")["popup_epoch"].clone();
    rejected(&compositor, &request);
    assert_eq!(client.focus(), Some(normal));
    action(&compositor, "touch-up accessible-menu 0");
    action(&compositor, "touch-frame accessible-menu");
    assert_eq!(client.touch_cancellations(), 0);
    assert_eq!(client.touch_releases(), &[0]);
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = crate::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    request["popup_epoch"] = query(&compositor, "layout-status")["popup_epoch"].clone();
    let reply = compositor.send_text_command(&command(&request));
    assert!(reply.contains("session is locked"), "{reply}");
    assert_eq!(query(&compositor, "status")["shell_panels"], json!([]));
    assert_ne!(client.focus(), Some(dock));
    locker.unlock();
    assert!(context(&compositor).is_null());
    finish(&mut compositor);
}
