use super::widget::TouchScroll;
use masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButtonEvent,
        PointerEvent, PointerInfo, PointerState, PointerType, PointerUpdate, PropertiesMut,
        PropertiesRef, RegisterCtx, TextEvent, Widget, WindowEvent,
        pointer::{PointerButton, PointerId},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{AsUnit, LenReq, Length},
    testing::TestHarness,
    widgets::{Button, ButtonPress, Portal, SizedBox},
};

type Harness = TestHarness<TouchScroll<SizedBox>>;

fn button() -> Button {
    Button::new(
        SizedBox::empty()
            .width(240.0.px())
            .height(600.0.px())
            .prepare(),
    )
}

fn harness(child: impl Widget) -> Harness {
    let child = SizedBox::new(child.prepare()).height(600.0.px());
    let portal = Portal::new(child.prepare()).constrain_horizontal(true);
    TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        TouchScroll::new(portal.prepare(), true, false).prepare(),
        (300, 180),
    )
}

fn touch(kind: &str, x: f64, y: f64) -> PointerEvent {
    let pointer = PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    };
    let state = PointerState {
        position: (x, y).into(),
        pressure: 0.5,
        ..Default::default()
    };
    match kind {
        "down" => PointerEvent::Down(PointerButtonEvent {
            pointer,
            state,
            button: None,
        }),
        "move" => PointerEvent::Move(PointerUpdate {
            pointer,
            current: state,
            coalesced: vec![],
            predicted: vec![],
        }),
        "up" => PointerEvent::Up(PointerButtonEvent {
            pointer,
            state,
            button: None,
        }),
        "cancel" => PointerEvent::Cancel(pointer),
        _ => unreachable!(),
    }
}

fn position(h: &Harness) -> Point {
    h.root_widget().children()[0]
        .downcast::<Portal<SizedBox>>()
        .unwrap()
        .get_viewport_pos()
}

#[test]
fn touch_tap_and_small_jitter_activate_without_scrolling() {
    let mut h = harness(button());
    h.process_pointer_event(touch("down", 80.0, 100.0));
    h.process_pointer_event(touch("move", 83.0, 97.0));
    h.process_pointer_event(touch("up", 80.0, 98.0));
    assert_eq!(position(&h), Point::ORIGIN);
    assert!(h.pop_action::<ButtonPress>().is_some());
    assert!(h.pop_action::<ButtonPress>().is_none());
}

#[test]
fn drag_over_button_scrolls_and_does_not_activate_on_release() {
    let mut h = harness(button());
    h.process_pointer_event(touch("down", 80.0, 140.0));
    h.process_pointer_event(touch("move", 80.0, 100.0));
    assert_eq!(position(&h).y, 40.0);
    h.process_pointer_event(touch("up", 80.0, 90.0));
    assert_eq!(position(&h).y, 50.0);
    assert!(h.pop_action::<ButtonPress>().is_none());
    h.process_pointer_event(touch("down", 80.0, 100.0));
    h.process_pointer_event(touch("up", 80.0, 100.0));
    assert!(h.pop_action::<ButtonPress>().is_some());
}

#[test]
fn drag_release_outside_does_not_suppress_later_programmatic_button_action() {
    let mut h = harness(button());
    let button_id = h.root_widget().children()[0].children()[0].children()[0].id();
    h.process_pointer_event(touch("down", 80.0, 140.0));
    h.process_pointer_event(touch("move", 80.0, 100.0));
    h.process_pointer_event(touch("up", -100.0, 90.0));
    assert!(h.pop_action::<ButtonPress>().is_none());
    h.edit_widget_with_id(button_id, |mut button| {
        button
            .ctx
            .submit_action::<ButtonPress>(ButtonPress { button: None });
    });
    assert!(h.pop_action::<ButtonPress>().is_some());
}

#[test]
fn mouse_drag_keeps_existing_button_behavior() {
    let mut h = harness(button());
    h.mouse_move((80.0, 140.0));
    h.mouse_button_press(Some(PointerButton::Primary));
    h.mouse_move((80.0, 90.0));
    h.mouse_button_release(Some(PointerButton::Primary));
    assert_eq!(position(&h), Point::ORIGIN);
    assert!(h.pop_action::<ButtonPress>().is_some());
}

