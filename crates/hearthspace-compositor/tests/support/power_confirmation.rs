//! Real native surfaces exercise power-modal placement, isolation and cancellation.
use super::{
    navigation::{action, keys, query},
    shell_panels::{click, connect},
    *,
};
use hearthspace_common::{
    POWER_CONFIRM_APP_ID, POWER_MENU_APP_ID, power_confirm_app_id_for_output,
    shell_bar_app_id_for_output,
};
use serde_json::Value;

const COLOR: [u8; 4] = [0x66, 0xcc, 0, 0xff];

fn panel(compositor: &HeadlessCompositor, kind: &str) -> Value {
    query(compositor, "status")["shell_panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|panel| panel["kind"] == kind)
        .expect("mapped shell panel")
        .clone()
}

fn touch(compositor: &HeadlessCompositor, x: f64, y: f64) {
    action(compositor, format!("touch-down power-modal 0 {x} {y}"));
    action(compositor, "touch-frame power-modal");
    action(compositor, "touch-up power-modal 0");
    action(compositor, "touch-frame power-modal");
}

#[rstest::rstest]
#[case::native("1280x800", 1, (420, 220), (430, 290))]
#[case::scaled("2560x1600", 2, (420, 220), (430, 290))]
#[case::small_scaled("640x480", 2, (272, 192), (24, 24))]
fn power_confirmation_centers_geometry_and_clamps_with_output_scale(
    #[case] output_size: &str,
    #[case] scale: i32,
    #[case] size: (i32, i32),
    #[case] origin: (i32, i32),
) {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options(output_size, None, None, scale);
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    // A regular power menu still uses the existing right/below-bar anchor.
    let menu = client.add(POWER_MENU_APP_ID.into(), (240, 120), (8, 6), COLOR);
    let bounds = panel(&compositor, "power");
    let logical_width = output_size
        .split_once('x')
        .unwrap()
        .0
        .parse::<i32>()
        .unwrap()
        / scale;
    assert_eq!(bounds["x"], logical_width - 240 - 12);
    assert_eq!(bounds["y"], 66);
    client.destroy(menu);

    let dialog = client.add(
        power_confirm_app_id_for_output(&output),
        (420, 220),
        (8, 6),
        COLOR,
    );
    assert_eq!(client.focus(), Some(dialog));
    assert_eq!(client.size(dialog), size);
    let bounds = panel(&compositor, "power-confirm");
    assert_eq!(bounds["output"], output);
    assert_eq!(
        (bounds["x"].as_i64().unwrap(), bounds["y"].as_i64().unwrap()),
        (i64::from(origin.0), i64::from(origin.1))
    );
    assert_eq!(bounds["width"], size.0);
    assert_eq!(bounds["height"], size.1);
    click(
        &compositor,
        f64::from(origin.0 + 20),
        f64::from(origin.1 + 20),
    );
    assert_eq!(client.presses().last(), Some(&(dialog, (28., 26.))));
    assert!(!client.closed().contains(&dialog));
    let layout = query(&compositor, "layout-status");
    assert_eq!(layout["editing"], false);
    assert_eq!(layout["session"], 0);
    client.destroy(dialog);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn power_confirmation_blocks_background_input_until_escape_is_acknowledged() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut background = connect(&compositor);
    background.enable_touch();
    let normal = background.add(
        "dev.hearthspace.PowerBackground".into(),
        (500, 320),
        (0, 0),
        COLOR,
    );
    background.add(
        shell_bar_app_id_for_output(&output),
        (1280, 54),
        (0, 0),
        COLOR,
    );
    let original = query(&compositor, "windows")["windows"][0].clone();
    let id = original["id"].as_u64().unwrap();
    let viewport = query(&compositor, "status")["viewport"].clone();
    let layout = query(&compositor, "layout-status");
    let mut shell = connect(&compositor);
    shell.enable_touch();
    let dialog = shell.add(POWER_CONFIRM_APP_ID.into(), (420, 220), (8, 6), COLOR);
    assert_eq!(shell.focus(), Some(dialog));
    assert_eq!(background.focus(), None);

    click(&compositor, 10., 20.); // Shell controls.
    click(&compositor, 400., 400.); // Exposed background application.
    touch(&compositor, 10., 20.);
    touch(&compositor, 400., 400.);
    for chord in [&[125][..], &[125, 57], &[125, 106], &[56, 15], &[56, 62]] {
        keys(&compositor, chord);
    }
    action(&compositor, "key-down 125");
    action(&compositor, "axis 0 -120");
    action(&compositor, "key-up 125");
    assert!(
        background.keys().is_empty(),
        "background keyboard input leaked"
    );
    assert!(
        background.presses().is_empty(),
        "background pointer input leaked"
    );
    assert!(
        background.touches().is_empty(),
        "background native touch leaked"
    );
    assert!(
        background.closed().is_empty(),
        "Alt-F4 closed the background"
    );
    assert!(
        shell.closed().is_empty(),
        "outside input dismissed the modal"
    );
    assert_eq!(shell.focus(), Some(dialog));
    assert!(
        compositor
            .send_text_command(&format!("focus-window {id}"))
            .starts_with("err ")
    );
    for command in ["pan-left", "pan-to 100 200", "zoom-in", "zoom-to 0.8"] {
        assert!(
            compositor.send_text_command(command).starts_with("err "),
            "modal must reject control-socket view mutation: {command}"
        );
    }
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    let current_layout = query(&compositor, "layout-status");
    assert_eq!(current_layout["session"], layout["session"]);
    assert_eq!(
        current_layout["outputs"][0]["layout"],
        layout["outputs"][0]["layout"]
    );
    let rejected = shell.add(
        power_confirm_app_id_for_output(&output),
        (420, 220),
        (0, 0),
        COLOR,
    );
    assert!(
        shell.closed().contains(&rejected),
        "a second dialog replaced modal ownership"
    );
    assert_eq!(shell.focus(), Some(dialog));
    shell.destroy(rejected);

    // Native controls remain interactive, with their surface's geometry inset.
    touch(&compositor, 450., 310.);
    assert_eq!(shell.touches().last(), Some(&(dialog, (28., 26.))));
    keys(&compositor, &[15]);
    assert!(shell.keys().contains(&15), "Tab must reach dialog controls");

    // A three-contact canvas pinch must cancel native touch and leave the view.
    for command in [
        "touch-down power-modal 0 450 310",
        "touch-frame power-modal",
        "touch-down power-modal 1 550 310",
        "touch-down power-modal 2 500 410",
        "touch-frame power-modal",
        "touch-motion power-modal 0 430 290",
        "touch-motion power-modal 1 570 290",
        "touch-motion power-modal 2 500 430",
        "touch-frame power-modal",
        "touch-up power-modal 0",
        "touch-up power-modal 1",
        "touch-up power-modal 2",
        "touch-frame power-modal",
    ] {
        action(&compositor, command);
    }
    assert!(shell.touch_cancellations() > 0);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);

    background.add(
        "dev.hearthspace.PowerLateBackground".into(),
        (200, 140),
        (0, 0),
        COLOR,
    );
    assert_eq!(background.focus(), None, "late app stole modal focus");
    keys(&compositor, &[1]);
    assert!(shell.closed().contains(&dialog));
    assert!(!shell.keys().contains(&1), "Escape must only dismiss");
    click(&compositor, 10., 20.);
    assert!(
        background.presses().is_empty(),
        "close acknowledgement gap leaked input"
    );
    shell.destroy(dialog);
    assert_eq!(
        background.focus(),
        Some(normal),
        "restore original app, not late app"
    );
    assert!(
        background.closed().is_empty(),
        "dismissal sent a close/power action"
    );
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    action(&compositor, format!("focus-window {id}"));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn power_confirmation_client_cancel_restores_focus_and_preserves_held_outside_release() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.PowerCancel".into(),
        (600, 400),
        (0, 0),
        COLOR,
    );
    let dialog = client.add(POWER_CONFIRM_APP_ID.into(), (420, 220), (0, 0), COLOR);
    action(&compositor, "pointer-motion-abs 400 400");
    action(&compositor, "pointer-button-down 272");
    client.destroy(dialog); // Native Cancel closes its window without a power action.
    assert_eq!(client.focus(), Some(normal));
    action(&compositor, "pointer-button-up 272");
    assert!(client.presses().is_empty());
    assert!(
        client.releases().is_empty(),
        "outside release leaked after cancellation"
    );
    assert!(!client.closed().contains(&normal));
    click(&compositor, 400., 400.);
    assert_eq!(client.presses().last().map(|press| press.0), Some(normal));
    let releases = client.releases().len();
    let dialog = client.add(POWER_CONFIRM_APP_ID.into(), (420, 220), (0, 0), COLOR);
    action(&compositor, "pointer-motion-abs 450 310");
    action(&compositor, "pointer-button-down 272");
    keys(&compositor, &[1]);
    assert!(client.closed().contains(&dialog));
    client.destroy(dialog);
    assert_eq!(
        client.focus(),
        Some(normal),
        "held modal press delayed focus restoration"
    );
    action(&compositor, "pointer-button-up 272");
    assert!(
        client.releases()[releases..]
            .iter()
            .all(|release| release.0 != normal),
        "Escape redirected the dialog's held release to the background"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
