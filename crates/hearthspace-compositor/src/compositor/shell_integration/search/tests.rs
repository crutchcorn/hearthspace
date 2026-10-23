use super::*;

fn launcher_request(now: Instant) -> SearchRequest {
    SearchRequest {
        pending: Some(Pending {
            token: 4,
            expires: now + REQUEST_LIFETIME,
            accepted: false,
            canvas: false,
            cancelled: false,
            output: Some("test-output".into()),
            keys: Vec::new(),
            held_before: HashSet::new(),
        }),
        ..Default::default()
    }
}

#[test]
fn unaccepted_search_requires_current_token_before_freshness_deadline() {
    let now = Instant::now();
    assert!(SearchRequest::default().accept_at(4, now).is_err());
    let mut request = launcher_request(now);
    let deadline = now + REQUEST_LIFETIME;
    assert!(request.valid_at(deadline - Duration::from_nanos(1)));
    assert!(request.accept_at(3, now).is_err());
    assert_eq!(request.pending.as_ref().unwrap().expires, deadline);
    assert!(!request.pending.as_ref().unwrap().accepted);
    assert!(!request.valid_at(deadline));
    assert!(request.accept_at(4, deadline).is_err());
    assert!(
        request
            .accept_at(4, deadline + Duration::from_secs(1))
            .is_err()
    );
    assert_eq!(request.pending.as_ref().unwrap().expires, deadline);
}

#[test]
fn accepted_search_gets_finite_first_buffer_budget_from_acknowledgement() {
    let now = Instant::now();
    let mut request = launcher_request(now);
    let accepted = now + Duration::from_millis(800);
    assert!(request.accept_at(4, accepted).is_ok());
    let deadline = accepted + FIRST_BUFFER_LIFETIME;
    assert_eq!(request.pending.as_ref().unwrap().expires, deadline);
    assert!(request.valid_at(now + Duration::from_millis(1200)));
    assert!(request.valid_at(deadline - Duration::from_nanos(1)));
    assert!(!request.valid_at(deadline));
    assert!(request.accept_at(4, deadline).is_err());
    assert_eq!(request.pending.as_ref().unwrap().expires, deadline);
    assert!(!request.valid_at(deadline + Duration::from_secs(1)));
}

#[test]
fn duplicate_or_wrong_acknowledgement_never_renews_startup_budget() {
    let now = Instant::now();
    let mut request = launcher_request(now);
    assert!(request.accept_at(4, now).is_ok());
    let deadline = request.pending.as_ref().unwrap().expires;
    let later = now + Duration::from_secs(2);
    assert!(request.accept_at(4, later).is_err());
    assert!(request.accept_at(3, later).is_err());
    assert_eq!(request.pending.as_ref().unwrap().expires, deadline);
    assert!(!request.valid_at(deadline));
    request.take_keys();
    assert!(request.accept_at(4, later).is_err());
}

#[test]
fn repeated_focus_requires_same_output_without_renewing_either_deadline() {
    let now = Instant::now();
    let mut request = launcher_request(now);
    let freshness_deadline = now + REQUEST_LIFETIME;
    assert!(request.can_reuse_at(Some("test-output"), now));
    assert_eq!(
        request.pending.as_ref().unwrap().expires,
        freshness_deadline
    );
    assert!(!request.can_reuse_at(Some("test-output"), freshness_deadline));
    assert!(request.accept_at(4, now).is_ok());
    let deadline = request.pending.as_ref().unwrap().expires;
    let later = now + Duration::from_secs(2);
    assert!(request.can_reuse_at(Some("test-output"), later));
    assert!(!request.can_reuse_at(Some("other-output"), later));
    assert!(!request.can_reuse_at(None, later));
    assert!(request.can_reuse_at(Some("test-output"), deadline - Duration::from_nanos(1)));
    assert_eq!(request.pending.as_ref().unwrap().expires, deadline);
    assert!(!request.can_reuse_at(Some("test-output"), deadline));
}

#[test]
fn cancellation_revokes_both_freshness_and_accepted_startup() {
    let now = Instant::now();
    let mut unaccepted = launcher_request(now);
    unaccepted.pending.as_mut().unwrap().cancelled = true;
    assert!(!unaccepted.valid_at(now));
    assert!(unaccepted.accept_at(4, now).is_err());

    let mut accepted = launcher_request(now);
    assert!(accepted.accept_at(4, now).is_ok());
    accepted.pending.as_mut().unwrap().cancelled = true;
    let later = now + Duration::from_secs(2);
    assert!(!accepted.valid_at(later));
    assert!(!accepted.can_reuse_at(Some("test-output"), later));
    assert!(accepted.accept_at(4, later).is_err());
    assert_eq!(
        accepted.pending.as_ref().unwrap().expires,
        now + FIRST_BUFFER_LIFETIME
    );
}

