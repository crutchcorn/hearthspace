use super::*;

fn pending_turn() -> Turn {
    Turn {
        target: Target {
            output: "test".into(),
            target: hearthspace_common::frame_layout::LayoutTarget::widget("tools"),
        },
        members: vec![1],
        bounds: Rect {
            x: 24.,
            y: 200.,
            width: 54.,
            height: 246.,
        },
        pivot: Point { x: -18., y: 323. },
        from_angle: -std::f64::consts::FRAC_PI_2,
        translation: Point { x: 480., y: -160. },
        pointer_anchor: false,
        started: None,
    }
}
#[test]
fn reflow_wait_does_not_consume_motion_time_and_translation_shares_the_turn_clock() {
    let queued = Instant::now();
    let mut turn = pending_turn();
    let committed = queued + Duration::from_millis(600);
    assert_eq!(turn.angle(committed), turn.from_angle);
    assert_eq!(turn.offset(committed), turn.translation);
    assert!(!turn.finished(committed));
    assert!(
        !turn.needs_animation_frame(),
        "waiting for native buffers must not render unchanged frames"
    );
    turn.started = Some(committed);
    for elapsed in [0, 16, 50, 100, 180, 280] {
        let now = committed + Duration::from_millis(elapsed);
        let fraction = turn.angle(now) / turn.from_angle;
        assert!((turn.offset(now).x - turn.translation.x * fraction).abs() < 1e-8);
        assert!((turn.offset(now).y - turn.translation.y * fraction).abs() < 1e-8);
        assert_eq!(turn.bounds.width, 54.);
        assert_eq!(turn.bounds.height, 246.);
        assert!(turn.needs_animation_frame());
    }
    assert!(turn.finished(committed + TURN_DURATION));
    assert_eq!(turn.angle(committed + TURN_DURATION), 0.);
    assert_eq!(
        turn.offset(committed + TURN_DURATION),
        Point { x: 0., y: 0. }
    );
}

#[test]
fn quarter_turn_preserves_the_pointer_grip_and_swaps_toolbar_extent() {
    let bounds = Rect {
        x: 100.,
        y: 100.,
        width: 54.,
        height: 200.,
    };
    let pivot = Point { x: 127., y: 127. };
    let rotated = rotated_bounds(bounds, pivot, -std::f64::consts::FRAC_PI_2);
    assert!((rotated.width - 200.).abs() < 0.0001);
    assert!((rotated.height - 54.).abs() < 0.0001);
    assert!((rotated.x - 100.).abs() < 0.0001);
    assert!((rotated.y - 100.).abs() < 0.0001);
    assert_eq!(rotated_bounds(bounds, pivot, 0.), bounds);
}

#[test]
fn pending_reflow_stays_idle_but_invalidating_it_redraws_the_exposed_surface() {
    let now = Instant::now();
    let mut state = State::default();
    state.turns.insert("tools".into(), pending_turn());
    let valid = vec!["tools".into()];
    assert!(!state.retire(now, false, &valid));
    assert!(!state.retire(now + Duration::from_secs(5), false, &valid));
    assert_eq!(state.turns.len(), 1);
    assert!(state.retire(now, false, &[]));
    assert!(state.turns.is_empty());
    assert!(!state.retire(now, false, &[]));
}

#[test]
fn started_turn_keeps_a_timer_until_final_advance_redraws_and_retires_it() {
    let now = Instant::now();
    let mut state = State::default();
    let mut turn = pending_turn();
    turn.started = Some(now);
    state.turns.insert("tools".into(), turn);
    let valid = vec!["tools".into()];
    assert!(state.retire(now + TURN_DURATION / 2, false, &valid));
    assert_eq!(state.turns.len(), 1);
    state.turns.get_mut("tools").unwrap().started =
        Some(Instant::now() - TURN_DURATION - Duration::from_millis(40));
    let overdue = Instant::now();
    assert!(state.turns["tools"].finished(overdue));
    assert!(state.turns["tools"].needs_animation_frame());
    assert!(state.retire(overdue, false, &valid));
    assert!(state.turns.is_empty());
    assert!(!state.retire(overdue, false, &valid));
}
