use super::*;
use crate::session_lock::lock_client;

#[test]
fn canvas_search_cancels_marquee_release_without_stealing_later_field_drags() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    action(&compositor, "pointer-motion-abs 20 600");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 180 650");
    keys(&compositor, &[29, 37]);
    let opened = search(&compositor);
    assert_eq!(opened["active"], true);
    let field_number = field(&mut client, &opened);
    let x = opened["field"]["x"].as_f64().unwrap() + 100.0;
    let y = opened["field"]["y"].as_f64().unwrap() + 25.0;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-up 272");
    assert!(client.presses().is_empty());
    assert!(
        client.releases().is_empty(),
        "cancelled marquee release belongs to the compositor"
    );
    action(&compositor, "pointer-button-down 272");
    action(
        &compositor,
        format!("pointer-motion-abs {} {}", x + 500.0, y + 100.0),
    );
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().len(), 1);
    assert_eq!(
        client.releases().len(),
        1,
        "field drag must receive its release beyond field bounds"
    );
    assert_eq!(client.presses()[0].0, field_number);
    assert_eq!(client.releases()[0].0, field_number);
    assert_eq!(search(&compositor)["active"], true);
    click(&compositor, x, y);
    assert_eq!(client.presses().len(), 2);
    assert_eq!(
        client.releases().len(),
        2,
        "next field click must remain paired"
    );
}

#[test]
fn canvas_search_stale_sessions_and_late_fields_cannot_replace_current_input() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    normal(&mut client, &compositor, "dev.hearthspace.SearchLifecycle");
    action(&compositor, "canvas-search-open");
    let first = search(&compositor);
    let old_session = first["session"].as_u64().unwrap();
    action(&compositor, format!("canvas-search-close {old_session}"));
    action(&compositor, "canvas-search-open");
    let second = search(&compositor);
    assert_ne!(first["session"], second["session"]);
    let current_field = field(&mut client, &second);
    let late_field = field(&mut client, &first);
    assert!(client.closed().contains(&late_field));
    assert_eq!(client.focus(), Some(current_field));
    for command in [
        format!("canvas-search-close {old_session}"),
        format!(
            "canvas-search-update {}",
            json!({"session":old_session,"query":"stale"})
        ),
        format!("canvas-search-activate {}", json!({"session":old_session})),
    ] {
        assert!(
            compositor.send_text_command(&command).starts_with("err "),
            "{command}"
        );
    }
    let current = search(&compositor);
    assert_eq!(current["session"], second["session"]);
    assert_eq!(current["query"], "");
    assert_eq!(client.focus(), Some(current_field));

    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert_eq!(query(&compositor, "status")["locked"], true);
    assert!(
        compositor
            .send_text_command("canvas-search-status")
            .starts_with("err ")
    );
    locker.unlock();
    wait_search(&compositor, |state| {
        state["active"] == false && state["mode"] == false
    });
    assert!(client.closed().contains(&current_field));
    assert!(
        compositor
            .send_text_command(&format!(
                "canvas-search-update {}",
                json!({"session":second["session"],"query":"after unlock"})
            ))
            .starts_with("err ")
    );
    assert!(search(&compositor)["scene"].as_array().unwrap().is_empty());
}

#[test]
fn canvas_search_armed_tool_opens_at_click_and_empty_click_dismisses() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-tool");
    assert_eq!(search(&compositor)["mode"], true);
    assert_eq!(search(&compositor)["active"], false);
    click(&compositor, 1100.0, 650.0);
    let opened = search(&compositor);
    assert_eq!(opened["active"], true);
    assert_eq!(opened["results_above"], true);
    // The field remains beside the requested point, clamped inside its output.
    let field = &opened["field"];
    assert!(field["x"].as_f64().unwrap() >= 24.0);
    assert!(field["x"].as_f64().unwrap() + field["width"].as_f64().unwrap() <= 1256.0);
    assert!(field["y"].as_f64().unwrap() + field["height"].as_f64().unwrap() <= 704.0);
    click(&compositor, 20.0, 760.0);
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(
        search(&compositor)["mode"],
        true,
        "dismissal retains the selected Search tool"
    );
    click(&compositor, 320.0, 200.0);
    assert_eq!(search(&compositor)["active"], true);
    assert_ne!(search(&compositor)["session"], opened["session"]);
}

#[test]
fn canvas_search_control_activation_tokens_are_invalid_after_session_lock() {
    use std::os::unix::net::UnixDatagram;

    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    action(
        &compositor,
        format!(
            "canvas-search-register {}",
            json!({"offset":0,"entries":[
                {"id":"control:sound","title":"Sound","search_text":"volume"}
            ]})
        ),
    );
    action(&compositor, "canvas-search-open");
    let opened = search(&compositor);
    action(
        &compositor,
        format!(
            "canvas-search-activate {}",
            json!({"session":opened["session"],"id":"control:sound"})
        ),
    );
    let token = loop {
        let mut bytes = [0; 4096];
        let count = events.recv(&mut bytes).unwrap();
        let event: Value = serde_json::from_slice(&bytes[..count]).unwrap();
        if event["action"] == "canvas-search-action" {
            break event["token"].as_u64().unwrap();
        }
    };
    assert_eq!(search(&compositor)["active"], false);
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    locker.unlock();
    assert!(
        compositor
            .send_text_command(&format!("canvas-search-event-accept {token}"))
            .starts_with("err "),
        "a delayed shell event must not open a control after unlock"
    );
}
