use super::*;

#[test]
fn search_acknowledgement_requires_current_unexpired_token_once() {
    let mut request = SearchRequest::default();
    assert!(request.accept(1).is_err());
    request.pending = Some(Pending {
        token: 4,
        expires: Instant::now() + REQUEST_LIFETIME,
        accepted: false,
        canvas: false,
        cancelled: false,
        output: None,
        keys: Vec::new(),
        held_before: HashSet::new(),
    });
    assert!(request.accept(3).is_err());
    assert!(request.accept(4).is_ok());
    assert!(request.accept(4).is_err());
    request.pending.as_mut().unwrap().expires = Instant::now() - REQUEST_LIFETIME;
    assert!(request.accept(4).is_err());
    request.take_keys();
    assert!(request.accept(4).is_err());
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
    let mut request = SearchRequest {
        pending: Some(Pending {
            token: 1,
            expires: Instant::now() - REQUEST_LIFETIME,
            accepted: true,
            canvas: true,
            cancelled: false,
            output: None,
            keys: Vec::new(),
            held_before: HashSet::new(),
        }),
        ..Default::default()
    };
    assert!(request.valid());
    request.pending.as_mut().unwrap().cancelled = true;
    assert!(!request.valid());
    request.pending.as_mut().unwrap().cancelled = false;
    request.pending.as_mut().unwrap().canvas = false;
    assert!(!request.valid());
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
