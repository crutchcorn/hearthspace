use super::*;

fn succeed(state: &mut State, generation: u64) -> Effect {
    state.authentication_finished(Completion {
        generation,
        result: Ok(()),
    })
}

#[test]
fn password_success_before_locked_waits_for_compositor_confirmation() {
    let mut state = State::default();
    let generation = state.begin_authentication().unwrap();
    assert_eq!(succeed(&mut state, generation), Effect::None);
    assert!(state.busy());
    assert!(!state.succeeded());
    assert_eq!(state.locked(), Effect::Unlock);
    state.ended();
    assert!(state.succeeded());
}

#[test]
fn locked_alone_never_unlocks() {
    let mut state = State::default();
    assert_eq!(state.locked(), Effect::None);
    assert_eq!(state.locked(), Effect::None);
    assert!(!state.succeeded());
}

#[test]
fn password_success_after_locked_requests_exactly_one_unlock() {
    let mut state = State::default();
    state.locked();
    let generation = state.begin_authentication().unwrap();
    assert_eq!(succeed(&mut state, generation), Effect::Unlock);
    assert_eq!(succeed(&mut state, generation), Effect::None);
    assert_eq!(state.locked(), Effect::None);
    assert!(state.begin_authentication().is_none());
    assert!(!state.succeeded());
    state.ended();
    assert!(state.succeeded());
}

#[test]
fn multiple_outputs_cannot_start_overlapping_authentication() {
    let mut state = State::default();
    let generation = state.begin_authentication().unwrap();
    assert!(state.begin_authentication().is_none());
    assert!(state.begin_authentication().is_none());
    assert_eq!(
        state.authentication_finished(Completion {
            generation,
            result: Err(Failure::Denied),
        }),
        Effect::Feedback(Failure::Denied)
    );
    assert!(!state.busy());
    assert!(state.begin_authentication().is_some());
}

#[test]
fn delayed_success_from_previous_attempt_cannot_unlock_or_cancel_new_attempt() {
    let mut state = State::default();
    state.locked();
    let old = state.begin_authentication().unwrap();
    state.authentication_finished(Completion {
        generation: old,
        result: Err(Failure::Denied),
    });
    let current = state.begin_authentication().unwrap();
    assert_ne!(old, current);
    assert_eq!(succeed(&mut state, old), Effect::None);
    assert!(state.busy());
    assert!(state.begin_authentication().is_none());
    assert_eq!(succeed(&mut state, current), Effect::Unlock);
}

#[test]
fn worker_loss_keeps_the_lock_and_allows_a_fresh_attempt() {
    let mut state = State::default();
    state.locked();
    let generation = state.begin_authentication().unwrap();
    assert_eq!(
        state.authentication_finished(Completion {
            generation,
            result: Err(Failure::Unavailable),
        }),
        Effect::Feedback(Failure::Unavailable)
    );
    assert!(!state.succeeded());
    assert!(state.accepts_outputs());
    assert!(state.begin_authentication().is_some());
}

#[test]
fn lock_failure_revokes_a_pending_success() {
    let mut state = State::default();
    let generation = state.begin_authentication().unwrap();
    assert_eq!(succeed(&mut state, generation), Effect::None);
    state.failed();
    assert_eq!(state.locked(), Effect::None);
    assert!(!state.succeeded());
    assert!(state.begin_authentication().is_none());
}

#[test]
fn in_flight_success_after_lock_failure_is_ignored() {
    let mut state = State::default();
    let generation = state.begin_authentication().unwrap();
    state.failed();
    assert_eq!(succeed(&mut state, generation), Effect::None);
    assert!(!state.succeeded());
}

#[test]
fn compositor_finished_before_locked_is_failure() {
    let mut state = State::default();
    state.ended();
    assert!(!state.succeeded());
    assert!(!state.accepts_outputs());
    assert_eq!(state.locked(), Effect::None);
}

#[test]
fn compositor_finished_after_locked_does_not_count_as_authenticated_unlock() {
    let mut state = State::default();
    state.locked();
    let generation = state.begin_authentication().unwrap();
    state.ended();
    assert!(!state.succeeded());
    assert_eq!(succeed(&mut state, generation), Effect::None);
}

#[test]
fn generations_never_wrap_and_accept_an_old_worker_result() {
    let mut state = State {
        generation: u64::MAX,
        ..State::default()
    };
    assert!(state.begin_authentication().is_none());
    assert_eq!(succeed(&mut state, 0), Effect::None);
}
