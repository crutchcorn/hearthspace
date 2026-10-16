use super::*;

fn arm(generation: u64) -> Arm {
    Arm {
        generation,
        window: 4,
        output: Some("test-output".into()),
        bounds: [20.0, 30.0, 300.0, 200.0],
        geometry: [100.0, 50.0, 2.0, 400.0, 300.0],
    }
}

fn ready() -> FavoriteDrawing {
    let mut state = FavoriteDrawing::default();
    state.arm(arm(9)).unwrap();
    state
}

fn complete(state: &mut FavoriteDrawing, now: Instant) -> u64 {
    assert!(state.begin(Source::Pointer, [10.0, 10.0], now));
    state.update(&Source::Pointer, [40.0, 40.0], now);
    state.end(&Source::Pointer, false, now);
    state.events().last().unwrap().stroke
}

#[test]
fn drawing_coordinates_respect_client_scale_and_reject_all_outside_edges() {
    let area = arm(9);
    assert_eq!(area.local([160.0, 130.0]), Some([10.0, 10.0]));
    assert_eq!(area.local([140.0, 110.0]), Some([0.0, 0.0]));
    for point in [
        [139.9, 110.0],
        [140.0, 109.9],
        [740.0, 110.0],
        [140.0, 510.0],
        [f64::NAN, 100.0],
        [100.0, f64::INFINITY],
    ] {
        assert_eq!(area.local(point), None, "{point:?}");
    }
}

#[test]
fn only_armed_drawing_starts_and_each_source_owns_its_own_updates() {
    let now = Instant::now();
    let mut state = FavoriteDrawing::default();
    assert!(!state.begin(Source::Pointer, [10.0, 10.0], now));
    state.arm(arm(9)).unwrap();
    assert!(!state.begin(Source::Pointer, [-1.0, 10.0], now));
    assert!(state.begin(Source::Pointer, [10.0, 10.0], now));
    state.update(
        &Source::Touch("screen".into(), Some(0).into()),
        [20.0, 20.0],
        now,
    );
    state.end(&Source::Touch("screen".into(), Some(0).into()), false, now);
    state.update(&Source::Pointer, [20.0, 20.0], now);
    state.end(&Source::Pointer, false, now);
    let events = state.events();
    assert_eq!(
        events
            .iter()
            .map(|e| (e.phase, e.sequence))
            .collect::<Vec<_>>(),
        [("begin", 0), ("update", 1), ("end", 2)]
    );
    assert!(
        events
            .iter()
            .all(|event| event.generation == 9 && event.stroke == events[0].stroke)
    );
    assert_eq!((events[1].x, events[1].y), (20.0, 20.0));
    assert_eq!(
        serde_json::to_value(&events[0]).unwrap()["output"],
        "test-output"
    );
}

#[test]
fn completion_requires_matching_generation_and_stroke_once_before_expiry() {
    let now = Instant::now();
    let mut state = ready();
    let stroke = complete(&mut state, now);
    assert!(state.accept(8, stroke, now).is_err());
    assert!(state.accept(9, stroke + 1, now).is_err());
    assert!(state.accept(9, stroke, now).is_ok());
    assert!(state.accept(9, stroke, now).is_err());
    let next = complete(&mut state, now);
    assert_ne!(next, stroke);
    assert!(state.accept(9, next, now + ACCEPT_LIFETIME).is_err());
}

#[test]
fn disarm_rearm_focus_loss_and_second_contact_cannot_complete_a_stale_stroke() {
    let now = Instant::now();
    for interrupt in 0..4 {
        let mut state = ready();
        assert!(state.begin(Source::Pointer, [10.0, 10.0], now));
        let stroke = state.events()[0].stroke;
        match interrupt {
            0 => state.disarm(9).unwrap(),
            1 => state.arm(arm(10)).unwrap(),
            2 => state.clear(),
            _ => assert!(!state.begin(
                Source::Touch("screen".into(), Some(1).into()),
                [20.0, 20.0],
                now
            )),
        }
        state.end(&Source::Pointer, false, now);
        assert!(state.accept(9, stroke, now).is_err());
        assert_eq!(
            state
                .events()
                .iter()
                .map(|event| event.phase)
                .collect::<Vec<_>>(),
            ["cancel"]
        );
    }
}

#[test]
fn completed_strokes_are_revoked_by_close_or_new_activity() {
    let now = Instant::now();
    for interrupt in 0..3 {
        let mut state = ready();
        let stroke = complete(&mut state, now);
        match interrupt {
            0 => state.clear(),
            1 => state.arm(arm(10)).unwrap(),
            _ => {
                state.begin(Source::Pointer, [20.0, 20.0], now);
            }
        }
        assert!(state.accept(9, stroke, now).is_err());
    }
}

