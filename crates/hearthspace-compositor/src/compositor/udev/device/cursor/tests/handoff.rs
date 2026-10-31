use super::*;
use crate::compositor::udev::device::cursor::policy::CursorPlane;

#[test]
fn retained_hardware_moves_until_the_submitted_software_image_completes() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let mut frame = CursorFrame::default();
    let mut pointer = target(1);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    let original = plane.active;
    drm.calls.clear();

    assert!(plane.update(pointer, true, scene(&[&frame], 0), &mut drm));
    assert_eq!(plane.active, original);
    assert!(drm.calls.is_empty());
    frame.queued(true);
    pointer.local.0 += 20.0;
    assert!(plane.update(pointer, true, scene(&[&frame], 0), &mut drm));
    assert_eq!(drm.calls, [Op::Move(pointer.crtc, (118, 77))]);

    frame.completed(true);
    assert!(plane.update(pointer, true, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_none());
    assert_eq!(drm.calls.last(), Some(&Op::Clear(pointer.crtc)));
}

#[test]
fn a_frame_without_imported_cursor_pixels_does_not_retire_hardware() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let mut frame = CursorFrame::default();
    plane.update(target(1), false, Scene::default(), &mut drm);
    let original = plane.active;
    // Software was requested, but the renderer produced no cursor element.
    frame.queued(false);
    frame.completed(true);
    assert!(plane.update(target(1), true, scene(&[&frame], 0), &mut drm));
    assert_eq!(plane.active, original);
    frame.queued(true);
    frame.completed(true);
    assert!(plane.update(target(1), true, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_none());
}

#[test]
fn hardware_recovery_waits_for_software_removal_and_uses_latest_location() {
    let mut frame = CursorFrame::default();
    frame.queued(true);
    frame.completed(true);
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let mut pointer = target(1);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(drm.calls.is_empty());
    frame.queued(false);
    pointer.local = (200.0, 120.0);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(drm.calls.is_empty());
    frame.completed(true);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert_eq!(plane.active.unwrap().position, Some((198, 117)));
}

#[test]
fn superseded_software_frame_is_retired_before_hardware_can_reappear() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let mut frame = CursorFrame::default();
    let pointer = target(1);
    plane.update(pointer, false, scene(&[&frame], 0), &mut drm);
    frame.queued(true);
    // The client request is superseded before its already queued pixels arrive.
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_some());
    frame.completed(true);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(
        plane.active.is_none(),
        "the older software pixels are now visible"
    );
    frame.queued(false);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_none());
    frame.completed(true);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_some());
}

#[test]
fn output_migration_keeps_old_hardware_until_replacement_output_has_pixels() {
    let mut old = CursorPlane::default();
    let mut new = CursorPlane::default();
    let mut old_drm = Drm::default();
    let mut new_drm = Drm::default();
    let mut a = CursorFrame::default();
    let mut b = CursorFrame::default();
    old.update(target(1), false, Scene::default(), &mut old_drm);
    b.queued(true);
    new.update(target(1), true, scene(&[&a, &b], 1), &mut new_drm);
    old.retire_previous(scene(&[&a, &b], 1), false, &mut old_drm);
    assert!(old.active.is_some());
    // Another output's clean flip is not this cursor's replacement.
    a.queued(false);
    a.completed(true);
    old.retire_previous(scene(&[&a, &b], 1), false, &mut old_drm);
    assert!(old.active.is_some());
    b.completed(true);
    old.retire_previous(scene(&[&a, &b], 1), false, &mut old_drm);
    assert!(old.active.is_none());
    assert!(new.active.is_none());
}

#[test]
fn hardware_migration_waits_for_old_outputs_software_to_retire() {
    let mut a = CursorFrame::default();
    let b = CursorFrame::default();
    a.queued(true);
    a.completed(true);
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    assert!(!plane.update(target(2), false, scene(&[&a, &b], 1), &mut drm));
    assert!(plane.active.is_none());
    a.queued(false);
    a.completed(true);
    assert!(!plane.update(target(2), false, scene(&[&a, &b], 1), &mut drm));
    assert_eq!(plane.active.unwrap().crtc, target(2).crtc);
}

#[test]
fn superseded_software_on_another_output_cannot_retire_current_hardware() {
    let current = CursorFrame::default();
    let mut previous = CursorFrame::default();
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    plane.update(target(1), false, Scene::default(), &mut drm);
    let original = plane.active;
    previous.queued(true);
    previous.completed(true);
    drm.calls.clear();
    assert!(!plane.update(target(1), false, scene(&[&current, &previous], 0), &mut drm,));
    assert_eq!(plane.active, original);
    assert!(drm.calls.is_empty());
    previous.queued(false);
    previous.completed(true);
    assert!(!plane.update(target(1), false, scene(&[&current, &previous], 0), &mut drm,));
    assert_eq!(plane.active, original);
}

#[test]
fn explicit_hide_and_lifecycle_reset_do_not_resurrect_pending_hardware() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let mut frame = CursorFrame::default();
    plane.update(target(1), false, Scene::default(), &mut drm);
    frame.queued(true);
    assert!(plane.clear(&mut drm));
    plane.allow_retry();
    frame.completed(true);
    // Completion only records pixels; it never enables a cursor by itself.
    assert!(plane.active.is_none());
    plane.reset();
    frame = CursorFrame::default();
    assert!(!frame.displayed && !frame.may_arrive());
    assert!(plane.active.is_none());
    assert!(!plane.failed(target(1)));
}