#[test]
fn pending_keys_preserve_modifier_order_and_bound_memory() {
    let shift = 50_u32.into();
    let a = 38_u32.into();
    let time = InputTime::now();
    let mut request = SearchRequest {
        sequence: 1,
        pending: Some(Pending {
            token: 1,
            expires: Instant::now() + REQUEST_LIFETIME,
            accepted: false,
            canvas: false,
            cancelled: false,
            output: None,
            keys: Vec::new(),
            held_before: HashSet::from([shift]),
        }),
    };
    // Shift was already held when search opened. Its release stays behind
    // the first typed letter; it can be paired if creation is cancelled.
    assert!(request.push_key(KeyboardSource::MAIN, None, a, KeyState::Pressed, time));
    assert!(request.push_key(KeyboardSource::MAIN, None, a, KeyState::Released, time));
    assert!(request.push_key(KeyboardSource::MAIN, None, shift, KeyState::Released, time));
    assert!(request.push_key(KeyboardSource::MAIN, None, shift, KeyState::Pressed, time));
    assert!(request.push_key(KeyboardSource::MAIN, None, shift, KeyState::Released, time));
    for _ in 5..MAX_PENDING_KEYS {
        assert!(request.push_key(KeyboardSource::MAIN, None, a, KeyState::Pressed, time));
    }
    assert!(!request.push_key(KeyboardSource::MAIN, None, a, KeyState::Pressed, time));
    let keys = request.take_keys();
    assert_eq!(keys.len(), MAX_PENDING_KEYS);
    assert_eq!(keys[0].code, a);
    assert_eq!(keys[1].code, a);
    assert_eq!(keys[2].code, shift);
    assert!(keys[2].finish_existing);
    assert!(!keys[4].finish_existing);
    assert!(request.take_keys().is_empty());
}

#[test]
fn canvas_input_reservation_outlives_renderer_startup_but_explicit_cancellation_wins() {
    let now = Instant::now();
    let mut request = SearchRequest {
        pending: Some(Pending {
            token: 1,
            expires: now - REQUEST_LIFETIME,
            accepted: true,
            canvas: true,
            cancelled: false,
            output: None,
            keys: Vec::new(),
            held_before: HashSet::new(),
        }),
        ..Default::default()
    };
    let later = now + FIRST_BUFFER_LIFETIME + REQUEST_LIFETIME;
    assert!(request.valid_at(later));
    request.pending.as_mut().unwrap().cancelled = true;
    assert!(!request.valid_at(later));
    request.pending.as_mut().unwrap().cancelled = false;
    request.pending.as_mut().unwrap().canvas = false;
    assert!(!request.valid_at(later));
}

#[test]
fn canvas_pending_uppercase_text_exceeds_legacy_limit_but_remains_bounded() {
    let mut request = SearchRequest {
        pending: Some(Pending {
            token: 1,
            expires: Instant::now() + REQUEST_LIFETIME,
            accepted: true,
            canvas: true,
            cancelled: false,
            output: None,
            keys: Vec::new(),
            held_before: HashSet::new(),
        }),
        ..Default::default()
    };
    let letter = 38_u32.into();
    let shift = 50_u32.into();
    let time = InputTime::now();
    let uppercase = [
        (shift, KeyState::Pressed),
        (letter, KeyState::Pressed),
        (letter, KeyState::Released),
        (shift, KeyState::Released),
    ];
    for index in 0..MAX_CANVAS_PENDING_KEYS {
        let (code, state) = uppercase[index % uppercase.len()];
        assert!(request.push_key(KeyboardSource::MAIN, None, code, state, time));
    }
    assert!(!request.push_key(KeyboardSource::MAIN, None, letter, KeyState::Pressed, time));
    let keys = request.take_keys();
    assert_eq!(keys.len(), MAX_CANVAS_PENDING_KEYS);
    for (index, key) in keys.iter().enumerate() {
        assert_eq!((key.code, key.state), uppercase[index % uppercase.len()]);
    }
}
