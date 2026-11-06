//! Main-side scene association after the worker has validated presentation.
//! These tests do not exercise DRM timestamps, GBM acknowledgement, or KMS.
use super::{OutputKey, OutputPresentation};
use crate::compositor::udev::identity::DeviceToken;
use std::num::NonZeroU32;

const A: DeviceToken = DeviceToken {
    id: 226,
    incarnation: 2,
};
const B: DeviceToken = DeviceToken {
    id: 227,
    incarnation: 2,
};

fn key(device: DeviceToken, crtc: u32, generation: u64) -> OutputKey {
    OutputKey {
        device,
        crtc: NonZeroU32::new(crtc).unwrap().into(),
        generation,
    }
}

#[test]
fn colliding_output_identities_cannot_complete_or_discard_the_current_scene() {
    let current = key(A, 7, 3);
    let collisions = [
        ("retired surface generation", key(A, 7, 2)),
        (
            "recycled device id",
            key(
                DeviceToken {
                    incarnation: 1,
                    ..A
                },
                7,
                3,
            ),
        ),
        ("same CRTC on another GPU", key(B, 7, 3)),
        ("another CRTC on the same GPU", key(A, 8, 3)),
    ];
    for (case, stale) in collisions {
        let mut output = OutputPresentation::new(current);
        output.queued(41);

        assert_eq!(output.presented(stale, 900, 41), None, "{case}");
        assert!(!output.discarded(stale, 41), "{case}");
        assert!(output.pending(), "{case}");
        assert!(!output.presented_once, "{case}");
        assert!(!output.dirty, "{case}");
        // A rejected event must not advance the live output's submission order.
        assert_eq!(output.presented(current, 1, 41), Some(true), "{case}");
        assert!(!output.pending(), "{case}");
        assert!(output.presented_once, "{case}");
    }
}

#[test]
fn duplicate_and_reordered_submissions_do_not_consume_a_newer_pending_scene() {
    let key = key(A, 7, 3);
    let mut output = OutputPresentation::new(key);
    output.queued(40);
    assert_eq!(output.presented(key, 100, 40), Some(true));
    output.queued(41);
    output.dirty = true;

    assert_eq!(output.presented(key, 99, 41), None);
    assert_eq!(output.presented(key, 100, 41), None);
    assert!(output.pending());
    assert!(output.dirty);
    assert_eq!(output.presented(key, 101, 41), Some(true));
    assert!(!output.pending());
    assert!(output.dirty);
    assert_eq!(output.presented(key, 101, 41), None);
}

#[test]
fn unproduced_or_regressed_scenes_do_not_advance_submission_order() {
    let key = key(A, 7, 3);
    let mut output = OutputPresentation::new(key);
    output.queued(40);
    assert_eq!(output.presented(key, 100, 40), Some(true));
    output.queued(41);

    assert_eq!(output.presented(key, 103, 42), None);
    assert_eq!(output.presented(key, 102, 39), None);
    assert!(output.pending());
    assert!(!output.dirty);
    assert_eq!(output.presented(key, 101, 41), Some(true));
    assert!(!output.pending());
    // Even a newer submission cannot roll the acknowledged scene backwards.
    assert_eq!(output.presented(key, 102, 40), None);
    assert_eq!(output.presented(key, 102, 41), Some(false));
}

#[test]
fn cursor_only_old_scene_completion_preserves_the_new_scene_and_its_redraw() {
    let key = key(A, 7, 3);
    let mut output = OutputPresentation::new(key);
    output.queued(40);
    assert_eq!(output.presented(key, 100, 40), Some(true));
    output.queued(41);
    output.dirty = true; // More scene work arrived while this submission waited.

    assert_eq!(output.presented(key, 101, 40), Some(false));
    assert!(output.pending());
    assert!(output.dirty);
    assert_eq!(output.presented(key, 102, 40), Some(false));
    assert!(output.pending());
    assert_eq!(output.presented(key, 103, 41), Some(true));
    assert!(!output.pending());
    assert!(output.dirty);
}

#[test]
fn only_the_exact_pending_scene_can_be_discarded_and_retried() {
    let key = key(A, 7, 3);
    let mut output = OutputPresentation::new(key);
    output.queued(40);
    assert_eq!(output.presented(key, 100, 40), Some(true));
    output.queued(41);

    assert!(!output.discarded(key, 40));
    assert!(!output.discarded(key, 42));
    assert!(output.pending());
    assert!(!output.dirty);
    assert!(output.discarded(key, 41));
    assert!(!output.pending());
    assert!(output.dirty);
    assert!(output.presented_once);
    assert!(!output.discarded(key, 41));

    output.queued(42);
    assert!(!output.discarded(key, 41));
    assert!(output.pending());
    assert!(!output.dirty);
    assert_eq!(output.presented(key, 101, 42), Some(true));
    assert!(!output.pending());
}

