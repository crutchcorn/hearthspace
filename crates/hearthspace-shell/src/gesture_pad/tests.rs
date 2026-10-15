use super::{widget::*, *};
use masonry::{
    core::{
        PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType, PointerUpdate,
        TextEvent, Widget,
        keyboard::{Key, NamedKey},
        pointer::{PointerButton, PointerId},
    },
    kurbo::Point,
    testing::TestHarness,
};

fn harness() -> TestHarness<DrawingPad> {
    TestHarness::create_with_size(
        hearthspace_ui_components::hearthspace_light_properties(),
        DrawingPad::new(7, true, "Draw to launch".into(), vec![]).prepare(),
        (320, 200),
    )
}

fn actions(harness: &mut TestHarness<DrawingPad>) -> Vec<DrawingAction> {
    std::iter::from_fn(|| {
        harness
            .pop_action::<DrawingAction>()
            .map(|(action, _)| action)
    })
    .collect()
}

fn begin(harness: &mut TestHarness<DrawingPad>) {
    actions(harness);
    harness.mouse_move((30.0, 40.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
}

#[test]
fn pointer_stroke_paints_and_emits_only_once_on_clean_release() {
    let mut harness = harness();
    assert_eq!(
        actions(&mut harness),
        [DrawingAction::Bounds {
            context: 7,
            rect: [0.0, 0.0, 320.0, 200.0]
        }]
    );
    let blank = harness.render();
    begin(&mut harness);
    harness.mouse_move((140.0, 80.0));
    assert!(actions(&mut harness).is_empty());
    assert_ne!(blank, harness.render());
    let node = harness.access_node(harness.root_id()).unwrap();
    assert_eq!(node.label().as_deref(), Some("Gesture drawing area"));
    assert!(node.value().unwrap().contains("2 points"));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert_eq!(
        actions(&mut harness),
        [DrawingAction::Stroke {
            context: 7,
            points: vec![[30.0, 40.0], [140.0, 80.0]],
        }]
    );
    harness.mouse_move((90.0, 70.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(actions(&mut harness).is_empty());
}

fn touch(kind: &str, point: Point) -> PointerEvent {
    let pointer = PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    };
    let state = PointerState {
        position: (point.x, point.y).into(),
        pressure: 0.5,
        ..Default::default()
    };
    match kind {
        "down" => PointerEvent::Down(PointerButtonEvent {
            pointer,
            state,
            button: None,
        }),
        "up" => PointerEvent::Up(PointerButtonEvent {
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
        "cancel" => PointerEvent::Cancel(pointer),
        _ => unreachable!(),
    }
}

#[test]
fn touch_includes_release_position_and_cancel_discards_input() {
    let mut harness = harness();
    actions(&mut harness);
    harness.process_pointer_event(touch("down", (20.0, 25.0).into()));
    harness.process_pointer_event(touch("move", (75.0, 50.0).into()));
    harness.process_pointer_event(touch("up", (100.0, 80.0).into()));
    assert_eq!(
        actions(&mut harness),
        [DrawingAction::Stroke {
            context: 7,
            points: vec![[20.0, 25.0], [75.0, 50.0], [100.0, 80.0]],
        }]
    );
    harness.process_pointer_event(touch("down", (20.0, 25.0).into()));
    harness.process_pointer_event(touch("move", (75.0, 50.0).into()));
    harness.process_pointer_event(touch("cancel", (75.0, 50.0).into()));
    harness.process_pointer_event(touch("up", (100.0, 80.0).into()));
    assert!(actions(&mut harness).is_empty());
}

#[test]
fn coalesced_motion_is_recorded_but_predictions_are_never_matched() {
    let mut harness = harness();
    actions(&mut harness);
    harness.process_pointer_event(touch("down", (20.0, 25.0).into()));
    let mut movement = touch("move", (75.0, 50.0).into());
    if let PointerEvent::Move(event) = &mut movement {
        event.coalesced = [[40.0, 30.0], [60.0, 40.0]]
            .map(|point| PointerState {
                position: (point[0], point[1]).into(),
                ..Default::default()
            })
            .to_vec();
        event.predicted.push(PointerState {
            position: (500.0, 900.0).into(),
            ..Default::default()
        });
    }
    harness.process_pointer_event(movement);
    harness.process_pointer_event(touch("up", (100.0, 80.0).into()));
    assert_eq!(
        actions(&mut harness),
        [DrawingAction::Stroke {
            context: 7,
            points: vec![
                [20.0, 25.0],
                [40.0, 30.0],
                [60.0, 40.0],
                [75.0, 50.0],
                [100.0, 80.0]
            ],
        }]
    );
}

#[test]
fn leaving_area_or_another_button_never_completes_a_partial_stroke() {
    for interruption in ["leave", "secondary_down", "secondary_up", "extra_contact"] {
        let mut harness = harness();
        begin(&mut harness);
        harness.mouse_move((80.0, 90.0));
        match interruption {
            "leave" => harness.mouse_move((350.0, 90.0)),
            "secondary_down" => harness.mouse_button_press(Some(PointerButton::Secondary)),
            "secondary_up" => harness.mouse_button_release(Some(PointerButton::Secondary)),
            "extra_contact" => {
                harness.process_pointer_event(touch("down", (90.0, 90.0).into()));
            }
            _ => unreachable!(),
        }
        harness.mouse_move((100.0, 100.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(actions(&mut harness).is_empty(), "{interruption}");
    }
}

#[test]
fn escape_window_blur_and_focus_loss_cancel_without_completing() {
    for interruption in ["escape", "window_blur", "focus_loss", "disabled"] {
        let mut harness = harness();
        begin(&mut harness);
        harness.mouse_move((80.0, 90.0));
        match interruption {
            "escape" => {
                harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
            }
            "window_blur" => {
                harness.process_text_event(TextEvent::WindowFocusChange(false));
            }
            "focus_loss" => harness.focus_on(None),
            "disabled" => harness.edit_root_widget(|mut widget| widget.ctx.set_disabled(true)),
            _ => unreachable!(),
        }
        harness.mouse_move((100.0, 100.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(actions(&mut harness).is_empty(), "{interruption}");
    }
}

#[test]
fn outside_start_hover_and_secondary_start_never_draw() {
    let mut harness = harness();
    actions(&mut harness);
    harness.mouse_move((-5.0, 40.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move((90.0, 60.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.mouse_button_press(Some(PointerButton::Secondary));
    harness.mouse_move((110.0, 80.0));
    harness.mouse_button_release(Some(PointerButton::Secondary));
    assert!(actions(&mut harness).is_empty());
}

#[test]
fn point_limit_discards_entire_stroke_instead_of_matching_a_prefix() {
    let mut harness = harness();
    begin(&mut harness);
    for index in 0..2100 {
        harness.mouse_move((50.0 + (index % 2) as f64 * 90.0, 60.0));
    }
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(actions(&mut harness).is_empty());
}

#[test]
fn external_preview_paints_but_never_emits_or_mixes_a_local_stroke() {
    let mut harness = harness();
    let empty = harness.render();
    harness.edit_root_widget(|mut widget| {
        widget.widget.capture_locally = false;
        widget
            .widget
            .set_preview(&[[40.0, 40.0], [180.0, 100.0], [f64::NAN, 0.0]]);
        widget.ctx.request_render();
    });
    assert_ne!(empty, harness.render());
    begin(&mut harness);
    harness.mouse_move((80.0, 90.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(actions(&mut harness).is_empty());
}

#[derive(Default)]
struct TestState {
    context: u64,
    status: String,
    bounds: Option<String>,
    stroke: Option<String>,
}

fn view(state: &TestState) -> GesturePad<TestState> {
    GesturePad {
        output_key: "eDP-1".into(),
        capture_locally: true,
        context: state.context,
        status: state.status.clone(),
        preview: vec![],
        on_stroke: |state, output, context, points| {
            state.stroke = Some(format!("{output} {context} {points:?}"));
        },
        on_bounds: |state, output, context, rect| {
            state.bounds = Some(format!("{output} {context} {rect:?}"));
        },
    }
}

#[test]
fn changing_mode_discards_queued_stroke_and_reannounces_unchanged_bounds() {
    let state = TestState::default();
    let mut ui = super::test_support::Ui::new(state, view, (320, 200));
    assert_eq!(
        ui.state.bounds.as_deref(),
        Some("eDP-1 0 [0.0, 0.0, 320.0, 200.0]")
    );
    begin(&mut ui.harness);
    ui.harness.mouse_move((90.0, 80.0));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.state.context = 1;
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(ui.state.stroke.is_none());
    assert_eq!(
        ui.state.bounds.as_deref(),
        Some("eDP-1 1 [0.0, 0.0, 320.0, 200.0]")
    );

    begin(&mut ui.harness);
    ui.harness.mouse_move((120.0, 80.0));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert_eq!(
        ui.state.stroke.as_deref(),
        Some("eDP-1 1 [[30.0, 40.0], [120.0, 80.0]]")
    );
}

#[test]
fn changing_mode_during_a_drag_cancels_it() {
    let state = TestState::default();
    let mut ui = super::test_support::Ui::new(state, view, (320, 200));
    begin(&mut ui.harness);
    ui.harness.mouse_move((90.0, 80.0));
    ui.state.context = 1;
    ui.rebuild(&view);
    ui.harness.mouse_move((110.0, 80.0));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert!(ui.state.stroke.is_none());
}

#[test]
fn status_only_rebuild_preserves_the_stroke() {
    let state = TestState::default();
    let mut ui = super::test_support::Ui::new(state, view, (320, 200));
    begin(&mut ui.harness);
    ui.harness.mouse_move((90.0, 80.0));
    ui.state.status = "Still drawing".into();
    ui.rebuild(&view);
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert_eq!(
        ui.state.stroke.as_deref(),
        Some("eDP-1 0 [[30.0, 40.0], [90.0, 80.0]]")
    );
}

#[test]
fn reported_bounds_follow_client_space_translation() {
    let mut harness = harness();
    actions(&mut harness);
    harness.edit_root_widget(|mut widget| {
        widget.set_transform(masonry::kurbo::Affine::translate((15.0, 25.0)));
        widget.ctx.request_compose();
    });
    harness.redraw();
    assert_eq!(
        actions(&mut harness),
        [DrawingAction::Bounds {
            context: 7,
            rect: [15.0, 25.0, 320.0, 200.0],
        }]
    );
}
