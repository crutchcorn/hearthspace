//! Temporary pan shares source ownership, cancellation, and seat recovery with Pan.
use super::*;
use crate::{remote_input::client::RemoteClient, session_lock::lock_client::LockClient};

#[rstest::rstest]
#[case::local_owner(false)]
#[case::remote_owner(true)]
fn meta_pan_only_origin_motion_and_release_control_the_transform(#[case] remote_owner: bool) {
    let _guard = headless_test_guard();
    let compositor = fixture(true);
    let mut client = clients(&compositor);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    action(&compositor, "key-down 125");
    if remote_owner {
        remote.motion(500, 500, 1280, 800);
        remote.button(true);
        remote.motion(550, 550, 1280, 800);
        motion(&compositor, (700., 700.));
        action(&compositor, "pointer-button-down 272");
    } else {
        start(&compositor, (500., 500.));
        motion(&compositor, (550., 550.));
        remote.motion(700, 700, 1280, 800);
        remote.button(true);
    }
    assert_eq!(offset(&compositor), [-50, -50], "foreign motion drove pan");
    action(&compositor, "key-up 125");
    if remote_owner {
        finish(&compositor); // Foreign release cannot end the owner's drag.
        remote.motion(600, 600, 1280, 800);
        action(&compositor, "pointer-button-down 272");
        remote.button(false); // Local still holds Smithay's aggregate button.
        remote.motion(650, 650, 1280, 800);
    } else {
        remote.button(false);
        motion(&compositor, (600., 600.));
        remote.button(true);
        finish(&compositor);
        motion(&compositor, (650., 650.));
    }
    assert_eq!(
        offset(&compositor),
        [-100, -100],
        "source releases lost ownership"
    );
    assert_silent(&mut client, "source aggregate leaked before it drained");
    if remote_owner {
        finish(&compositor);
    } else {
        remote.button(false);
    }
    assert_buttons(&client.take_pointer_events(), &[]);
    assert_eq!(tool(&compositor), "select");
    action(&compositor, "key-down 125");
    start(&compositor, (650., 650.));
    motion(&compositor, (670., 670.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [-120, -120]);
}

#[test]
fn meta_pan_escape_drains_secondary_buttons_and_scroll_before_normal_client_input() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, "pointer-button-down 273");
    keys(&compositor, &[1]);
    let stopped = viewport(&compositor);
    motion(&compositor, (600., 600.));
    finish(&compositor);
    action(&compositor, "axis 0 -120");
    motion(&compositor, (200., 200.));
    action(&compositor, "key-up 125");
    assert_eq!(viewport(&compositor), stopped);
    assert_eq!(tool(&compositor), "select");
    assert_silent(&mut client, "cancelled temporary pan released input early");
    action(&compositor, "pointer-button-up 273");
    assert_buttons(&client.take_pointer_events(), &[]);
    start(&compositor, (200., 200.));
    finish(&compositor);
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Pressed), (272, ButtonState::Released)],
    );
    assert_eq!(viewport(&compositor), stopped);
}

#[test]
fn meta_pan_lock_and_reset_discard_old_modifiers_and_releases() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, "pointer-button-down 273");
    action(&compositor, "session-resume");
    finish(&compositor);
    action(&compositor, "pointer-button-up 273");
    action(&compositor, "key-up 125");
    motion(&compositor, (600., 600.));
    assert_eq!(offset(&compositor), [-50, -50]);
    start(&compositor, (600., 600.));
    motion(&compositor, (620., 620.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [-50, -50], "reset left Meta pan armed");
    action(&compositor, "key-down 125");
    start(&compositor, (620., 620.));
    motion(&compositor, (640., 640.));
    assert_eq!(offset(&compositor), [-70, -70]);
    let mut locker = LockClient::connect(&socket(&compositor));
    locker.wait_locked();
    locker.unlock();
    finish(&compositor);
    action(&compositor, "key-up 125");
    motion(&compositor, (650., 650.));
    assert_eq!(offset(&compositor), [-70, -70]);
    assert_buttons(&client.take_pointer_events(), &[]);
    action(&compositor, "key-down 125");
    start(&compositor, (650., 650.));
    motion(&compositor, (670., 670.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [-90, -90]);
}

#[rstest::rstest]
#[case::layout("canvas-tool layout")]
#[case::open_search("canvas-search-open")]
fn meta_pan_does_not_override_modal_canvas_tools(#[case] enter: &str) {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, enter);
    let before = viewport(&compositor);
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(viewport(&compositor), before);
    assert_buttons(&client.take_pointer_events(), &[]);
}
