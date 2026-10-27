use super::*;

fn assert_center(harness: &TestHarness<Minimap>, projection: &Projection, point: Point) {
    let actual = harness.root_widget().viewport(projection).unwrap().center();
    // Compositor offsets round to whole canvas units.
    assert!((actual.x - point.x).abs() <= projection.scale * 0.51);
    assert!((actual.y - point.y).abs() <= projection.scale * 0.51);
}

#[test]
fn outside_mouse_press_centers_selected_output_and_drags_beyond_map_bounds() {
    let mut harness = harness(2.0);
    let before = harness.root_widget().snapshot.clone().unwrap();
    let projection = Projection::new(Some(&before), harness.root_widget().size);
    let start = Point::new(270.0, 140.0);
    assert!(
        !harness
            .root_widget()
            .viewport(&projection)
            .unwrap()
            .contains(start)
    );
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::Begin, MapAction::PanTo { .. }]
    ));
    assert_center(&harness, &projection, start);

    let outside = Point::new(365.0, -30.0);
    harness.mouse_move(outside);
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::PanTo { .. }]
    ));
    assert_center(&harness, &projection, outside);
    assert_eq!(
        harness
            .root_widget()
            .drag
            .as_ref()
            .unwrap()
            .projection
            .world,
        projection.world
    );
    let after = harness.root_widget().snapshot.clone().unwrap();
    assert_eq!(after.windows, before.windows);
    assert_eq!(after.outputs, before.outputs);
    assert_eq!(after.scale(), before.scale());
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    assert!(harness.root_widget().drag.is_none());
}

#[test]
fn outside_touch_centers_immediately_and_applies_final_release_position() {
    let mut harness = harness(0.5);
    let projection = Projection::new(
        harness.root_widget().snapshot.as_ref(),
        harness.root_widget().size,
    );
    let start = Point::new(275.0, 135.0);
    harness.process_pointer_event(touch_event("down", start));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::Begin, MapAction::PanTo { .. }]
    ));
    assert_center(&harness, &projection, start);
    harness.process_pointer_event(touch_event("move", Point::new(295.0, 145.0)));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::PanTo { .. }]
    ));
    let released = Point::new(345.0, 200.0);
    harness.process_pointer_event(touch_event("up", released));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::PanTo { .. }, MapAction::End]
    ));
    assert_center(&harness, &projection, released);
    assert!(harness.root_widget().drag.is_none());
}

fn second_contact(kind: &str, point: Point) -> PointerEvent {
    let mut event = touch_event(kind, point);
    let pointer = match &mut event {
        PointerEvent::Down(event) | PointerEvent::Up(event) => &mut event.pointer,
        PointerEvent::Move(event) => &mut event.pointer,
        PointerEvent::Cancel(pointer) => pointer,
        _ => unreachable!(),
    };
    pointer.pointer_id = PointerId::new(2);
    event
}

#[test]
fn additional_contacts_never_recenter_and_their_release_cannot_strand_capture() {
    let mut harness = harness(1.0);
    let start = center(&harness);
    harness.process_pointer_event(touch_event("down", start));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    let offset = harness.root_widget().snapshot.as_ref().unwrap().offset;
    harness.process_pointer_event(second_contact("down", Point::new(290.0, 130.0)));
    harness.process_pointer_event(second_contact("move", Point::new(300.0, 150.0)));
    assert!(drain(&mut harness).is_empty());
    assert_eq!(
        harness.root_widget().snapshot.as_ref().unwrap().offset,
        offset
    );
    assert!(harness.root_widget().drag.is_some());
    harness.process_pointer_event(second_contact("up", Point::new(300.0, 150.0)));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    harness.process_pointer_event(touch_event("move", start + (50.0, 10.0)));
    harness.process_pointer_event(touch_event("up", start + (80.0, 30.0)));
    assert!(drain(&mut harness).is_empty());
    assert_eq!(
        harness.root_widget().snapshot.as_ref().unwrap().offset,
        offset
    );
    assert!(harness.root_widget().drag.is_none());
}

#[test]
fn unrelated_cancel_safely_ends_capture_without_navigation() {
    let mut harness = harness(1.0);
    let start = center(&harness);
    harness.process_pointer_event(touch_event("down", start));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    let offset = harness.root_widget().snapshot.as_ref().unwrap().offset;
    harness.process_pointer_event(second_contact("cancel", start));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    harness.process_pointer_event(touch_event("move", start + (70.0, 40.0)));
    harness.process_pointer_event(touch_event("up", start + (70.0, 40.0)));
    assert!(drain(&mut harness).is_empty());
    assert_eq!(
        harness.root_widget().snapshot.as_ref().unwrap().offset,
        offset
    );
    assert!(harness.root_widget().drag.is_none());
}

#[test]
fn touch_focus_loss_clears_drag_and_next_press_uses_fresh_snapshot_projection() {
    let mut harness = harness(1.0);
    let start = center(&harness);
    harness.process_pointer_event(touch_event("down", start));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    let original = harness
        .root_widget()
        .drag
        .as_ref()
        .unwrap()
        .projection
        .world;
    harness.process_text_event(TextEvent::WindowFocusChange(false));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    harness.process_pointer_event(touch_event("up", Point::new(300.0, 150.0)));
    assert!(drain(&mut harness).is_empty());
    assert!(harness.root_widget().drag.is_none());

    harness.edit_root_widget(|mut root| {
        root.widget.snapshot.as_mut().unwrap().windows[0].bounds =
            Rect::new(-50_000.0, 30_000.0, -49_400.0, 30_400.0);
        root.ctx.request_render();
    });
    harness.process_text_event(TextEvent::WindowFocusChange(true));
    let fresh = Projection::new(
        harness.root_widget().snapshot.as_ref(),
        harness.root_widget().size,
    );
    assert_ne!(original, fresh.world);
    let destination = Point::new(260.0, 40.0);
    harness.process_pointer_event(touch_event("down", destination));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::Begin, MapAction::PanTo { .. }]
    ));
    assert_eq!(
        harness
            .root_widget()
            .drag
            .as_ref()
            .unwrap()
            .projection
            .world,
        fresh.world
    );
    assert_center(&harness, &fresh, destination);
}

#[test]
fn viewport_edge_press_keeps_grab_offset_and_missing_output_cannot_pan() {
    let mut harness = harness(1.0);
    let projection = Projection::new(
        harness.root_widget().snapshot.as_ref(),
        harness.root_widget().size,
    );
    let viewport = harness.root_widget().viewport(&projection).unwrap();
    let start = Point::new(viewport.x0, viewport.center().y);
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    harness.mouse_move(start + (20.0, 10.0));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::PanTo { .. }]
    ));
    let moved = harness.root_widget().viewport(&projection).unwrap();
    assert!((moved.x0 - start.x - 20.0).abs() <= projection.scale * 0.51);
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    harness.edit_root_widget(|mut root| {
        root.widget.output_name = Some("disconnected".into());
        root.ctx.request_render();
    });
    harness.mouse_move((280.0, 140.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(drain(&mut harness).is_empty());
    assert!(harness.root_widget().drag.is_none());
}
