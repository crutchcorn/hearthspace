use super::*;
use crate::compositor::udev::device::cursor::policy::CursorPlane;

#[test]
fn non_owner_device_clears_inherited_planes_once_per_lifecycle() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let crtcs = [target(1).crtc, target(2).crtc];
    drm.visible.extend(crtcs);
    assert!(
        plane.active.is_none(),
        "fresh policy knows no previous owner"
    );
    plane.initialize(crtcs.into_iter(), &mut drm);
    assert!(drm.visible.is_empty());
    assert_eq!(
        drm.calls,
        [Op::ClearAll, Op::Clear(crtcs[0]), Op::Clear(crtcs[1])]
    );
    drm.calls.clear();
    plane.initialize(crtcs.into_iter(), &mut drm);
    plane.retire_previous(Scene::default(), false, &mut drm);
    assert!(
        drm.calls.is_empty(),
        "ordinary non-owner updates do not reinitialize"
    );

    plane.reset();
    drm.visible.insert(crtcs[1]);
    plane.initialize(crtcs.into_iter(), &mut drm);
    assert!(
        drm.visible.is_empty(),
        "rebuild or seat resume clears inherited KMS state again"
    );
    assert_eq!(drm.calls.first(), Some(&Op::ClearAll));
}

#[test]
fn failed_same_device_migration_preserves_a_until_b_presents_software() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let a = target(1);
    let b = target(2);
    plane.update(a, false, Scene::default(), &mut drm);
    let original = plane.active;
    drm.calls.clear();
    drm.set_error = true;
    assert!(plane.update(b, false, Scene::default(), &mut drm));
    assert_eq!(drm.calls, [Op::Set(b.crtc)]);
    assert_eq!(plane.active, original);
    assert_eq!(drm.visible, [a.crtc].into());

    let mut a_frame = CursorFrame::default();
    let mut b_frame = CursorFrame::default();
    a_frame.queued(false);
    a_frame.completed(true);
    b_frame.queued(true);
    assert!(plane.update(b, false, scene(&[&a_frame, &b_frame], 1), &mut drm));
    assert_eq!(drm.visible, [a.crtc].into());
    b_frame.completed(true);
    assert!(plane.update(b, false, scene(&[&a_frame, &b_frame], 1), &mut drm));
    assert!(drm.visible.is_empty());
    assert!(plane.active.is_none());
}

#[test]
fn migration_positions_b_before_retiring_a_and_tracks_failed_retirement() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let a = target(1);
    let b = target(2);
    plane.update(a, false, Scene::default(), &mut drm);
    drm.calls.clear();
    drm.clear_error = true;
    assert!(!plane.update(b, false, Scene::default(), &mut drm));
    assert_eq!(
        drm.calls,
        [
            Op::Set(b.crtc),
            Op::Move(b.crtc, (98, 77)),
            Op::Clear(a.crtc)
        ]
    );
    assert_eq!(plane.active.unwrap().crtc, b.crtc);
    assert_eq!(drm.visible, [a.crtc, b.crtc].into());
    assert!(plane.clear_retry_pending());
    drm.clear_error = false;
    plane.retry_clear();
    drm.calls.clear();
    assert!(!plane.update(b, false, Scene::default(), &mut drm));
    assert_eq!(drm.calls, [Op::Clear(a.crtc)]);
    assert_eq!(drm.visible, [b.crtc].into());
    assert!(!plane.clear_retry_pending());
}

#[test]
fn failed_initial_move_on_b_keeps_a_and_cleans_only_the_failed_b() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let a = target(1);
    let b = target(2);
    plane.update(a, false, Scene::default(), &mut drm);
    let original = plane.active;
    drm.calls.clear();
    drm.move_error = true;
    assert!(plane.update(b, false, Scene::default(), &mut drm));
    assert_eq!(
        drm.calls,
        [
            Op::Set(b.crtc),
            Op::Move(b.crtc, (98, 77)),
            Op::Clear(b.crtc)
        ]
    );
    assert_eq!(plane.active, original);
    assert_eq!(drm.visible, [a.crtc].into());
}
