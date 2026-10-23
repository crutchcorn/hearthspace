use super::*;

#[test]
fn settled_coordinates_are_revealed_by_retiring_the_offset_and_final_wake() {
    let mut state = State::default();
    let now = Instant::now();
    let delta = WorldPoint { x: -180.0, y: 90.0 };
    state.queue(7, delta, false, now);
    assert_eq!(state.offset(7), delta);

    assert!(state.advance(now + DURATION / 2, false, |id| id == 7));
    let halfway = state.offset(7);
    assert!(halfway.x > -90.0 && halfway.x < 0.0);
    assert!(halfway.y > 0.0 && halfway.y < 45.0);
    assert!(state.needs_animation_frame());

    // Merely passing the deadline cannot remove the timer or change the pose
    // used by the last frame. The final advance must request a redraw.
    let overdue = now + DURATION + Duration::from_millis(40);
    assert_eq!(state.offset(7), halfway);
    assert!(state.needs_animation_frame());
    assert!(state.advance(overdue, false, |id| id == 7));
    assert_eq!(state.offset(7), WorldPoint::default());
    assert!(!state.needs_animation_frame());
    assert!(!state.advance(overdue, false, |_| true));
}

#[test]
fn interrupted_motion_keeps_displayed_position_after_canonical_position_changes() {
    let mut state = State::default();
    let now = Instant::now();
    let canonical = WorldPoint { x: 260.0, y: 120.0 };
    state.queue(7, WorldPoint { x: -160.0, y: 80.0 }, false, now);
    state.advance(now + Duration::from_millis(100), false, |_| true);
    let visible = WorldPoint {
        x: canonical.x + state.offset(7).x,
        y: canonical.y + state.offset(7).y,
    };

    let next = WorldPoint { x: 180.0, y: 90.0 };
    let interrupted = now + Duration::from_millis(125);
    state.queue(
        7,
        WorldPoint {
            x: canonical.x - next.x,
            y: canonical.y - next.y,
        },
        false,
        interrupted,
    );
    assert!((next.x + state.offset(7).x - visible.x).abs() < 1e-10);
    assert!((next.y + state.offset(7).y - visible.y).abs() < 1e-10);
    // The new animation's initial sample preserves that same display position.
    state.advance(interrupted, false, |_| true);
    assert!((next.x + state.offset(7).x - visible.x).abs() < 1e-10);
    assert!((next.y + state.offset(7).y - visible.y).abs() < 1e-10);
    state.advance(interrupted + DURATION, false, |_| true);
    assert_eq!(state.offset(7), WorldPoint::default());
}

#[test]
fn all_frame_reads_share_a_world_sample_but_use_the_current_viewport_scale() {
    let mut state = State::default();
    let now = Instant::now();
    state.queue(7, WorldPoint { x: 120.0, y: -60.0 }, false, now);
    state.advance(now + Duration::from_millis(80), false, |_| true);
    let world = state.offset(7);
    let normal = state.screen_offset(7, 1.0);
    let zoomed = state.screen_offset(7, 2.5);
    assert_eq!(normal, (world.x, world.y).into());
    assert_eq!(zoomed, (world.x * 2.5, world.y * 2.5).into());
    assert_eq!(state.screen_offset(7, 1.0), normal);
    assert_eq!(state.offset(7), world);

    // The pan belongs to the settled position; the displacement is scaled once.
    let canonical = WorldPoint { x: 400.0, y: 220.0 };
    let pan = Point::<f64, Logical>::from((-90.0, 45.0));
    let origin = Point::from((canonical.x * 2.5, canonical.y * 2.5)) + pan + zoomed;
    assert!((origin.x - ((canonical.x + world.x) * 2.5 + pan.x)).abs() < 1e-10);
    assert!((origin.y - ((canonical.y + world.y) * 2.5 + pan.y)).abs() < 1e-10);
}

#[test]
fn no_change_does_not_restart_motion_and_an_immediate_reverse_can_cancel_it() {
    let mut state = State::default();
    let now = Instant::now();
    let delta = WorldPoint { x: -180.0, y: 90.0 };
    state.queue(7, delta, false, now);
    state.queue(7, WorldPoint::default(), false, now + DURATION / 2);
    state.advance(now + DURATION, false, |_| true);
    assert!(!state.needs_animation_frame());

    state.queue(7, delta, false, now + DURATION);
    state.queue(
        7,
        WorldPoint {
            x: -delta.x,
            y: -delta.y,
        },
        false,
        now + DURATION,
    );
    assert_eq!(state.offset(7), WorldPoint::default());
    assert!(!state.needs_animation_frame());
}

#[test]
fn unmapped_or_newly_attached_windows_retire_without_affecting_live_motion() {
    let mut state = State::default();
    let now = Instant::now();
    let delta = WorldPoint { x: 100.0, y: 0.0 };
    state.queue(7, delta, false, now);
    state.queue(8, delta, false, now);
    assert!(state.advance(now + DURATION / 2, false, |id| id == 8));
    assert_eq!(state.offset(7), WorldPoint::default());
    assert!(state.offset(8).x > 0.0);
    assert!(state.needs_animation_frame());
    assert!(state.advance(now + DURATION / 2, false, |_| false));
    assert!(!state.needs_animation_frame());
}

#[test]
fn reduced_motion_and_preview_takeover_clear_offsets_with_a_final_redraw() {
    let mut state = State::default();
    let now = Instant::now();
    let delta = WorldPoint { x: 100.0, y: 0.0 };
    state.queue(7, delta, true, now);
    assert_eq!(state.offset(7), WorldPoint::default());
    assert!(!state.needs_animation_frame());

    state.queue(7, delta, false, now);
    assert!(state.advance(now + DURATION / 2, true, |_| true));
    assert_eq!(state.offset(7), WorldPoint::default());
    assert!(!state.needs_animation_frame());

    state.queue(7, delta, false, now + DURATION);
    state.queue(7, delta, true, now + DURATION);
    assert_eq!(state.offset(7), WorldPoint::default());
    assert!(!state.needs_animation_frame());
}
