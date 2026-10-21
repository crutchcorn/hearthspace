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
