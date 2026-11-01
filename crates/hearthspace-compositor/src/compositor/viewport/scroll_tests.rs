//! Real input policy and animation sampling, without a renderer or seat session.
use super::*;
use crate::{
    compositor::{
        CalloopData, OutputDescriptor, frame_callbacks::test_support::app::new_app,
        shell_integration::ControlReply,
    },
    geometry::{canvas_to_screen, screen_to_canvas},
};
use hearthspace_ipc::command::ShellCommand;
use smithay::{
    output::{PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
};
use wayland_server::Display;

struct Fixture {
    app: App,
    _display: Display<App>,
    _events: EventLoop<'static, CalloopData>,
}

impl Fixture {
    fn new() -> Self {
        let display = Display::new().unwrap();
        let events = EventLoop::try_new().unwrap();
        let mut app = new_app(&display.handle(), events.handle());
        let outputs = [("A", (800, 600), 1), ("B", (1600, 1200), 2)].map(|(name, size, scale)| {
            OutputDescriptor {
                name: name.into(),
                properties: PhysicalProperties {
                    size: (300, 200).into(),
                    subpixel: Subpixel::Unknown,
                    make: "Scroll test".into(),
                    model: name.into(),
                    serial_number: name.into(),
                },
                size: size.into(),
                scale,
                refresh: 60_000,
            }
        });
        app.sync_connector_outputs(&display.handle(), outputs.into());
        app.viewport_offset = CanvasPoint { x: -321, y: 467 };
        app.viewport_scale = 0.75;
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

    fn pointer(&mut self, x: f64, y: f64) {
        self.action(ShellCommand::PointerMotionAbs { x, y });
        assert_eq!(self.app.pointer_location, (x, y).into());
    }

    fn wheel(&mut self, vertical: f64) {
        self.action(ShellCommand::Axis {
            horizontal: 0.,
            vertical,
        });
    }
}

fn assert_anchored(animation: &ViewportAnimation, anchor: Point<f64, Logical>) {
    let canvas = screen_to_canvas(anchor, animation.from_offset, animation.from_scale);
    for step in 0..=20 {
        let now = animation.started_at + VIEWPORT_ANIMATION_DURATION * step / 20;
        let (offset, scale, _) = animation.sample(now);
        let projected = canvas_to_screen(canvas, offset, scale);
        // Persisted canvas offsets are integral: each axis rounds once per sample.
        assert!(
            (projected.x - anchor.x).abs() <= scale * 0.5 + 1e-8,
            "anchor x drift at sample {step}: {projected:?} vs {anchor:?}"
        );
        assert!(
            (projected.y - anchor.y).abs() <= scale * 0.5 + 1e-8,
            "anchor y drift at sample {step}: {projected:?} vs {anchor:?}"
        );
    }
    let (offset, scale, finished) =
        animation.sample(animation.started_at + VIEWPORT_ANIMATION_DURATION);
    assert_eq!(
        (offset, scale, finished),
        (animation.to_offset, animation.to_scale, true)
    );
}

#[test]
fn super_wheel_anchors_every_animation_sample_on_a_secondary_hidpi_output() {
    let mut f = Fixture::new();
    let output = f.app.output_logical_rect_for_name(Some("B")).unwrap();
    let pointer = (
        f64::from(output.loc.x) + 123.25,
        f64::from(output.loc.y) + 217.5,
    );
    f.pointer(pointer.0, pointer.1);
    f.action(ShellCommand::KeyDown(125));
    assert!(f.app.keyboard.modifier_state().logo);
    f.wheel(-120.);
    let animation = f.app.viewport_animation.as_ref().unwrap();
    assert!(animation.to_scale > animation.from_scale);
    assert_anchored(animation, pointer.into());
    // Control actions sample wall-clock time, so release only after the fixed
    // animation samples have been checked, even if the test host pauses.
    f.action(ShellCommand::KeyUp(125));
    assert!(!f.app.keyboard.modifier_state().logo);
}

#[test]
fn scroll_retarget_uses_the_current_mouse_point_and_displayed_view() {
    let mut f = Fixture::new();
    f.pointer(147.25, 281.75);
    f.action(ShellCommand::KeyDown(125));
    f.wheel(-120.);
    let initial = f.app.viewport_animation.as_ref().unwrap();
    let old_target = initial.to_scale;
    let retarget_at = initial.started_at + VIEWPORT_ANIMATION_DURATION / 3;
    let (shown_offset, shown_scale, _) = initial.sample(retarget_at);
    // Input admission is exercised above. Sample the animation at an explicit
    // instant here so a loaded test host cannot complete it before retargeting.
    f.app.pointer_location = (654.5, 123.25).into();
    f.app.animate_zoom_around_pointer_at(0.8, retarget_at);
    let next = f.app.viewport_animation.as_ref().unwrap();
    assert_eq!(
        (next.from_offset, next.from_scale),
        (shown_offset, shown_scale)
    );
    assert!(next.from_scale > 0.75 && next.from_scale < old_target);
    assert!(next.to_scale < next.from_scale);
    assert_anchored(next, (654.5, 123.25).into());
    f.action(ShellCommand::KeyUp(125));
}

#[test]
fn wheel_zoom_limits_preserve_the_mouse_anchor_and_do_not_pan_at_the_limit() {
    let mut f = Fixture::new();
    f.pointer(197.25, 453.5);
    f.action(ShellCommand::KeyDown(125));
    for (amount, target) in [
        (-10000., crate::config::MAX_ZOOM),
        (10000., crate::config::MIN_ZOOM),
    ] {
        f.wheel(amount);
        let animation = f.app.viewport_animation.as_ref().unwrap();
        assert_eq!(animation.to_scale, target);
        assert_anchored(animation, (197.25, 453.5).into());
        let final_offset = animation.to_offset;
        f.app.viewport_animation = None;
        f.app.viewport_offset = final_offset;
        f.app.viewport_scale = target;
        f.wheel(amount);
        assert!(f.app.viewport_animation.is_none());
        assert_eq!(f.app.viewport_offset, final_offset);
    }
    f.action(ShellCommand::KeyUp(125));
}

#[test]
fn ordinary_and_ctrl_wheel_remain_client_input_and_scroll_zoom_setting_uses_mouse() {
    let mut f = Fixture::new();
    f.pointer(123.5, 234.75);
    f.wheel(-120.);
    assert!(f.app.viewport_animation.is_none());
    f.action(ShellCommand::KeyDown(29));
    assert!(f.app.keyboard.modifier_state().ctrl);
    f.wheel(-120.);
    assert!(f.app.viewport_animation.is_none());
    f.action(ShellCommand::KeyUp(29));
    f.app.scroll_zooms_without_super = true;
    f.wheel(-120.);
    assert_anchored(
        f.app.viewport_animation.as_ref().unwrap(),
        (123.5, 234.75).into(),
    );
}
