use super::*;

pub(super) fn session() -> UiTouch {
    UiTouch {
        device: "screen".into(),
        slot: Some(2).into(),
        window: 7,
        geometry: [30.0, 20.0, 2.0, 400.0, 300.0],
        output: Rectangle::new((0, 0).into(), (1000, 800).into()),
        origin: (100.0, 80.0).into(),
        local: (20.0, 15.0).into(),
        target: Target::Bar(BarSwipe::new((100.0, 80.0).into(), Instant::now(), None)),
        apps_identity: None,
    }
}

#[test]
fn surface_coordinates_follow_scale_and_original_contact_owner() {
    let touch = session();
    assert_eq!(
        touch.local_position((180.0, 40.0).into()),
        (60.0, -5.0).into()
    );
    assert!(touch.owns("screen", Some(2).into()));
    assert!(!touch.owns("other-screen", Some(2).into()));
    assert!(!touch.owns("screen", Some(1).into()));
}

#[test]
fn stale_window_geometry_output_or_locked_session_invalidates_delivery() {
    let touch = session();
    assert!(touch.valid(Some(touch.geometry), &[touch.output], true, None));
    assert!(!touch.valid(None, &[touch.output], true, None));
    assert!(!touch.valid(Some(touch.geometry), &[], true, None));
    assert!(!touch.valid(Some(touch.geometry), &[touch.output], false, None));
    let mut moved = touch.geometry;
    moved[0] += 1.0;
    assert!(!touch.valid(Some(moved), &[touch.output], true, None));
}

#[test]
fn held_dock_touch_cannot_activate_a_replacement_in_the_same_surface_bounds() {
    let mut touch = session();
    let original = vec![(1, Some("editor".into())), (2, Some("browser".into()))];
    touch.apps_identity = Some(original.clone());
    assert!(touch.valid(Some(touch.geometry), &[touch.output], true, Some(&original)));
    let replacement = vec![(2, Some("browser".into())), (3, Some("terminal".into()))];
    assert!(!touch.valid(
        Some(touch.geometry),
        &[touch.output],
        true,
        Some(&replacement)
    ));
    let regrouped = vec![(1, Some("terminal".into())), (2, Some("browser".into()))];
    assert!(!touch.valid(
        Some(touch.geometry),
        &[touch.output],
        true,
        Some(&regrouped)
    ));
    touch.cancel();
    assert!(matches!(touch.target, Target::Cancelled));
    assert!(touch.suppresses_pointer("screen", &TouchAction::Release));
}

#[test]
fn cancelled_touch_never_leaks_pointer_clicks_but_global_gestures_still_pass() {
    let mut touch = session();
    assert!(!touch.cancel());
    assert!(matches!(touch.target, Target::Cancelled));
    for action in [
        TouchAction::Press((1.0, 2.0).into()),
        TouchAction::Motion((2.0, 8.0).into()),
        TouchAction::Tap((3.0, 4.0).into()),
        TouchAction::Release,
    ] {
        assert!(touch.suppresses_pointer("screen", &action));
        assert!(!touch.suppresses_pointer("other-screen", &action));
    }
    assert!(!touch.suppresses_pointer(
        "screen",
        &TouchAction::BeginPinch {
            anchor: (3.0, 4.0).into()
        }
    ));
    assert!(!touch.suppresses_pointer("screen", &TouchAction::EndOverview { cancelled: false }));
}
