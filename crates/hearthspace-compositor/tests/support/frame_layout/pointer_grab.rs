//! A held click follows the same frame surface when its native size changes.
use super::*;
use hearthspace_common::frame_layout::frame_widget_app_id;

fn apps_bounds(compositor: &HeadlessCompositor) -> Value {
    layout(compositor)["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == "apps")
        .unwrap()["rect"]
        .clone()
}

#[test]
fn frame_layout_held_apps_click_tracks_resized_surface_without_retargeting_release() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(compositor.wait_for_socket());
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add("frame-grab-neighbor".into(), (380, 200), (0, 0), COLOR);
    let normal_id = window(&compositor, "frame-grab-neighbor")["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {normal_id} 900 500"));
    let apps = client.add(
        frame_widget_app_id(Some(&output), "apps"),
        (150, 54),
        (0, 0),
        COLOR,
    );
    assert_eq!(apps_bounds(&compositor)["x"], 1106.0);
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    assert_eq!(client.presses().last(), Some(&(apps, (123.0, 27.0))));

    // Closing a running app shrinks the dock while Apps List stays at exactly
    // the same screen position. No physical pointer motion occurs before up.
    client.resize(apps, (102, 54));
    assert_eq!(apps_bounds(&compositor)["x"], 1154.0);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.releases().last(), Some(&(apps, (75.0, 27.0))));
    assert_eq!(client.releases().len(), 1);

    action(&compositor, "pointer-button-down 272");
    client.resize(apps, (54, 54));
    action(&compositor, "pointer-motion-abs 1100 650");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        client.releases().last(),
        Some(&(apps, (-102.0, -72.0))),
        "leaving the control must cancel the click in its native button, not retarget the release"
    );
    assert_eq!(client.presses().len(), 2);
    assert_eq!(client.releases().len(), 2);

    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().last(), Some(&(apps, (27.0, 27.0))));
    assert_eq!(client.releases().last(), Some(&(apps, (27.0, 27.0))));
    assert_eq!(client.presses().len(), client.releases().len());

    // An ordinary application keeps Smithay's normal grab when the pointer
    // crosses onto the shell, and never sends a stray shell activation.
    action(&compositor, "pointer-motion-abs 1000 600");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 1229 749");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().last(), Some(&(normal, (100.0, 100.0))));
    assert_eq!(client.releases().last(), Some(&(normal, (329.0, 249.0))));
    assert_eq!(client.presses().len(), 4);
    assert_eq!(client.releases().len(), 4);

    action(&compositor, "pointer-button-down 272");
    assert_eq!(client.presses().last(), Some(&(apps, (27.0, 27.0))));
    client.destroy(apps);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        client.releases().last().unwrap().0,
        apps,
        "Smithay's destruction cleanup must not redirect the release to another window"
    );
    assert_eq!(client.releases().len(), 5);
    action(&compositor, "pointer-motion-abs 1000 600");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.releases().last(), Some(&(normal, (100.0, 100.0))));
    assert_eq!(
        client.releases().len(),
        6,
        "destroying a held control must not leave a stuck grab"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