#[test]
fn cancel_and_window_focus_loss_end_touch_scrolling() {
    for cancel in [true, false] {
        let mut h = harness(button());
        h.process_pointer_event(touch("down", 80.0, 140.0));
        h.process_pointer_event(touch("move", 80.0, 100.0));
        if cancel {
            h.process_pointer_event(touch("cancel", 80.0, 100.0));
        } else {
            h.process_text_event(TextEvent::WindowFocusChange(false));
        }
        h.process_pointer_event(touch("move", 80.0, 50.0));
        h.process_pointer_event(touch("up", 80.0, 50.0));
        assert_eq!(position(&h).y, 40.0);
        assert!(h.pop_action::<ButtonPress>().is_none());
    }
}

// This deliberately isn't Masonry's Button: the shell search results use a
// custom button with the same role and ButtonPress action contract.
struct CustomControl {
    role: Role,
    handled: bool,
}

impl Widget for CustomControl {
    type Action = ButtonPress;
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(_) => ctx.capture_pointer(),
            PointerEvent::Up(_) if ctx.is_active() && ctx.is_hovered() => {
                ctx.submit_action::<ButtonPress>(ButtonPress { button: None })
            }
            _ => {}
        }
        if self.handled {
            ctx.set_handled();
        }
    }
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        _: Axis,
        request: LenReq,
        _: Option<Length>,
    ) -> Length {
        match request {
            LenReq::FitContent(size) => size,
            _ => 100.0.px(),
        }
    }
    fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        self.role
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

#[test]
fn custom_button_rows_scroll_without_activating() {
    let mut h = harness(CustomControl {
        role: Role::Button,
        handled: false,
    });
    h.process_pointer_event(touch("down", 80.0, 140.0));
    h.process_pointer_event(touch("move", 80.0, 90.0));
    h.process_pointer_event(touch("up", 80.0, 90.0));
    assert_eq!(position(&h).y, 50.0);
    assert!(h.pop_action::<ButtonPress>().is_none());
}

#[test]
fn drawing_and_captured_direct_controls_retain_their_touch_stream() {
    for (role, handled) in [
        (Role::Canvas, true),
        (Role::TextInput, false),
        (Role::Slider, false),
    ] {
        let mut h = harness(CustomControl { role, handled });
        h.process_pointer_event(touch("down", 80.0, 140.0));
        h.process_pointer_event(touch("move", 80.0, 90.0));
        h.process_pointer_event(touch("up", 80.0, 90.0));
        assert_eq!(position(&h), Point::ORIGIN);
        assert!(h.pop_action::<ButtonPress>().is_some());
    }
}

#[test]
fn touch_drag_uses_logical_distance_at_double_scale() {
    let mut h = harness(button());
    h.process_window_event(WindowEvent::Rescale(2.0));
    h.process_pointer_event(touch("down", 80.0, 140.0));
    h.process_pointer_event(touch("move", 80.0, 130.0));
    assert_eq!(position(&h), Point::ORIGIN);
    h.process_pointer_event(touch("move", 80.0, 100.0));
    assert_eq!(position(&h).y, 20.0);
    h.process_pointer_event(touch("up", 80.0, 100.0));
    assert!(h.pop_action::<ButtonPress>().is_none());
}

#[test]
fn background_drag_captures_pointer_and_clamps_at_content_edges() {
    let mut h = harness(SizedBox::empty());
    h.process_pointer_event(touch("down", 80.0, 140.0));
    h.process_pointer_event(touch("move", 80.0, -1000.0));
    h.process_pointer_event(touch("up", 80.0, -1000.0));
    assert_eq!(position(&h).y, 420.0);
    h.process_pointer_event(touch("down", 80.0, 60.0));
    h.process_pointer_event(touch("move", 80.0, 5000.0));
    h.process_pointer_event(touch("up", 80.0, 5000.0));
    assert_eq!(position(&h), Point::ORIGIN);
}
