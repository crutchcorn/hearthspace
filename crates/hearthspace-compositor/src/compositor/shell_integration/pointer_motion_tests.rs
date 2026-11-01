//! Real control dispatch and pointer policy without services, a renderer or KMS.
use hearthspace_ipc::command::{CanvasTool, ShellCommand};
use smithay::{backend::input::InputTime, reexports::calloop::EventLoop};
use wayland_server::Display;

use super::ControlReply;
use crate::{
    compositor::{
        App, CalloopData, cursor::CursorIcon, frame_callbacks::test_support::app::new_app,
    },
    config::VIEWPORT_ANIMATION_DURATION,
    geometry::CanvasPoint,
};

struct Fixture {
    app: App,
    _display: Display<App>,
    _events: EventLoop<'static, CalloopData>,
}

impl Fixture {
    fn new(independent_cursor: bool, software_cursor_visible: bool) -> Self {
        let display = Display::new().unwrap();
        let events = EventLoop::try_new().unwrap();
        let mut app = new_app(&display.handle(), events.handle());
        app.independent_cursor = independent_cursor;
        app.software_cursor_visible = software_cursor_visible;
        Self {
            app,
            _display: display,
            _events: events,
        }
    }

    fn action(&mut self, action: ShellCommand) {
        assert!(matches!(
            self.app.run_control_action(action),
            ControlReply::Ok
        ));
    }

    fn assert_pointer(&self, x: f64, y: f64) {
        assert_eq!(self.app.pointer_location, (x, y).into());
        assert_eq!(self.app.raw_pointer_location, (x, y).into());
        assert_eq!(self.app.pointer.current_location(), (x, y).into());
    }
}

#[test]
fn native_absolute_motion_updates_pointer_without_inventing_scene_damage() {
    let mut control = Fixture::new(true, false);
    let mut physical = Fixture::new(true, false);
    physical
        .app
        .apply_pointer_motion((234.5, 321.25).into(), InputTime::now());

    control.action(ShellCommand::PointerMotionAbs {
        x: 234.5,
        y: 321.25,
    });

    physical.assert_pointer(234.5, 321.25);
    control.assert_pointer(234.5, 321.25);
    assert!(!physical.app.needs_redraw);
    assert!(!control.app.needs_redraw);
    assert_eq!(control.app.cursor_icon, physical.app.cursor_icon);
    assert_eq!(control.app.cursor_visible, physical.app.cursor_visible);
}

#[test]
fn native_relative_motion_updates_pointer_without_inventing_scene_damage() {
    let mut fixture = Fixture::new(true, false);
    fixture
        .app
        .apply_pointer_motion((200., 200.).into(), InputTime::now());
    assert!(!fixture.app.needs_redraw);

    fixture.action(ShellCommand::PointerMotionRel {
        dx: 34.5,
        dy: -21.25,
    });

    fixture.assert_pointer(234.5, 178.75);
    assert!(!fixture.app.needs_redraw);
}

#[test]
fn native_motion_preserves_pending_scene_redraw() {
    let mut fixture = Fixture::new(true, false);
    fixture.app.request_redraw();

    fixture.action(ShellCommand::PointerMotionAbs { x: 200., y: 200. });
    assert!(fixture.app.needs_redraw);
    fixture.action(ShellCommand::PointerMotionRel { dx: 10., dy: 20. });
    fixture.assert_pointer(210., 220.);
    assert!(fixture.app.needs_redraw);
}

#[test]
fn native_marquee_motion_still_requests_a_scene_and_release_ends_it() {
    let mut fixture = Fixture::new(true, false);
    fixture.action(ShellCommand::PointerMotionAbs { x: 200., y: 200. });
    fixture.action(ShellCommand::PointerButtonDown(0x110));
    assert!(fixture.app.desktop_gesture_active());
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionAbs { x: 300., y: 250. });
    assert!(fixture.app.desktop_gesture_active());
    assert_eq!(fixture.app.pointer_location, (300., 250.).into());
    assert!(fixture.app.needs_redraw);
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionRel { dx: 20., dy: 10. });
    assert_eq!(fixture.app.pointer_location, (320., 260.).into());
    assert!(fixture.app.needs_redraw);
    fixture.action(ShellCommand::PointerButtonUp(0x110));
    assert!(!fixture.app.desktop_gesture_active());
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionRel { dx: 10., dy: 10. });
    fixture.assert_pointer(330., 270.);
    assert!(!fixture.app.needs_redraw);
}

