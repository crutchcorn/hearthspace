//! A buffer unmap retires modal ownership without destroying its Wayland objects.
use super::*;

#[rstest::rstest]
#[case::left(0x110, None)]
#[case::right(0x111, None)]
#[case::secondary_after_left_release(0x110, Some(0x111))]
fn power_confirmation_unmap_remap_restores_focus_and_drains_held_pointer(
    #[case] button: u32,
    #[case] secondary: Option<u32>,
) {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut background = connect(&compositor);
    let original = background.add(
        "dev.hearthspace.PowerUnmapOriginal".into(),
        (600, 400),
        (0, 0),
        COLOR,
    );
    let original_id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();
    let viewport = query(&compositor, "status")["viewport"].clone();
    let mut shell = connect(&compositor);
    let dialog = shell.add(POWER_CONFIRM_APP_ID.into(), (420, 220), (8, 6), COLOR);
    let held_button = secondary.unwrap_or(button);
    let presses_per_sequence = if secondary.is_some() { 2 } else { 1 };
    assert_eq!(shell.focus(), Some(dialog));

    // Ordinary fallback chooses the newest app. Modal teardown must instead
    // remember the app focused before this dialog was accepted.
    background.add(
        "dev.hearthspace.PowerUnmapLate".into(),
        (200, 140),
        (0, 0),
        COLOR,
    );
    assert_eq!(background.focus(), None);
    action(&compositor, "pointer-motion-abs 450 310");
    action(&compositor, format!("pointer-button-down {button}"));
    if let Some(secondary) = secondary {
        action(&compositor, format!("pointer-button-down {secondary}"));
        action(&compositor, format!("pointer-button-up {button}"));
    }
    assert_eq!(
        shell.presses(),
        vec![(dialog, (28., 26.)); presses_per_sequence]
    );

    // Commit a null buffer, keeping the wl_surface and xdg_toplevel alive.
    // Keyboard recovery must not wait for this dialog's held pointer release.
    shell.unmap(dialog);
    assert_eq!(shell.focus(), None);
    assert_eq!(
        background.focus(),
        Some(original),
        "null-buffer unmap must restore the original app while the press is held"
    );
    assert_eq!(
        query(&compositor, "status")["shell_panels"],
        serde_json::json!([])
    );
    action(&compositor, "pointer-motion-abs 400 400");
    action(&compositor, format!("pointer-button-up {held_button}"));
    assert!(background.presses().is_empty());
    assert!(
        background.releases().is_empty(),
        "unmapping redirected the dialog's release to the background"
    );

    // Reuse exactly the same protocol objects after the old sequence drains.
    // Focus alone is insufficient: a plain shell panel would also take focus.
    shell.remap(dialog, POWER_CONFIRM_APP_ID);
    assert_eq!(shell.focus(), Some(dialog));
    assert!(!shell.closed().contains(&dialog));
    assert!(
        compositor
            .send_text_command(&format!("focus-window {original_id}"))
            .starts_with("err "),
        "remapped dialog did not reacquire modal ownership"
    );
    click(&compositor, 400., 400.);
    assert!(background.presses().is_empty());
    assert!(background.releases().is_empty());
    assert_eq!(shell.focus(), Some(dialog));

    // Destruction after remapping must retire that new ownership, again without
    // redirecting the press or deferring keyboard restoration until release.
    action(&compositor, "pointer-motion-abs 450 310");
    action(&compositor, format!("pointer-button-down {button}"));
    if let Some(secondary) = secondary {
        action(&compositor, format!("pointer-button-down {secondary}"));
        action(&compositor, format!("pointer-button-up {button}"));
    }
    assert_eq!(
        shell.presses(),
        vec![(dialog, (28., 26.)); 2 * presses_per_sequence]
    );
    shell.destroy(dialog);
    assert_eq!(
        background.focus(),
        Some(original),
        "destroying the remapped dialog must restore its saved app focus"
    );
    action(&compositor, "pointer-motion-abs 400 400");
    action(&compositor, format!("pointer-button-up {held_button}"));
    assert!(background.presses().is_empty());
    assert!(
        background.releases().is_empty(),
        "destroying the remapped dialog leaked its held release"
    );

    let replacement = shell.add(POWER_CONFIRM_APP_ID.into(), (420, 220), (8, 6), COLOR);
    assert_eq!(shell.focus(), Some(replacement));
    assert!(!shell.closed().contains(&replacement));
    assert!(
        compositor
            .send_text_command(&format!("focus-window {original_id}"))
            .starts_with("err "),
        "retired ownership prevented a fresh modal from being accepted"
    );
    click(&compositor, 450., 310.);
    assert_eq!(shell.presses().last(), Some(&(replacement, (28., 26.))));
    shell.destroy(replacement);
    assert_eq!(background.focus(), Some(original));
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    assert!(background.closed().is_empty());

    click(&compositor, 400., 400.);
    assert_eq!(
        background.presses().last().map(|event| event.0),
        Some(original)
    );
    assert_eq!(
        background.releases().last().map(|event| event.0),
        Some(original)
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
