use super::{test_support::held_preview, *};

#[test]
fn animation_reads_hold_the_presented_sample_until_the_next_advance() {
    let (mut state, now) = held_preview();
    let sampled = state.pose(7).unwrap();
    assert_eq!(sampled, state.poses[&7].current(state.sampled_at));
    assert_ne!(sampled, state.poses[&7].current(now));
    assert_eq!(state.pose(7).unwrap(), sampled);
    state.advance(now, true);
    assert_eq!(state.pose(7).unwrap().0, state.poses[&7].to);
}

#[rstest::rstest]
#[case::search(Owner::Search)]
#[case::switcher(Owner::Switcher)]
fn active_preview_keeps_a_timer_until_terminal_advance_requests_redraw(#[case] owner: Owner) {
    let (mut state, now) = held_preview();
    state.owner = Some(owner);
    state.sampled_at = state.poses[&7].started + DURATION - Duration::from_millis(1);
    let mut needs_redraw = false;
    assert!(!needs_redraw);
    assert!(state.needs_animation_frame());
    assert!(now.duration_since(state.poses[&7].started) > DURATION);
    assert_ne!(state.pose(7).unwrap().0, state.poses[&7].to);

    // Both the timer and vblank must stay eligible after the wall-clock
    // deadline, until advance marks this last frame for redraw.
    needs_redraw |= state.advance(now, true);
    assert!(needs_redraw);
    assert_eq!(state.pose(7).unwrap().0, state.poses[&7].to);
    assert!(state.owner == Some(owner));
    assert!(!state.needs_animation_frame());

    needs_redraw = false;
    needs_redraw |= state.advance(now + Duration::from_secs(1), true);
    assert!(!needs_redraw, "a settled modal must stop scheduling frames");
    assert_eq!(
        state.poses.len(),
        1,
        "active previews keep their final pose"
    );
}

#[test]
fn exiting_preview_redraws_when_its_last_pose_retires() {
    let (mut state, now) = held_preview();
    state.owner = Some(Owner::Search);
    assert!(state.needs_animation_frame());
    let mut needs_redraw = false;
    needs_redraw |= state.advance(now, false);
    assert!(needs_redraw);
    assert!(state.poses.is_empty());
    assert!(state.owner.is_none());
    assert!(!state.needs_animation_frame());
    assert!(!state.advance(now + Duration::from_secs(1), false));
}

#[test]
fn already_sampled_exit_still_redraws_when_retired() {
    let (mut state, now) = held_preview();
    state.advance(now, true);
    assert!(!state.needs_animation_frame());
    assert!(state.advance(now, false));
    assert!(state.poses.is_empty());
}

#[test]
fn retargeting_a_settled_preview_restores_frame_eligibility() {
    let (mut state, now) = held_preview();
    state.advance(now, true);
    assert!(!state.needs_animation_frame());
    let pose = state.poses.get_mut(&7).unwrap();
    pose.from = pose.to;
    pose.to.x += 100.;
    pose.started = now;
    assert!(state.needs_animation_frame());
    assert!(state.advance(now + DURATION, true));
    assert_eq!(state.pose(7).unwrap().0, state.poses[&7].to);
    assert!(!state.needs_animation_frame());
}