#[test]
fn repeated_bounds_are_idempotent_and_stale_disarm_cannot_close_new_mode() {
    let now = Instant::now();
    let mut state = ready();
    state.begin(Source::Pointer, [10.0, 10.0], now);
    state.arm(arm(9)).unwrap();
    assert!(state.owns(&Source::Pointer));
    state.arm(arm(10)).unwrap();
    assert!(state.disarm(9).is_err());
    assert!(state.arm(arm(9)).is_err());
    assert_eq!(state.arm.as_ref().unwrap().generation, 10);
    let mut moved = arm(10);
    moved.geometry[0] += 20.0;
    state.begin(Source::Pointer, [10.0, 10.0], now);
    state.arm(moved).unwrap();
    assert!(!state.owns(&Source::Pointer));
    state.disarm(10).unwrap();
    assert!(
        state.arm(arm(10)).is_err(),
        "late bounds cannot reopen closed mode"
    );
    assert!(state.arm(arm(9)).is_err());
    state.arm(arm(11)).unwrap();
}

#[test]
fn leaving_the_area_nonfinite_positions_and_time_limit_cancel_without_end() {
    let now = Instant::now();
    for (point, elapsed) in [
        ([300.0, 0.0], Duration::ZERO),
        ([20.0, f64::NAN], Duration::ZERO),
        ([20.0, 20.0], MAX_DURATION),
    ] {
        let mut state = ready();
        state.begin(Source::Pointer, [10.0, 10.0], now);
        state.update(&Source::Pointer, point, now + elapsed);
        state.end(&Source::Pointer, false, now + elapsed);
        let events = state.events();
        assert_eq!(events.last().unwrap().phase, "cancel");
        assert!(state.accept(9, events[0].stroke, now).is_err());
    }
    let mut state = ready();
    state.begin(Source::Pointer, [10.0, 10.0], now);
    state.end(&Source::Pointer, false, now + MAX_DURATION);
    assert_eq!(state.events().last().unwrap().phase, "cancel");
}

#[test]
fn samples_are_bounded_and_input_owners_survive_cancellation_until_release() {
    let now = Instant::now();
    let mut state = ready();
    state.pointer_held = true;
    state.contacts.insert(("screen".into(), Some(0).into()));
    state.begin(Source::Pointer, [10.0, 10.0], now);
    state.events();
    state.stroke.as_mut().unwrap().sequence = MAX_EVENTS;
    state.update(&Source::Pointer, [20.0, 20.0], now);
    let events = state.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events.last().unwrap().phase, "cancel");
    state.clear();
    assert!(state.pointer_held);
    assert_eq!(state.contacts.len(), 1);
}

#[test]
fn high_rate_pointer_updates_are_sampled_but_release_keeps_exact_endpoint() {
    let now = Instant::now();
    let mut state = ready();
    state.begin(Source::Pointer, [10.0, 10.0], now);
    for millisecond in 1..20 {
        state.update(
            &Source::Pointer,
            [10.0 + millisecond as f64, 20.0],
            now + Duration::from_millis(millisecond),
        );
    }
    state.end(&Source::Pointer, false, now + Duration::from_millis(20));
    let events = state.events();
    assert_eq!(
        events
            .iter()
            .map(|e| (e.phase, e.sequence, e.x))
            .collect::<Vec<_>>(),
        [
            ("begin", 0, 10.0),
            ("update", 1, 18.0),
            ("update", 2, 26.0),
            ("update", 3, 29.0),
            ("end", 4, 29.0)
        ]
    );
    assert!(
        state
            .accept(9, events[0].stroke, now + Duration::from_millis(20))
            .is_ok()
    );
}

#[test]
fn geometry_change_revokes_old_generation_and_notifies_even_when_idle() {
    let now = Instant::now();
    for active in [false, true] {
        let mut state = ready();
        if active {
            state.begin(Source::Pointer, [10.0, 10.0], now);
            state.events();
        }
        state.reconfigure();
        let events = state.events();
        let last = events.last().unwrap();
        assert_eq!(
            (last.phase, last.stroke, last.sequence),
            ("reconfigure", 0, 0)
        );
        assert_eq!(last.generation, 9);
        assert_eq!(events.len(), if active { 2 } else { 1 });
        assert!(state.arm(arm(9)).is_err());
        state.arm(arm(10)).unwrap();
        state.clear();
        assert!(state.events().is_empty(), "focusloss must not ask to rearm");
    }
}
