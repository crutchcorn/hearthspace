use super::*;
use crate::compositor::{
    frame_callbacks::test_support::app::new_app,
    udev::{cursor::apply_visibility, device::cursor::policy::CursorPlane},
};

#[test]
fn failed_move_retains_old_position_until_fallback_and_does_not_retry_each_tick() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let mut frame = CursorFrame::default();
    let mut pointer = target(1);
    plane.update(pointer, false, Scene::default(), &mut drm);
    let original = plane.active;
    pointer.local.0 += 30.0;
    drm.move_error = true;
    assert!(plane.update(pointer, false, Scene::default(), &mut drm));
    assert_eq!(plane.active, original);
    drm.calls.clear();
    assert!(plane.update(pointer, false, Scene::default(), &mut drm));
    assert!(
        drm.calls.is_empty(),
        "failed request stays software until explicit recovery"
    );
    frame.queued(true);
    assert!(plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert_eq!(plane.active, original);
    frame.completed(true);
    assert!(plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_none());

    drm.move_error = false;
    plane.allow_retry(); // explicit new cursor request / lifecycle recovery
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert!(
        plane.active.is_none(),
        "software must retire before hardware recovers"
    );
    frame.queued(false);
    frame.completed(true);
    assert!(!plane.update(pointer, false, scene(&[&frame], 0), &mut drm));
    assert_eq!(plane.active.unwrap().position, Some((128, 77)));
}

#[test]
fn image_map_failure_preserves_previous_cursor_and_retries_a_new_image_request() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    let mut pointer = target(1);
    plane.update(pointer, false, Scene::default(), &mut drm);
    let original = plane.active;
    pointer.image.icon = CursorIcon::Grab;
    drm.image_error = true;
    drm.calls.clear();
    assert!(plane.update(pointer, false, Scene::default(), &mut drm));
    assert_eq!(drm.calls, [Op::Image(pointer.image)]);
    assert_eq!(plane.active, original);
    assert!(plane.failed(pointer));
    drm.calls.clear();
    assert!(plane.update(pointer, false, Scene::default(), &mut drm));
    assert!(drm.calls.is_empty());
    drm.image_error = false;
    pointer.image.icon = CursorIcon::Grabbing;
    assert!(!plane.update(pointer, false, Scene::default(), &mut drm));
    assert!(!plane.failed(pointer));
    assert_eq!(plane.active.unwrap().position, Some((88, 68)));
}

#[test]
fn failed_assignment_and_initial_move_never_cache_a_successful_position() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm {
        set_error: true,
        ..Default::default()
    };
    assert!(plane.update(target(1), false, Scene::default(), &mut drm));
    assert!(plane.active.is_none());
    assert!(drm.calls.iter().all(|call| !matches!(call, Op::Move(..))));

    plane.allow_retry();
    drm.set_error = false;
    drm.move_error = true;
    assert!(plane.update(target(1), false, Scene::default(), &mut drm));
    assert_eq!(plane.active.unwrap().position, None);
    assert!(plane.clear(&mut drm));
    assert!(plane.active.is_none());
}

#[test]
fn failed_clear_preserves_the_owner_until_it_can_be_cleared() {
    let mut plane = CursorPlane::default();
    let mut drm = Drm::default();
    plane.update(target(1), false, Scene::default(), &mut drm);
    let mut frame = CursorFrame::default();
    frame.queued(true);
    frame.completed(true);
    drm.clear_error = true;
    assert!(plane.update(target(1), true, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_some());
    drm.clear_error = false;
    plane.retry_clear(); // production timer permits the bounded next attempt
    assert!(plane.update(target(1), true, scene(&[&frame], 0), &mut drm));
    assert!(plane.active.is_none());
}

#[test]
fn actual_app_requests_redraw_for_fallback_recovery_and_superseded_pixels() {
    let display = wayland_server::Display::<crate::compositor::App>::new().unwrap();
    let event_loop = smithay::reexports::calloop::EventLoop::try_new().unwrap();
    let mut app = new_app(&display.handle(), event_loop.handle());
    let mut plane = CursorPlane::default();
    let mut drm = Drm {
        set_error: true,
        ..Default::default()
    };
    app.software_cursor_visible = false;
    app.needs_redraw = false;
    let software = plane.update(target(1), false, Scene::default(), &mut drm);
    apply_visibility(&mut app, software, Scene::default());
    assert!(app.software_cursor_visible && app.needs_redraw);

    let mut frame = CursorFrame::default();
    frame.queued(true);
    frame.completed(true);
    plane.allow_retry();
    drm.set_error = false;
    app.needs_redraw = false;
    let software = plane.update(target(1), false, scene(&[&frame], 0), &mut drm);
    apply_visibility(&mut app, software, scene(&[&frame], 0));
    assert!(!app.software_cursor_visible && app.needs_redraw);
    assert!(plane.active.is_none());

    // Even when desired software is already false, old submitted pixels need
    // removal when they arrive after supersession.
    app.needs_redraw = false;
    apply_visibility(&mut app, false, scene(&[&frame], 0));
    assert!(app.needs_redraw);
    frame.queued(false);
    frame.completed(true);
    app.needs_redraw = false;
    let software = plane.update(target(1), false, scene(&[&frame], 0), &mut drm);
    apply_visibility(&mut app, software, scene(&[&frame], 0));
    assert!(plane.active.is_some());
    assert!(
        !app.needs_redraw,
        "clean hardware motion requires no scene redraw"
    );
}
