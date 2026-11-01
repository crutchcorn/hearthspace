//! Catalog menus preserve the source client's input and compositor-owned generation.
use super::*;

#[path = "accessibility.rs"]
mod accessibility;

fn source(client: &mut MapClient, compositor: &HeadlessCompositor) -> (usize, Value) {
    action(
        compositor,
        "desktop-app-register {\"offset\":0,\"ids\":[\"test.desktop\"]}",
    );
    let output = query(compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let id = hearthspace_common::launcher_app_id_for_output(&output);
    let source = client.add(id.clone(), (500, 300), (8, 6), [0, 255, 0, 255]);
    assert_eq!(client.focus(), Some(source));
    (
        source,
        json!({
            "app_id":"test.desktop", "output_name":output, "source_app_id":id,
            "popup_epoch":query(compositor, "layout-status")["popup_epoch"],
            "anchor":{"x":28.,"y":26.,"width":120.,"height":40.}
        }),
    )
}

fn command(request: &Value) -> String {
    format!("desktop-app-context {request}")
}

fn rejected(compositor: &HeadlessCompositor, request: &Value) {
    let reply = compositor.send_text_command(&command(request));
    assert!(reply.starts_with("err "), "{reply}");
    assert!(context(compositor).is_null());
}

#[test]
fn catalog_context_uses_source_geometry_preserves_selection_and_restores_exact_focus() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, _) = fixture();
    action(&compositor, "desktop-select \"file:61\"");
    let before = query(&compositor, "desktop-status")["items"].clone();
    let (source, request) = source(&mut client, &compositor);
    let source_bounds = query(&compositor, "status")["shell_panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == "search")
        .unwrap()
        .clone();
    action(&compositor, command(&request));
    let menu = context(&compositor);
    assert_eq!(menu["ids"], json!(["app:test.desktop"]));
    assert_eq!(menu["output"], request["output_name"]);
    let surface = map_menu(&mut client, &menu);
    assert_eq!(client.focus(), Some(surface));
    assert!(!client.closed().contains(&source));
    let bounds = panel(&compositor);
    assert_eq!(bounds["x"], source_bounds["x"].as_i64().unwrap() + 20);
    assert_eq!(bounds["y"], source_bounds["y"].as_i64().unwrap() + 60);
    let after = query(&compositor, "desktop-status")["items"].clone();
    assert_eq!(before[0]["selected"], after[0]["selected"]);
    let accepted = query(
        &compositor,
        &format!("desktop-context-read {}", menu["generation"]),
    );
    assert_eq!(
        accepted["context"], menu,
        "catalog target needs no mapped tile"
    );
    close(&compositor, &menu);
    assert_eq!(client.focus(), Some(source));
    client.destroy(surface);
    assert_eq!(client.focus(), Some(source));
    finish(&mut compositor);
}

#[test]
fn catalog_context_rejects_stale_sources_outputs_anchors_and_input_grabs() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, normal, _) = fixture();
    let (source, request) = source(&mut client, &compositor);
    for (field, value) in [
        ("app_id", json!("missing.desktop")),
        ("source_app_id", json!("context-app")),
        ("output_name", json!("missing-output")),
        ("popup_epoch", json!(u64::MAX)),
        (
            "anchor",
            json!({"x":900.,"y":900.,"width":10.,"height":10.}),
        ),
        ("anchor", json!({"x":10.,"y":10.,"width":-1.,"height":10.})),
    ] {
        let mut invalid = request.clone();
        invalid[field] = value;
        rejected(&compositor, &invalid);
    }
    let bounds = query(&compositor, "status")["shell_panels"][0].clone();
    let x = bounds["x"].as_f64().unwrap() + 20.;
    let y = bounds["y"].as_f64().unwrap() + 20.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 273");
    rejected(&compositor, &request);
    action(&compositor, "pointer-button-up 273");
    assert_eq!(client.presses().last().unwrap().0, source);
    assert_eq!(client.releases().last().unwrap().0, source);
    client.enable_touch();
    action(&compositor, format!("touch-down app-menu 0 {x} {y}"));
    action(&compositor, "touch-frame app-menu");
    rejected(&compositor, &request);
    action(&compositor, "touch-up app-menu 0");
    action(&compositor, "touch-frame app-menu");
    assert_eq!(client.touch_cancellations(), 0);
    assert_eq!(client.touch_releases(), &[0]);
    action(&compositor, command(&request));
    let menu = context(&compositor);
    let surface = map_menu(&mut client, &menu);
    click(&compositor, 850., 400.);
    assert_eq!(client.focus(), Some(normal));
    assert!(context(&compositor).is_null());
    rejected(&compositor, &request);
    client.destroy(surface);
    assert_eq!(client.focus(), Some(normal));
    finish(&mut compositor);
}

