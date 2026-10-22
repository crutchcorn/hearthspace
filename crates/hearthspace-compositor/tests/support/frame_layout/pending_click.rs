//! Clicks during dock resize belong to the original control and committed layout.
use super::*;
use hearthspace_common::frame_layout::frame_widget_app_id;

#[test]
fn frame_layout_pending_resize_click_waits_for_the_resized_native_buffer() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(compositor.wait_for_socket());
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let apps = client.add(
        frame_widget_app_id(Some(&output), "apps"),
        (150, 54),
        (0, 0),
        COLOR,
    );

    client.begin_resize(apps, (102, 54));
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert!(
        client.presses().is_empty(),
        "an old-local press must not reach the already-resized client layout"
    );
    assert!(client.releases().is_empty());

    // Acknowledging the configure, even followed by an old-buffer commit,
    // does not prove that the surface now uses the new local coordinates.
    client.commit_old_buffer(apps);
    assert!(client.presses().is_empty());
    client.commit_geometry_without_buffer(apps);
    assert!(
        client.presses().is_empty(),
        "new xdg geometry alone must not release old-buffer input"
    );
    client.commit_resize(apps);
    assert_eq!(client.presses(), &[(apps, (75.0, 27.0))]);
    assert_eq!(client.releases(), &[(apps, (75.0, 27.0))]);
    assert_eq!(
        client.pointer_target(),
        Some(apps),
        "finishing the deferred grab must retain stationary hover"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_pending_resize_preserves_outside_release_and_later_app_click() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(compositor.wait_for_socket());
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add("pending-click-neighbor".into(), (380, 200), (0, 0), COLOR);
    let id = window(&compositor, "pending-click-neighbor")["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {id} 900 500"));
    let apps = client.add(
        frame_widget_app_id(Some(&output), "apps"),
        (150, 54),
        (0, 0),
        COLOR,
    );
    client.begin_resize(apps, (102, 54));
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 1000 600");
    action(&compositor, "pointer-button-up 272");
    client.commit_resize(apps);
    assert_eq!(client.presses(), &[(apps, (75.0, 27.0))]);
    assert_eq!(
        client.releases(),
        &[(apps, (-154.0, -122.0))],
        "outside release must not turn into activation on replay"
    );
    assert_eq!(client.pointer_target(), Some(normal));

    client.begin_resize(apps, (54, 54));
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "pointer-motion-abs 1000 600");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().last(), Some(&(normal, (100.0, 100.0))));
    assert_eq!(client.releases().last(), Some(&(normal, (100.0, 100.0))));
    client.commit_resize(apps);
    assert_eq!(
        client.presses().len(),
        2,
        "the superseded queued click must not fire later"
    );
    assert_eq!(client.releases().len(), 2);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_pending_resize_cancels_dead_owner_and_lock_without_unpaired_release() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(compositor.wait_for_socket());
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let apps = client.add(
        frame_widget_app_id(Some(&output), "apps"),
        (150, 54),
        (0, 0),
        COLOR,
    );
    client.begin_resize(apps, (102, 54));
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    client.destroy(apps);
    action(&compositor, "pointer-button-up 272");
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());

    let replacement = client.add(
        frame_widget_app_id(Some(&output), "apps"),
        (150, 54),
        (0, 0),
        COLOR,
    );
    client.begin_resize(replacement, (102, 54));
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    client.commit_resize(replacement);
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    locker.unlock();
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses(), &[(replacement, (75.0, 27.0))]);
    assert_eq!(client.releases(), &[(replacement, (75.0, 27.0))]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_pending_resize_timeout_drops_old_click_and_releases_seat() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(compositor.wait_for_socket());
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add("timeout-neighbor".into(), (380, 200), (0, 0), COLOR);
    let id = window(&compositor, "timeout-neighbor")["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {id} 900 500"));
    let apps = client.add(
        frame_widget_app_id(Some(&output), "apps"),
        (150, 54),
        (0, 0),
        COLOR,
    );
    client.begin_resize(apps, (102, 54));
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 1000 600");
    action(&compositor, "pointer-button-up 272");
    std::thread::sleep(Duration::from_millis(2100));
    assert_eq!(client.pointer_target(), Some(normal));
    assert_eq!(client.pointer_position(), (100.0, 100.0));
    client.commit_resize(apps);
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses(), &[(apps, (75.0, 27.0))]);
    assert_eq!(client.releases(), &[(apps, (75.0, 27.0))]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