#[test]
fn native_canvas_pan_updates_scene_and_cursor_through_the_shared_grab() {
    let mut fixture = Fixture::new(true, false);
    fixture.action(ShellCommand::CanvasTool(CanvasTool::Pan));
    fixture.app.needs_redraw = false;
    fixture.action(ShellCommand::PointerMotionAbs { x: 200., y: 200. });
    assert_eq!(fixture.app.cursor_icon, CursorIcon::Grab);
    assert!(!fixture.app.needs_redraw);
    fixture.action(ShellCommand::PointerButtonDown(0x110));
    assert!(fixture.app.canvas_pan_captured());
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionRel { dx: 40., dy: 25. });
    fixture.assert_pointer(240., 225.);
    assert_eq!(fixture.app.viewport_offset, CanvasPoint { x: -40, y: -25 });
    assert_eq!(fixture.app.cursor_icon, CursorIcon::Grabbing);
    assert!(fixture.app.needs_redraw);

    fixture.action(ShellCommand::PointerButtonUp(0x110));
    assert!(!fixture.app.canvas_pan_captured());
    fixture.app.needs_redraw = false;
    fixture.action(ShellCommand::PointerMotionRel { dx: 10., dy: 10. });
    assert_eq!(fixture.app.cursor_icon, CursorIcon::Grab);
    assert_eq!(fixture.app.viewport_offset, CanvasPoint { x: -40, y: -25 });
    assert!(!fixture.app.needs_redraw);
}

#[test]
fn native_relative_motion_samples_an_active_viewport_animation() {
    let mut fixture = Fixture::new(true, false);
    fixture
        .app
        .start_viewport_animation(CanvasPoint { x: 120, y: -80 }, 1.);
    fixture.app.viewport_animation.as_mut().unwrap().started_at =
        std::time::Instant::now() - VIEWPORT_ANIMATION_DURATION / 2;
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionRel { dx: 200., dy: 200. });

    fixture.assert_pointer(200., 200.);
    // A paused test process may reach the endpoint; both samples change the view.
    assert_ne!(fixture.app.viewport_offset, CanvasPoint { x: 0, y: 0 });
    assert!(fixture.app.needs_redraw);
}

#[test]
fn native_motion_samples_animation_and_redraws_its_final_scene() {
    let mut fixture = Fixture::new(true, false);
    let target = CanvasPoint { x: 120, y: -80 };
    fixture.app.start_viewport_animation(target, 1.);
    // Complete on dispatch without sleeping, including the final-frame case
    // where no animation remains to request the next scene.
    fixture.app.viewport_animation.as_mut().unwrap().started_at =
        std::time::Instant::now() - VIEWPORT_ANIMATION_DURATION;
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionAbs { x: 200., y: 200. });

    fixture.assert_pointer(200., 200.);
    assert_eq!(fixture.app.viewport_offset, target);
    assert!(fixture.app.viewport_animation.is_none());
    assert!(fixture.app.needs_redraw);
}

#[test]
fn nested_client_and_headless_software_cursor_motion_still_requests_a_scene() {
    // Both backends composite cursor pixels into their application scene.
    let mut fixture = Fixture::new(false, true);
    fixture.action(ShellCommand::PointerMotionAbs { x: 200., y: 200. });
    fixture.assert_pointer(200., 200.);
    assert!(fixture.app.needs_redraw);
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionRel { dx: 10., dy: 20. });
    fixture.assert_pointer(210., 220.);
    assert!(fixture.app.needs_redraw);
}

#[test]
fn nested_host_cursor_keeps_existing_control_motion_redraw_behavior() {
    let mut fixture = Fixture::new(false, false);
    fixture.action(ShellCommand::PointerMotionAbs { x: 200., y: 200. });
    fixture.assert_pointer(200., 200.);
    assert!(fixture.app.needs_redraw);
    fixture.app.needs_redraw = false;

    fixture.action(ShellCommand::PointerMotionRel { dx: 10., dy: 20. });
    fixture.assert_pointer(210., 220.);
    assert!(fixture.app.needs_redraw);
}

#[test]
fn native_non_motion_control_actions_keep_their_redraw_behavior() {
    let mut fixture = Fixture::new(true, false);
    fixture.action(ShellCommand::Axis {
        horizontal: 0.,
        vertical: 0.,
    });
    assert!(fixture.app.needs_redraw);
}