#[test]
fn catalog_context_source_destruction_and_escape_reject_late_menus() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, _) = fixture();
    let (source, request) = source(&mut client, &compositor);
    action(&compositor, command(&request));
    let first = context(&compositor);
    keys(&compositor, &[1]);
    assert!(context(&compositor).is_null());
    assert!(!client.closed().contains(&source));
    let late = map_menu(&mut client, &first);
    assert!(client.closed().contains(&late));
    assert_eq!(client.focus(), Some(source));
    client.destroy(late);
    action(&compositor, command(&request));
    let next = context(&compositor);
    assert!(next["generation"].as_u64() > first["generation"].as_u64());
    let mapped = map_menu(&mut client, &next);
    assert_eq!(client.focus(), Some(mapped));
    let received = client.keys().len();
    action(&compositor, "key-down 1");
    assert!(context(&compositor).is_null());
    assert_eq!(client.focus(), Some(source));
    action(&compositor, "key-up 1");
    assert_eq!(client.keys().len(), received);
    assert!(!client.closed().contains(&source));
    client.destroy(mapped);
    action(&compositor, command(&request));
    let next = context(&compositor);
    client.destroy(source);
    let late = map_menu(&mut client, &next);
    assert!(client.closed().contains(&late));
    let reply =
        compositor.send_text_command(&format!("desktop-context-read {}", next["generation"]));
    assert!(reply.starts_with("err "), "{reply}");
    finish(&mut compositor);
}

#[test]
fn catalog_context_teardown_before_close_restores_source_without_dismissing_apps_list() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, _) = fixture();
    let (source, request) = source(&mut client, &compositor);
    for unmap_first in [false, true] {
        action(&compositor, command(&request));
        let menu = context(&compositor);
        let mapped = map_menu(&mut client, &menu);
        assert_eq!(client.focus(), Some(mapped));
        if unmap_first {
            client.unmap(mapped);
        } else {
            client.destroy(mapped);
        }
        assert_eq!(client.focus(), Some(source));
        assert!(context(&compositor).is_null());
        assert!(!client.closed().contains(&source));
        // A slow saved-preference worker can delay this independent IPC.
        close(&compositor, &menu);
        assert_eq!(client.focus(), Some(source));
        if unmap_first {
            client.destroy(mapped);
        }
    }
    finish(&mut compositor);
}

#[test]
fn dock_context_uses_the_actual_apps_widget_and_keeps_its_release_and_focus() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    action(
        &compositor,
        "desktop-app-register {\"offset\":0,\"ids\":[\"test.desktop\"]}",
    );
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let app_id = hearthspace_common::frame_layout::frame_widget_app_id(Some(&output), "apps");
    let dock = client.add(app_id.clone(), (300, 54), (0, 0), [0, 255, 0, 255]);
    let layout = query(&compositor, "layout-status");
    let rect = &layout["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == "apps")
        .unwrap()["rect"];
    let x = rect["x"].as_f64().unwrap() + 20.;
    let y = rect["y"].as_f64().unwrap() + 20.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 273");
    action(&compositor, "pointer-button-up 273");
    assert_eq!(client.presses().last().unwrap().0, dock);
    assert_eq!(client.releases().last().unwrap().0, dock);
    assert_eq!(client.focus(), Some(dock));
    let request = json!({
        "app_id":"test.desktop", "output_name":output, "source_app_id":app_id,
        "popup_epoch":layout["popup_epoch"],
        "anchor":{"x":10.,"y":10.,"width":40.,"height":30.}
    });
    action(&compositor, command(&request));
    let menu = context(&compositor);
    let mapped = map_menu(&mut client, &menu);
    assert_eq!(client.focus(), Some(mapped));
    let bounds = panel(&compositor);
    assert!(bounds["x"].as_i64().unwrap() >= 8);
    assert!(bounds["y"].as_i64().unwrap() + bounds["height"].as_i64().unwrap() <= 792);
    close(&compositor, &menu);
    assert_eq!(client.focus(), Some(dock));
    assert!(!client.closed().contains(&dock));
    finish(&mut compositor);
}

