//! Desktop drags cross the real Wayland data-device boundary without app clicks.
use super::*;
use super::{
    canvas_map_client::MapClient,
    navigation::{action, keys, query},
    shell_panels::connect,
};
use serde_json::{Value, json};

#[cfg(feature = "xwayland")]
#[path = "desktop_files/clipboard_x11.rs"]
mod clipboard_x11;

fn fixture() -> (HeadlessCompositor, MapClient, usize) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let target = client.add(
        "desktop-drop-receiver".into(),
        (400, 300),
        (0, 0),
        [0, 0, 255, 255],
    );
    let id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {id} 500 200"));
    for id in ["file:61", "file:62"] {
        client.add(
            hearthspace_common::desktop_item_app_id(id),
            (100, 108),
            (0, 0),
            [0, 255, 0, 255],
        );
    }
    action(
        &compositor,
        "desktop-register {\"offset\":0,\"ids\":[\"file:61\",\"file:62\"]}",
    );
    action(
        &compositor,
        format!(
            "desktop-file-register {}",
            json!({"offset":0,"entries":[
                {"id":"file:61","uri":"file:///tmp/music%20%231%25.ogg"},
                {"id":"file:62","uri":"file:///tmp/caf%C3%A9%FF.ogg"}
            ]})
        ),
    );
    (compositor, client, target)
}
fn position(compositor: &HeadlessCompositor, id: &str) -> Value {
    let status = query(compositor, "desktop-status");
    let item = status["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap();
    json!([item["x"], item["y"]])
}
fn press(compositor: &HeadlessCompositor, id: &str) {
    let point = position(compositor, id);
    action(
        compositor,
        format!(
            "pointer-motion-abs {} {}",
            point[0].as_i64().unwrap() + 44,
            point[1].as_i64().unwrap() + 40
        ),
    );
    action(compositor, "pointer-button-down 272");
}

#[test]
fn desktop_multi_file_drop_delivers_uris_without_moving_icons_or_clicking_application() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, target) = fixture();
    client.enable_file_drops(true);
    action(&compositor, "desktop-select \"file:61\"");
    action(&compositor, "key-down 29");
    press(&compositor, "file:62");
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 29");
    let before = [
        position(&compositor, "file:61"),
        position(&compositor, "file:62"),
    ];
    press(&compositor, "file:61");
    action(&compositor, "pointer-motion-abs 650 350");
    assert_eq!(client.drop_enters(), 1);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        client.dropped_files(),
        [b"file:///tmp/music%20%231%25.ogg\r\nfile:///tmp/caf%C3%A9%FF.ogg\r\n".to_vec()]
    );
    assert!(!client.presses().iter().any(|(index, _)| *index == target));
    assert_eq!(
        [
            position(&compositor, "file:61"),
            position(&compositor, "file:62")
        ],
        before
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn desktop_file_drop_cancels_on_escape_and_canvas_drop_keeps_ordinary_movement() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _) = fixture();
    client.enable_file_drops(true);
    let before = position(&compositor, "file:61");
    press(&compositor, "file:61");
    action(&compositor, "pointer-motion-abs 650 350");
    assert_eq!(client.drop_enters(), 1);
    keys(&compositor, &[1]);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.drops(), 0);
    assert_eq!(position(&compositor, "file:61"), before);
    press(&compositor, "file:61");
    action(&compositor, "pointer-motion-abs 350 600");
    action(&compositor, "pointer-button-up 272");
    assert_ne!(position(&compositor, "file:61"), before);
    assert_eq!(client.drops(), 0);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn desktop_file_clipboard_interoperates_and_cut_clear_never_erases_newer_copy() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _) = fixture();
    client.enable_file_drops(true);
    action(&compositor, "desktop-select \"file:61\"");
    action(
        &compositor,
        "desktop-files-copy {\"ids\":[\"file:61\"],\"cut\":true}",
    );
    let first = query(&compositor, "desktop-files-clipboard-read");
    assert_eq!(
        client.clipboard_files("x-special/gnome-copied-files"),
        b"cut\nfile:///tmp/music%20%231%25.ogg"
    );
    action(
        &compositor,
        "desktop-files-copy {\"ids\":[\"file:61\"],\"cut\":true}",
    );
    let second = query(&compositor, "desktop-files-clipboard-read");
    assert_ne!(first["generation"], second["generation"]);
    action(
        &compositor,
        format!(
            "desktop-files-clipboard-clear {}",
            json!({"generation":first["generation"]})
        ),
    );
    assert_eq!(query(&compositor, "desktop-files-clipboard-read"), second);

    action(
        &compositor,
        format!(
            "desktop-files-clipboard-clear {}",
            json!({"generation":second["generation"]})
        ),
    );
    assert!(
        compositor
            .send_text_command("desktop-files-clipboard-read")
            .starts_with("err ")
    );
    client.own_file_clipboard(b"copy\nfile:///tmp/from%20another%20app.ogg".to_vec());
    let external = std::thread::scope(|scope| {
        let read = scope.spawn(|| query(&compositor, "desktop-files-clipboard-read"));
        while !read.is_finished() {
            client.sync();
            std::thread::sleep(Duration::from_millis(5));
        }
        read.join().unwrap()
    });
    assert_eq!(
        external,
        json!({"cut":false,"uris":["file:///tmp/from%20another%20app.ogg"]})
    );
    action(
        &compositor,
        format!(
            "desktop-files-clipboard-clear {}",
            json!({"generation":second["generation"]})
        ),
    );
    assert_eq!(
        client.clipboard_files("x-special/gnome-copied-files"),
        b"copy\nfile:///tmp/from%20another%20app.ogg"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn desktop_file_drag_lock_and_source_removal_cancel_the_offer() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, target) = fixture();
    client.enable_file_drops(true);
    let before = position(&compositor, "file:61");
    press(&compositor, "file:61");
    action(&compositor, "pointer-motion-abs 650 350");
    assert_eq!(client.drop_enters(), 1);
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert_eq!(client.drops(), 0);
    locker.unlock();
    action(&compositor, "pointer-button-up 272");
    assert_eq!(position(&compositor, "file:61"), before);
    assert_eq!(client.drops(), 0);

    press(&compositor, "file:61");
    action(&compositor, "pointer-motion-abs 650 350");
    assert_eq!(client.drop_enters(), 2);
    client.destroy(1); // The first desktop source; the receiver remains alive.
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.drops(), 0);
    assert!(!client.presses().iter().any(|(index, _)| *index == target));
    press(&compositor, "file:62");
    action(&compositor, "pointer-motion-abs 650 350");
    assert_eq!(client.drop_enters(), 3);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        client.dropped_files(),
        [b"file:///tmp/caf%C3%A9%FF.ogg\r\n".to_vec()]
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn desktop_file_drop_rejection_preserves_icons_and_does_not_activate_the_app() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, target) = fixture();
    client.enable_file_drops(false);
    let before = position(&compositor, "file:61");
    press(&compositor, "file:61");
    action(&compositor, "pointer-motion-abs 650 350");
    assert_eq!(client.drop_enters(), 1);
    action(&compositor, "pointer-button-up 272");
    assert!(client.dropped_files().is_empty());
    assert_eq!(position(&compositor, "file:61"), before);
    assert!(!client.presses().iter().any(|(index, _)| *index == target));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
