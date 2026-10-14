use super::*;

fn input<'a>() -> Incoming<'a> {
    Incoming {
        sender: ":1.2",
        replaces_id: 0,
        app_name: "Chat",
        summary: "Message",
        body: "Hello",
        actions: &[],
        urgency: 1,
        resident: false,
        transient: false,
        expire_timeout: -1,
    }
}

#[test]
fn replacement_retains_id_and_refreshes_deadline_atomically() {
    let now = Instant::now();
    let mut state = State::default();
    let id = state.notify(input(), now).unwrap();
    let mut replacement = input();
    replacement.replaces_id = id;
    replacement.summary = "Edited";
    assert_eq!(
        state
            .notify(replacement, now + Duration::from_secs(5))
            .unwrap(),
        id
    );
    state.expire(now + Duration::from_secs(9));
    assert_eq!(state.list().len(), 1);
    assert!(state.list()[0].active);
    assert_eq!(state.list()[0].summary, "Edited");
    assert!(state.take_events().is_empty());
    state.expire(now + Duration::from_secs(14));
    assert!(
        matches!(state.take_events()[0], Event::Closed { id: found, reason: 1, .. } if found == id)
    );
}

#[test]
fn foreign_clients_cannot_replace_or_close_notifications() {
    let mut state = State::default();
    let id = state.notify(input(), Instant::now()).unwrap();
    let mut foreign = input();
    foreign.sender = ":1.3";
    foreign.replaces_id = id;
    assert!(state.notify(foreign, Instant::now()).is_err());
    assert!(state.close(id, 3, Some(":1.3")).is_err());
    assert!(state.list()[0].active);
    state.close(id, 3, Some(":1.2")).unwrap();
    assert_eq!(state.list()[0].close_reason, Some(3));
    assert!(state.close(id, 3, Some(":1.2")).is_err());
}

#[test]
fn action_goes_to_original_sender_then_closes_once() {
    let actions = vec!["open".into(), "Open".into()];
    let mut incoming = input();
    incoming.actions = &actions;
    let mut state = State::default();
    let id = state.notify(incoming, Instant::now()).unwrap();
    assert!(state.invoke(id, "not-an-action").is_err());
    state.invoke(id, "open").unwrap();
    let events = state.take_events();
    assert!(
        matches!(&events[0], Event::Action { sender, id: found, key } if sender == ":1.2" && *found == id && key == "open")
    );
    assert!(matches!(events[1], Event::Closed { reason: 2, .. }));
    assert!(state.invoke(id, "open").is_err());
    assert!(state.list()[0].actions.is_empty());
}

#[test]
fn resident_action_stays_active_and_transient_close_has_no_history() {
    let actions = vec!["open".into(), "Open".into()];
    let mut incoming = input();
    incoming.actions = &actions;
    incoming.resident = true;
    incoming.transient = true;
    let mut state = State::default();
    let id = state.notify(incoming, Instant::now()).unwrap();
    state.invoke(id, "open").unwrap();
    assert!(state.list()[0].active);
    assert_eq!(state.take_events().len(), 1);
    state.close(id, 2, None).unwrap();
    assert!(state.list().is_empty());
}

#[test]
fn explicit_zero_and_default_critical_do_not_expire() {
    let now = Instant::now();
    let mut state = State::default();
    let mut persistent = input();
    persistent.expire_timeout = 0;
    state.notify(persistent, now).unwrap();
    let mut critical = input();
    critical.urgency = 2;
    state.notify(critical, now).unwrap();
    state.expire(now + Duration::from_secs(86400));
    assert!(state.list().iter().all(|entry| entry.active));
}

#[test]
fn retention_and_active_count_are_bounded_without_silently_closing_live_items() {
    let mut state = State::default();
    for _ in 0..100 {
        let id = state.notify(input(), Instant::now()).unwrap();
        state.close(id, 2, None).unwrap();
        state.take_events();
    }
    assert_eq!(state.list().len(), MAX_RETAINED);
    for _ in 0..MAX_ACTIVE {
        state.notify(input(), Instant::now()).unwrap();
    }
    assert!(state.notify(input(), Instant::now()).is_err());
    assert_eq!(state.list().len(), MAX_RETAINED);
    assert_eq!(
        state.list().iter().filter(|entry| entry.active).count(),
        MAX_ACTIVE
    );
}

#[test]
fn worst_case_text_fits_control_response_and_utf8_boundaries() {
    let mut state = State::default();
    let noisy = "\"\\\n".repeat(1000);
    let actions: Vec<_> = (0..4)
        .flat_map(|_| ["\"".repeat(32), "\\".repeat(48)])
        .collect();
    for index in 0..MAX_RETAINED + MAX_ACTIVE {
        let mut incoming = input();
        incoming.app_name = &noisy;
        incoming.summary = &noisy;
        incoming.body = &noisy;
        incoming.actions = &actions;
        let id = state.notify(incoming, Instant::now()).unwrap();
        if index < MAX_RETAINED {
            state.close(id, 2, None).unwrap();
            state.take_events();
        }
    }
    assert!(
        serde_json::to_vec(&serde_json::json!({"ok": true, "notifications": state.list()}))
            .unwrap()
            .len()
            < 64 * 1024
    );
    assert_eq!(text("🧡🧡🧡", 9), "🧡🧡");
    assert_eq!(text("a\u{1b}\tb", 20), "ab");
}

#[test]
fn pending_signal_queue_is_bounded_and_retry_does_not_mutate_state() {
    let mut state = State::default();
    for _ in 0..MAX_EVENTS {
        let id = state.notify(input(), Instant::now()).unwrap();
        state.close(id, 2, None).unwrap();
    }
    let id = state.notify(input(), Instant::now()).unwrap();
    assert!(state.close(id, 2, None).is_err());
    assert!(state.list()[0].active);
    assert_eq!(state.take_events().len(), MAX_EVENTS);
    state.close(id, 2, None).unwrap();
}
