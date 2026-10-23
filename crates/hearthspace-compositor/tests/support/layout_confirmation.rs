//! Real shell/client surfaces exercise modal close and protected-tool recovery.
use super::{
    canvas_map_client::MapClient,
    navigation::{action, keys, query},
    shell_panels::{click, connect},
    *,
};
use hearthspace_common::frame_layout::{
    FrameLayout, LayoutTarget, TargetKind, layout_confirm_app_id, layout_editor_app_id,
};
use serde_json::{Value, json};

const GREEN: [u8; 4] = [0x66, 0xcc, 0, 0xff];

fn saved_layout(path: &Path, layout: &FrameLayout) {
    std::fs::write(
        path,
        serde_json::to_vec(&json!({
            "version":1,"x":0,"y":0,"scale":1.0,"placements":[],
            "frames":{"hearthspace-0":layout},"allow_hide_tools":false,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn status(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "layout-status")
}

fn wait_status(compositor: &HeadlessCompositor, condition: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = status(compositor);
        if condition(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "layout state timed out: {state}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn command(state: &Value, action_name: &str, target: Option<&LayoutTarget>) -> String {
    format!(
        "layout-command {}",
        json!({"session":state["session"],"output":state["output"],"action":action_name,"target":target})
    )
}

fn modal(client: &mut MapClient, state: &Value) -> usize {
    client.add(
        layout_confirm_app_id(state["session"].as_u64().unwrap(), state["output"].as_str()),
        (420, 220),
        (0, 0),
        GREEN,
    )
}

fn window_id(compositor: &HeadlessCompositor, app_id: &str) -> u64 {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["app_id"] == app_id)
        .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn pinned_close_is_modal_outside_editor_cancel_keeps_pin_and_confirm_preserves_group_neighbors() {
    let _guard = headless_test_guard();
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    let app = "dev.hearthspace.PinClose";
    let encoded: String = app.bytes().map(|byte| format!("{byte:02x}")).collect();
    let window = LayoutTarget::new(TargetKind::Window, format!("saved-{encoded}-0"));
    let apps = LayoutTarget::widget("apps");
    let mut layout = FrameLayout::default();
    let group = layout.merge(&window, &apps, false).unwrap();
    saved_layout(&path, &layout);
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let normal = client.add(app.into(), (500, 320), (0, 0), GREEN);
    let id = window_id(&compositor, app);
    wait_status(&compositor, |state| {
        state["pinned_windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pin| pin["id"] == id)
    });
    action(&compositor, format!("focus-window {id}"));
    action(&compositor, format!("close-window {id}"));
    let opened = wait_status(&compositor, |state| {
        state["confirmation"]["kind"] == "close-window"
    });
    assert_eq!(opened["editing"], false);
    assert_eq!(opened["confirmation"]["window_id"], id);
    let dialog = modal(&mut client, &opened);
    assert_eq!(client.focus(), Some(dialog));
    click(&compositor, 32.0, 400.0);
    assert!(
        client.presses().is_empty(),
        "the modal backdrop leaked a client press"
    );
    keys(&compositor, &[1]);
    wait_status(&compositor, |state| state["confirmation"].is_null());
    assert_eq!(client.focus(), Some(normal));
    assert!(!client.closed().contains(&normal));
    assert!(
        status(&compositor)["pinned_windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pin| pin["id"] == id)
    );
    client.destroy(dialog);
    action(&compositor, format!("close-window {id}"));
    let reopened = wait_status(&compositor, |state| {
        state["confirmation"]["kind"] == "close-window"
    });
    assert_ne!(opened["session"], reopened["session"]);
    assert!(
        compositor
            .send_text_command(&command(&opened, "confirm", None))
            .starts_with("err ")
    );
    assert!(
        !client.closed().contains(&normal),
        "a stale dialog closed a current client"
    );
    action(&compositor, command(&reopened, "confirm", None));
    assert!(client.closed().contains(&normal));
    let closed = wait_status(&compositor, |state| state["confirmation"].is_null());
    assert!(closed["pinned_windows"].as_array().unwrap().is_empty());
    let restored: FrameLayout =
        serde_json::from_value(closed["outputs"][0]["layout"].clone()).unwrap();
    assert!(
        restored.placement(&apps).is_frame(),
        "closing one member removed its neighbor"
    );
    assert!(
        !restored.groups.contains_key(&group.id),
        "one-member group did not dissolve"
    );
    assert!(restored.groups.values().all(|group| {
        group
            .members
            .iter()
            .all(|target| target.kind != TargetKind::Window)
    }));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn protected_tools_group_warning_cancels_without_changes_and_opens_system_settings_without_hiding()
{
    use std::os::unix::fs::PermissionsExt;
    let _guard = headless_test_guard();
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    let mut layout = FrameLayout::default();
    let group = layout
        .merge(
            &LayoutTarget::widget("tools"),
            &LayoutTarget::widget("apps"),
            false,
        )
        .unwrap();
    saved_layout(&path, &layout);
    let data = saved.path().join("data");
    std::fs::create_dir_all(data.join("applications")).unwrap();
    let executable = saved.path().join("settings-fixture");
    std::fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"${0}.args\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        data.join("applications/dev.hearthspace.Settings.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Settings\nExec={}\n",
            executable.display()
        ),
    )
    .unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_command(
        "1280x800",
        Some(&path),
        None,
        1,
        false,
        |command| {
            command
                .env("XDG_DATA_HOME", &data)
                .env("XDG_DATA_DIRS", &data);
        },
    );
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    action(&compositor, "layout-tool");
    let opened = wait_status(&compositor, |state| state["editing"] == true);
    let editor = client.add(
        layout_editor_app_id(
            opened["session"].as_u64().unwrap(),
            opened["output"].as_str(),
        ),
        (260, 156),
        (0, 0),
        GREEN,
    );
    assert_eq!(client.focus(), Some(editor));
    let before = opened["outputs"][0]["layout"].clone();
    action(&compositor, command(&opened, "select", Some(&group)));
    action(&compositor, command(&opened, "remove", Some(&group)));
    let warning = wait_status(&compositor, |state| {
        state["confirmation"]["kind"] == "hide-tools"
    });
    assert_eq!(warning["outputs"][0]["layout"], before);
    assert_eq!(warning["allow_hide_tools"], false);
    let dialog = modal(&mut client, &warning);
    assert_eq!(client.focus(), Some(dialog));
    action(&compositor, command(&warning, "cancel", None));
    let cancelled = wait_status(&compositor, |state| state["confirmation"].is_null());
    assert_eq!(cancelled["editing"], true);
    assert_eq!(cancelled["outputs"][0]["layout"], before);
    assert_eq!(client.focus(), Some(editor));
    client.destroy(dialog);
    action(&compositor, command(&cancelled, "remove", Some(&group)));
    let warning = wait_status(&compositor, |state| {
        state["confirmation"]["kind"] == "hide-tools"
    });
    action(&compositor, command(&warning, "confirm", None));
    let done = wait_status(&compositor, |state| state["editing"] == false);
    assert_eq!(
        done["allow_hide_tools"], false,
        "Open settings silently enabled hiding"
    );
    assert_eq!(done["outputs"][0]["layout"], before);
    let args = executable.with_extension("args");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !args.exists() {
        assert!(Instant::now() < deadline, "Settings did not launch");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(args).unwrap(), "--page\nsystem\n");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