#[test]
fn security_invalidation_allows_a_shield_without_completing_the_next_scene() {
    let key = key(A, 7, 3);
    let mut output = OutputPresentation::new(key);
    output.queued(40);
    assert_eq!(output.presented(key, 100, 40), Some(true));
    output.queued(41);
    output.invalidate();

    assert!(!output.pending());
    assert!(!output.presented_once);
    assert!(output.dirty);
    assert!(!output.discarded(key, 41));
    // The caller validates the security epoch. Invalidation must still retain
    // submission ordering, while allowing the worker's new scene-zero shield.
    assert_eq!(output.presented(key, 100, 0), None);
    assert!(!output.presented_once);
    assert_eq!(output.presented(key, 101, 0), Some(false));
    assert!(output.presented_once);
    assert!(!output.pending());
    assert!(output.dirty);

    output.queued(42);
    assert_eq!(output.presented(key, 102, 0), Some(false));
    assert!(output.pending());
    assert_eq!(output.presented(key, 103, 42), Some(true));
    assert!(!output.pending());
}

#[test]
fn clean_scene_completion_leaves_the_output_idle() {
    let key = key(A, 7, 3);
    let mut output = OutputPresentation::new(key);
    assert!(output.dirty);
    assert!(!output.pending());
    assert!(!output.presented_once);

    output.queued(1);
    assert!(output.pending());
    assert!(!output.dirty);
    assert_eq!(output.presented(key, 1, 1), Some(true));
    assert!(!output.pending());
    assert!(!output.dirty);
    assert!(output.presented_once);
    assert_eq!(output.presented(key, 2, 1), Some(false));
    assert!(!output.dirty);
}

#[test]
fn outputs_with_identical_crtcs_and_sequences_progress_independently() {
    let a_key = key(A, 7, 3);
    let b_key = key(B, 7, 3);
    let mut a = OutputPresentation::new(a_key);
    let mut b = OutputPresentation::new(b_key);
    a.queued(40);
    b.queued(40);

    assert_eq!(a.presented(a_key, 100, 40), Some(true));
    assert!(!a.pending());
    assert!(b.pending());
    assert!(!b.presented_once);
    a.queued(41);
    assert_eq!(a.presented(a_key, 101, 41), Some(true));
    assert!(b.pending());

    a.queued(42);
    a.dirty = true;
    assert_eq!(b.presented(b_key, 100, 40), Some(true));
    assert!(!b.pending());
    assert!(a.pending());
    assert!(a.dirty);
    b.queued(41);
    assert_eq!(a.presented(a_key, 102, 42), Some(true));
    assert!(a.dirty);
    assert!(b.pending());
    assert_eq!(b.presented(b_key, 101, 41), Some(true));
    assert!(!a.pending());
    assert!(!b.pending());
    assert!(!b.dirty);
    assert!(a.presented_once && b.presented_once);
}

#[test]
fn staggered_output_completions_drain_scene_work_without_redraw_ping_pong() {
    let a_key = key(A, 7, 3);
    let b_key = key(B, 7, 3);
    let mut a = OutputPresentation::new(a_key);
    let mut b = OutputPresentation::new(b_key);
    a.queued(1);
    b.queued(1);
    // Real scene damage arrives while both outputs are busy.
    assert!(!a.request_scene(true));
    assert!(!b.request_scene(true));
    assert_eq!(a.presented(a_key, 1, 1), Some(true));
    assert!(a.request_scene(false));
    assert!(!b.request_scene(false));
    a.queued(2);
    assert_eq!(b.presented(b_key, 1, 1), Some(true));
    // Retrying B cannot dirty A's already submitted replacement.
    assert!(!a.request_scene(false));
    assert!(b.request_scene(false));
    b.queued(2);
    assert_eq!(a.presented(a_key, 2, 2), Some(true));
    assert!(!a.request_scene(false));
    assert!(!b.request_scene(false));
    assert_eq!(b.presented(b_key, 2, 2), Some(true));
    assert!(!a.needs_scene() && !b.needs_scene());
    assert!(!a.dirty && !b.dirty);
}
