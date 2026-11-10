use super::*;
use crate::compositor::udev::{identity::DeviceToken, presenter::OutputKey};
use std::num::NonZeroU32;

fn key(generation: u64) -> OutputKey {
    OutputKey {
        device: DeviceToken {
            id: 226,
            incarnation: 1,
        },
        crtc: NonZeroU32::new(7).unwrap().into(),
        generation,
    }
}

#[test]
fn admission_and_pending_replacement_do_not_claim_a_new_scanout() {
    let mut output = OutputPresentation::new(key(1));
    output.queued(10);
    assert_eq!(output.scanout_status(true)["state"], "unknown");
    assert!(output.scanout_status(true)["primary_scanout"].is_null());

    let planes = PresentedPlanes::new(true, Some(42), true);
    assert_eq!(
        output.presented_with_planes(key(1), 1, 10, planes),
        Some(true)
    );
    output.queued(11);
    let current = output.scanout_status(true);
    assert_eq!(current["state"], "direct-scanout");
    assert_eq!(current["window_id"], 42);
    assert_eq!(current["scene_sequence"], 10);
    assert_eq!(current["cursor_plane"], true);

    // A cursor-only completion still presents the old scene, even while a
    // replacement scene is pending. Associate its app with that exact frame.
    assert_eq!(
        output.presented_with_planes(key(1), 2, 10, planes),
        Some(false)
    );
    assert!(output.pending());
    assert_eq!(output.scanout_status(true)["window_id"], 42);

    let composed = PresentedPlanes::new(false, Some(42), false);
    assert_eq!(
        output.presented_with_planes(key(1), 3, 11, composed),
        Some(true)
    );
    let current = output.scanout_status(true);
    assert_eq!(current["state"], "composited");
    assert_eq!(current["primary_scanout"], false);
    assert!(current["window_id"].is_null());
    assert_eq!(current["submission_sequence"], 3);
}

#[test]
fn stale_events_cannot_replace_the_reported_application() {
    let mut output = OutputPresentation::new(key(2));
    output.queued(20);
    output.presented_with_planes(key(2), 5, 20, PresentedPlanes::new(true, Some(42), false));
    let stale = PresentedPlanes::new(true, Some(99), false);
    assert_eq!(output.presented_with_planes(key(1), 6, 20, stale), None);
    assert_eq!(output.presented_with_planes(key(2), 4, 20, stale), None);
    assert_eq!(output.presented_with_planes(key(2), 6, 21, stale), None);
    assert_eq!(output.scanout_status(true)["window_id"], 42);
    assert_eq!(output.scanout_status(true)["submission_sequence"], 5);
}

#[test]
fn recovery_and_security_changes_do_not_report_stale_scanout() {
    let mut output = OutputPresentation::new(key(1));
    output.queued(10);
    output.presented_with_planes(key(1), 1, 10, PresentedPlanes::new(true, Some(42), false));
    assert_eq!(output.scanout_status(false)["state"], "unknown");
    assert!(output.scanout_status(false)["window_id"].is_null());
    output.invalidate();
    assert_eq!(output.scanout_status(true)["state"], "unknown");
    assert!(output.scanout_status(true)["window_id"].is_null());
    let replacement = OutputPresentation::new(key(2));
    assert_eq!(replacement.scanout_status(true)["state"], "unknown");
}
