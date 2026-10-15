use super::{test_support::*, *};
use std::{
    io::Read,
    sync::{Arc, Mutex},
};

#[tokio::test(flavor = "current_thread")]
async fn fresh_snapshots_inhibitor_lifetimes_and_noninteractive_suspend() {
    let bus = Bus::start();
    let state = Arc::new(Mutex::new(State {
        active: true,
        ..State::default()
    }));
    let _service = service(&bus, state.clone()).await;
    let logind = Logind::from_connection(bus.connection().await)
        .await
        .unwrap();
    let snapshot = logind.snapshot().await.unwrap();
    assert_eq!(snapshot.session_id, "test");
    assert_eq!(snapshot.session_path, SESSION_PATH);
    assert!(snapshot.active && snapshot.local_seat);
    assert!(!snapshot.lid_closed && !snapshot.docked && !snapshot.preparing_for_sleep);
    assert_eq!(snapshot.delay_max, Duration::from_secs(5));
    {
        let mut state = state.lock().unwrap();
        state.active = false;
        state.closed = true;
        state.docked = true;
        state.preparing = true;
    }
    let snapshot = logind.snapshot().await.unwrap();
    assert!(!snapshot.active);
    assert!(snapshot.lid_closed && snapshot.docked && snapshot.preparing_for_sleep);
    let sleep = logind.take_sleep_delay().await.unwrap();
    let lid = logind.take_lid_control().await.unwrap();
    {
        let mut state = state.lock().unwrap();
        assert_eq!(
            (&*state.inhibitors[0].0, &*state.inhibitors[0].1),
            ("sleep", "delay")
        );
        assert_eq!(
            (&*state.inhibitors[1].0, &*state.inhibitors[1].1),
            ("handle-lid-switch", "block")
        );
        assert_eq!(
            state.inhibitors[0].2.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    drop(sleep);
    drop(lid);
    for (_, _, reader) in &mut state.lock().unwrap().inhibitors {
        assert_eq!(
            reader.read(&mut [0]).unwrap(),
            0,
            "dropping fd releases inhibitor"
        );
    }
    logind.set_locked_hint(true).await.unwrap();
    logind.suspend().await.unwrap();
    let state = state.lock().unwrap();
    assert!(state.locked);
    assert_eq!(state.suspend_calls, [false]);
}

#[tokio::test(flavor = "current_thread")]
async fn signals_are_authenticated_and_daemon_replacement_requires_reconnection() {
    let bus = Bus::start();
    let state = Arc::new(Mutex::new(State::default()));
    let original = service(&bus, state.clone()).await;
    let mut logind = Logind::from_connection(bus.connection().await)
        .await
        .unwrap();
    let stranger = bus.connection().await;
    signal(&stranger, true).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), logind.next_event())
            .await
            .is_err()
    );
    signal(&original, true).await;
    assert_eq!(
        tokio::time::timeout(RPC_TIMEOUT, logind.next_event())
            .await
            .unwrap()
            .unwrap(),
        Event::PrepareForSleep(true)
    );
    signal(&original, false).await;
    assert_eq!(
        tokio::time::timeout(RPC_TIMEOUT, logind.next_event())
            .await
            .unwrap()
            .unwrap(),
        Event::PrepareForSleep(false)
    );
    original
        .emit_signal(None::<&str>, SESSION_PATH, SESSION_INTERFACE, "Lock", &())
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(RPC_TIMEOUT, logind.next_event())
            .await
            .unwrap()
            .unwrap(),
        Event::SessionLock
    );
    original.release_name(SERVICE).await.unwrap();
    let _replacement = service(&bus, state).await;
    assert!(
        tokio::time::timeout(RPC_TIMEOUT, logind.next_event())
            .await
            .unwrap()
            .is_err()
    );
    assert!(logind.snapshot().await.is_err());
    let reconnected = Logind::from_connection(bus.connection().await)
        .await
        .unwrap();
    assert!(reconnected.snapshot().await.is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn remote_session_cannot_own_lid_and_suspend_timeout_does_not_retry() {
    let bus = Bus::start();
    let state = Arc::new(Mutex::new(State {
        remote: true,
        slow_suspend: true,
        ..State::default()
    }));
    let _service = service(&bus, state.clone()).await;
    let logind = Logind::from_connection(bus.connection().await)
        .await
        .unwrap();
    assert!(!logind.snapshot().await.unwrap().local_seat);
    let before = std::time::Instant::now();
    assert_eq!(
        logind.suspend().await.unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(before.elapsed() < Duration::from_secs(4));
    assert_eq!(state.lock().unwrap().suspend_calls, [false]);
}
