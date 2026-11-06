use super::*;

#[test]
fn satellite_desktop_drop_uses_a_validated_child_proxy_with_original_target_identity() {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(files());
    scene.receiver.install_proxy(true, true);
    scene.select_all();
    let before = scene.locations();
    scene.begin(0);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(scene.receiver.dropped[0], scene.expected);
    assert_eq!(scene.locations(), before);
    scene.finish();
}

#[test]
fn satellite_desktop_invalid_and_destroyed_proxy_cannot_redirect_a_new_offer() {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(files());
    let proxy = scene.receiver.install_proxy(false, false);
    // An unverified proxy does not redirect ordinary destination messages.
    scene.begin(0);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 1);
    assert_eq!(
        scene.receiver.dropped[0],
        b"file:///tmp/music%20%231%25.ogg\r\n"
    );
    scene.receiver.wait_owner_released();
    scene
        .receiver
        .client
        .connection
        .destroy_window(proxy)
        .unwrap()
        .check()
        .unwrap();
    let proxy = scene.receiver.install_proxy(true, true);
    scene.begin(1);
    // Proxy destruction must revoke host acceptance before the held release.
    scene
        .receiver
        .client
        .connection
        .destroy_window(proxy)
        .unwrap()
        .check()
        .unwrap();
    scene.receiver.wait_owner_released();
    scene.receiver.barrier(&scene.compositor);
    scene.release();
    scene.receiver.barrier(&scene.compositor);
    assert_eq!(scene.receiver.dropped.len(), 1);
    scene.receiver.clear_proxy();
    scene.begin(1);
    scene.release();
    scene.receiver.wait(|r| r.dropped.len() == 2);
    assert_eq!(
        scene.receiver.dropped[1],
        b"file:///tmp/caf%C3%A9%FF.ogg\r\n"
    );
    scene.finish();
}
