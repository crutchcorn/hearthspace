//! Cursor handoff goes through the real KMS acknowledgement/cutoff boundary.
use super::*;
use crate::compositor::udev::device::cursor::{
    policy::CursorPlane,
    tests::{Drm, Op, scene, target},
};
use std::time::Duration;

const A: DeviceToken = DeviceToken {
    id: 1,
    incarnation: 4,
};
const B: DeviceToken = DeviceToken {
    id: 2,
    incarnation: 5,
};

fn output(device: DeviceToken, name: &str) -> (OutputPresentation, ()) {
    (
        OutputPresentation::new(device, 1, name.into(), SurfaceEventCutoff::now()),
        (),
    )
}

fn event() -> DrmEventMetadata {
    DrmEventMetadata {
        time: DrmEventTime::Monotonic(Clock::<Monotonic>::new().now().into()),
        sequence: 12,
    }
}

fn complete(
    outputs: &mut [(OutputPresentation, ())],
    device: DeviceToken,
    metadata: Option<&DrmEventMetadata>,
    submitted: bool,
) -> Submission {
    complete_frame(
        outputs
            .iter_mut()
            .map(|(presentation, buffer)| (presentation, buffer)),
        device,
        1,
        metadata,
        |_| Ok(submitted),
    )
    .unwrap()
}

#[test]
fn only_matching_verified_submission_retires_the_original_cursor() {
    let mut outputs = [output(A, "A"), output(B, "B")];
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    plane.update(target(1), false, Default::default(), &mut drm);
    let original = plane.active;
    outputs[0].0.queued(None, true);
    outputs[1].0.queued(None, false);
    drm.calls.clear();

    assert!(matches!(
        complete(&mut outputs, B, Some(&event()), true),
        Submission::Verified { .. }
    ));
    let current = scene(&[&outputs[0].0.cursor, &outputs[1].0.cursor], 0);
    assert!(plane.update(target(1), true, current, &mut drm));
    assert_eq!(plane.active, original);
    assert!(drm.calls.is_empty());

    let old_device = DeviceToken {
        incarnation: 3,
        ..A
    };
    assert!(matches!(
        complete(&mut outputs, old_device, Some(&event()), true),
        Submission::Ignored
    ));
    assert!(matches!(
        complete(&mut outputs, A, None, true),
        Submission::Ignored
    ));
    let stale = DrmEventMetadata {
        time: DrmEventTime::Monotonic(Duration::ZERO),
        sequence: 12,
    };
    assert!(matches!(
        complete(&mut outputs, A, Some(&stale), true),
        Submission::Ignored
    ));
    assert!(matches!(
        complete(&mut outputs, A, Some(&event()), false),
        Submission::Ignored
    ));
    assert!(!outputs[0].0.cursor.displayed);
    assert!(outputs[0].0.cursor.may_arrive());
    let current = scene(&[&outputs[0].0.cursor, &outputs[1].0.cursor], 0);
    assert!(plane.update(target(1), true, current, &mut drm));
    assert_eq!(plane.active, original);

    assert!(matches!(
        complete(&mut outputs, A, Some(&event()), true),
        Submission::Verified { .. }
    ));
    let current = scene(&[&outputs[0].0.cursor, &outputs[1].0.cursor], 0);
    assert!(plane.update(target(1), true, current, &mut drm));
    assert!(plane.active.is_none());
    assert_eq!(drm.calls, [Op::Clear(target(1).crtc)]);
}

#[test]
fn untimed_modeset_does_not_authorize_handoff_or_hardware_recovery() {
    let mut outputs = [output(A, "A")];
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    plane.update(target(1), false, Default::default(), &mut drm);
    outputs[0].0.queued(None, true);
    let untimed = DrmEventMetadata {
        time: DrmEventTime::Monotonic(Duration::ZERO),
        sequence: 0,
    };
    assert!(matches!(
        complete(&mut outputs, A, Some(&untimed), true),
        Submission::Untimed(_)
    ));
    assert!(!outputs[0].0.cursor.displayed);
    assert!(outputs[0].0.cursor.may_arrive());
    let current = scene(&[&outputs[0].0.cursor], 0);
    assert!(plane.update(target(1), true, current, &mut drm));
    assert!(plane.active.is_some());

    // A different plane with no retained image cannot enable hardware while
    // the untimed frame may have left software in scanout.
    let mut recovering = CursorPlane::default();
    let mut recovering_drm = Drm::default();
    assert!(!recovering.update(target(1), false, current, &mut recovering_drm));
    assert!(recovering_drm.calls.is_empty());

    outputs[0].0.queued(None, true);
    assert!(matches!(
        complete(&mut outputs, A, Some(&event()), true),
        Submission::Verified { .. }
    ));
    assert!(plane.update(target(1), true, scene(&[&outputs[0].0.cursor], 0), &mut drm));
    assert!(plane.active.is_none());
}

#[test]
fn rebuild_retires_pending_cursor_metadata_and_rejects_old_incarnation() {
    let mut outputs = [output(A, "A")];
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    plane.update(target(1), false, Default::default(), &mut drm);
    outputs[0].0.queued(None, true);
    plane.reset(); // same invalidation used by seat pause and surface rebuild
    let replacement = DeviceToken {
        incarnation: 6,
        ..A
    };
    outputs[0] = output(replacement, "A");
    outputs[0].0.queued(None, false);
    assert!(matches!(
        complete(&mut outputs, A, Some(&event()), true),
        Submission::Ignored
    ));
    assert!(!outputs[0].0.cursor.displayed);
    assert!(!outputs[0].0.cursor.may_arrive());
    assert!(plane.active.is_none());
    assert!(matches!(
        complete(&mut outputs, replacement, Some(&event()), true),
        Submission::Verified { .. }
    ));
    assert!(!plane.update(
        target(1),
        false,
        scene(&[&outputs[0].0.cursor], 0),
        &mut drm
    ));
    assert!(plane.active.is_some());
}
