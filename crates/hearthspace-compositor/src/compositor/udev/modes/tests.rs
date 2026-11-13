use super::*;

fn pending(now: Instant) -> Pending {
    Pending {
        token: 9,
        output: "eDP-1".into(),
        previous: "native".into(),
        requested: "small".into(),
        deadline: now + Duration::from_secs(5),
        ready: false,
        device: super::super::identity::DeviceToken {
            id: 1,
            incarnation: 1,
        },
    }
}

#[test]
fn confirmation_budget_starts_after_first_presentation_and_never_extends() {
    let now = Instant::now();
    let mut p = pending(now);
    assert!(!p.can_keep(9, now));
    assert!(!p.poll(now + Duration::from_secs(4), true, false));
    assert!(p.can_keep(9, now + Duration::from_secs(4)));
    assert!(!p.can_keep(8, now + Duration::from_secs(4)));
    assert!(!p.poll(now + Duration::from_secs(18), true, false));
    assert!(p.poll(now + Duration::from_secs(19), true, false));
    assert!(!p.can_keep(9, now + Duration::from_secs(19)));
}

#[test]
fn failed_presentation_hotplug_lock_and_inactivity_require_rollback() {
    let now = Instant::now();
    assert!(pending(now).poll(now + Duration::from_secs(5), false, false));
    let mut p = pending(now);
    assert!(!p.poll(now, true, false));
    assert!(p.poll(now + Duration::from_millis(1), true, true));
}

#[test]
fn preview_and_stale_confirmation_cannot_overwrite_saved_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("display-modes.json");
    std::fs::write(&path, r#"{"eDP-1":"native"}"#).unwrap();
    let now = Instant::now();
    let mut state = State::load_path(Some(path.clone()));
    state.selected.insert("eDP-1".into(), "small".into());
    state.pending = Some(pending(now));
    assert!(state.confirm(9, now).is_err());
    assert_eq!(
        State::load_path(Some(path.clone())).selected["eDP-1"],
        "native"
    );
    assert!(!state.pending.as_mut().unwrap().poll(now, true, false));
    assert!(state.confirm(8, now).is_err());
    assert_eq!(
        State::load_path(Some(path.clone())).selected["eDP-1"],
        "native"
    );
    state.confirm(9, now).unwrap();
    assert!(state.pending.is_none());
    assert_eq!(State::load_path(Some(path)).selected["eDP-1"], "small");
}

#[test]
fn failed_save_keeps_the_preview_timed_and_does_not_claim_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut state = State::load_path(Some(dir.path().to_path_buf()));
    state.pending = Some(pending(now));
    state.pending.as_mut().unwrap().poll(now, true, false);
    assert!(state.confirm(9, now).is_err());
    assert!(state.pending.is_some());
    assert!(
        state
            .pending
            .as_mut()
            .unwrap()
            .poll(now + Duration::from_secs(15), true, false)
    );
}