#[test]
fn catalog_context_admits_apps_beyond_the_tile_limit_and_rechecks_refreshed_ids() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, _) = fixture();
    let (source, mut request) = source(&mut client, &compositor);
    let ids: Vec<_> = (0..=hearthspace_common::MAX_DESKTOP_ITEMS)
        .map(|index| format!("catalog-{index}.desktop"))
        .collect();
    let tiles: Vec<_> = ids[..hearthspace_common::MAX_DESKTOP_ITEMS]
        .iter()
        .map(|id| format!("app:{id}"))
        .collect();
    for (command, entries) in [("desktop-register", &tiles), ("desktop-app-register", &ids)] {
        let mut offset = 0;
        for batch in entries.chunks(256) {
            let registration = format!("{command} {}", json!({"offset":offset,"ids":batch}));
            assert!(registration.len() < hearthspace_ipc::transport::MAX_REQUEST_BYTES);
            action(&compositor, registration);
            offset += batch.len();
        }
    }
    let target = ids.last().unwrap();
    let item = format!("app:{target}");
    assert!(!tiles.contains(&item));
    request["app_id"] = json!(target);
    action(&compositor, command(&request));
    let menu = context(&compositor);
    let mapped = map_menu(&mut client, &menu);
    assert_eq!(client.focus(), Some(mapped));
    let read = format!("desktop-context-read {}", menu["generation"]);
    assert_eq!(query(&compositor, &read)["context"]["ids"], json!([item]));
    for forbidden in [
        format!("desktop-select {}", json!(item)),
        format!("desktop-activate {}", json!(item)),
        format!("desktop-context-copy {}", menu["generation"]),
        format!("desktop-files-copy {}", json!({"ids":[item],"cut":false})),
    ] {
        let reply = compositor.send_text_command(&forbidden);
        assert!(reply.starts_with("err "), "{forbidden}: {reply}");
    }
    action(
        &compositor,
        "desktop-app-register {\"offset\":0,\"ids\":[\"replacement.desktop\"]}",
    );
    assert!(compositor.send_text_command(&read).starts_with("err "));
    close(&compositor, &menu);
    client.destroy(mapped);
    assert_eq!(client.focus(), Some(source));
    rejected(&compositor, &request);
    request["app_id"] = json!("replacement.desktop");
    action(&compositor, command(&request));
    let replacement = context(&compositor);
    assert!(replacement["generation"].as_u64() > menu["generation"].as_u64());
    assert!(compositor.send_text_command(&read).starts_with("err "));
    close(&compositor, &replacement);
    finish(&mut compositor);
}

#[test]
fn catalog_context_registration_rejects_invalid_batches_without_losing_valid_ids() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, _) = fixture();
    let (_, request) = source(&mut client, &compositor);
    // The production registry admits every supported byte-length identity, not
    // just short filenames. A failed mixed batch must not revoke these IDs.
    let boundary = "é".repeat(510);
    action(
        &compositor,
        format!(
            "desktop-app-register {}",
            json!({"offset":1,"ids":[boundary]})
        ),
    );
    let mut boundary_request = request.clone();
    boundary_request["app_id"] = json!(boundary);
    action(&compositor, command(&boundary_request));
    let boundary_menu = context(&compositor);
    let mapped = map_menu(&mut client, &boundary_menu);
    assert_eq!(client.focus(), Some(mapped));
    assert_eq!(
        query(
            &compositor,
            &format!("desktop-context-read {}", boundary_menu["generation"])
        )["context"]["ids"],
        json!([format!("app:{boundary}")])
    );
    close(&compositor, &boundary_menu);
    client.destroy(mapped);
    for registration in [
        json!({"offset":3,"ids":["next.desktop"]}),
        json!({"offset":2,"ids":["test.desktop"]}),
        json!({"offset":0,"ids":["duplicate.desktop","duplicate.desktop"]}),
        json!({"offset":0,"ids":[""]}),
        json!({"offset":0,"ids":["bad\n.desktop"]}),
        json!({"offset":0,"ids":["x".repeat(1021)]}),
        json!({"offset":0,"ids":["test.desktop","x".repeat(1021)]}),
        json!({"offset":0,"ids":["test.desktop","é".repeat(511)]}),
        json!({"offset":0,"ids":[],"unexpected":true}),
    ] {
        let reply = compositor.send_text_command(&format!("desktop-app-register {registration}"));
        assert!(reply.starts_with("err "), "{reply}");
        action(&compositor, command(&request));
        let menu = context(&compositor);
        assert_eq!(menu["ids"], json!(["app:test.desktop"]));
        close(&compositor, &menu);
        action(&compositor, command(&boundary_request));
        close(&compositor, &context(&compositor));
    }
    action(
        &compositor,
        "desktop-app-register {\"offset\":0,\"ids\":[]}",
    );
    rejected(&compositor, &request);
    finish(&mut compositor);
}
