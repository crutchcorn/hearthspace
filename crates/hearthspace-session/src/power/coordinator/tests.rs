use super::*;
use std::io::Read;

#[path = "test_support.rs"]
mod test_support;
use test_support::*;

#[tokio::test(flavor = "current_thread")]
async fn sleep_delay_waits_for_presentation_and_resume_rearms_it_once() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let mut status = Status::default();
    let snapshot = fixture.logind.snapshot().await.unwrap();
    cycle
        .sleep_event(
            true,
            &snapshot,
            &fixture.logind,
            &fixture.control.path,
            &mut status,
        )
        .await
        .unwrap();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(cycle.delay.is_some());
    assert!(cycle.preparing.is_some());
    assert_eq!(
        fixture.state.lock().unwrap().inhibitors[0]
            .2
            .read(&mut [0])
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    fixture.control.state.lock().unwrap().confirmed = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(cycle.delay.is_none());
    assert_eq!(status.phase, "sleeping");
    assert_eq!(
        fixture.state.lock().unwrap().inhibitors[0]
            .2
            .read(&mut [0])
            .unwrap(),
        0
    );
    cycle
        .sleep_event(
            true,
            &snapshot,
            &fixture.logind,
            &fixture.control.path,
            &mut status,
        )
        .await
        .unwrap();
    assert_eq!(status.sleep_count, 1);
    cycle
        .sleep_event(
            false,
            &snapshot,
            &fixture.logind,
            &fixture.control.path,
            &mut status,
        )
        .await
        .unwrap();
    assert!(cycle.delay.is_some());
    assert!(!status.lock_confirmed);
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(status.phase, "recovering");
    fixture.control.state.lock().unwrap().confirmed = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(status.phase, "awake");
    cycle
        .sleep_event(
            false,
            &snapshot,
            &fixture.logind,
            &fixture.control.path,
            &mut status,
        )
        .await
        .unwrap();
    assert_eq!(status.resume_count, 1);
    assert_eq!(
        fixture
            .control
            .state
            .lock()
            .unwrap()
            .commands
            .iter()
            .filter(|cmd| *cmd == "session-resume")
            .count(),
        1
    );
}

#[tokio::test(flavor = "current_thread")]
async fn lid_suspend_requires_presentation_and_does_not_repeat() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let mut status = Status::default();
    let mut snapshot = fixture.logind.snapshot().await.unwrap();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    fixture.state.lock().unwrap().closed = true;
    snapshot.lid_closed = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(cycle.lid_request.is_some());
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
    fixture.control.state.lock().unwrap().confirmed = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(cycle.lid_request.is_none());
    assert_eq!(fixture.state.lock().unwrap().suspend_calls, [false]);
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(fixture.state.lock().unwrap().suspend_calls, [false]);
}

#[tokio::test(flavor = "current_thread")]
async fn late_confirmation_and_status_failures_cannot_extend_suspend_deadlines() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let mut status = Status::default();
    let mut snapshot = fixture.logind.snapshot().await.unwrap();
    snapshot.lid_closed = true;
    fixture.state.lock().unwrap().closed = true;
    cycle.preparing = Some(Instant::now());
    cycle.lid_request = Some(Instant::now());
    fixture.control.state.lock().unwrap().fail_status = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(cycle.preparing.is_none() && cycle.delay.is_none());
    assert!(cycle.lid_request.is_none());
    {
        let mut control = fixture.control.state.lock().unwrap();
        control.fail_status = false;
        control.locked = true;
        control.confirmed = true;
    }
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
    cycle.lid_request = Some(Instant::now());
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(cycle.lid_request.is_none());
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn final_active_session_check_rejects_an_outdated_lid_snapshot() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let mut status = Status::default();
    fixture.state.lock().unwrap().closed = true;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    fixture.state.lock().unwrap().active = false;
    cycle.lid_request = Some(Instant::now() + Duration::from_secs(10));
    {
        let mut control = fixture.control.state.lock().unwrap();
        control.locked = true;
        control.confirmed = true;
    }
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(cycle.lid_request.is_none());
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn transient_startup_status_failure_recovers_and_inactive_vt_defers_recovery() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let mut status = Status::default();
    let mut snapshot = fixture.logind.snapshot().await.unwrap();
    fixture.control.state.lock().unwrap().fail_status = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(status.phase, "degraded");
    fixture.control.state.lock().unwrap().fail_status = false;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(status.phase, "awake");
    assert!(status.last_error.is_none());
    snapshot.active = false;
    cycle.recovering = Some(Instant::now());
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(status.phase, "recovering");
    assert!(cycle.recovering.unwrap() > Instant::now());
    assert!(cycle.lid.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn property_reconciliation_handles_missing_sleep_and_resume_signals() {
    let mut fixture = Fixture::new().await;
    let mut status = Status::default();
    let runtime = fixture.control.path.parent().unwrap();
    let shared = Shared::new();
    let (_sender, mut requests) = tokio::sync::mpsc::channel(1);
    tokio::select! {
        result = connected(&mut fixture.logind, runtime, &fixture.control.path, &mut status, &shared, &mut requests) => {
            panic!("coordinator ended early: {result:?}");
        }
        () = async {
            wait_status(runtime, "awake").await;
            fixture.state.lock().unwrap().preparing = true;
            let preparing = wait_status(runtime, "preparing-sleep").await;
            assert_eq!(preparing["sleep_count"], 1);
            assert_eq!(preparing["sleep_inhibited"], true);
            fixture.control.state.lock().unwrap().confirmed = true;
            let sleeping = wait_status(runtime, "sleeping").await;
            assert_eq!(sleeping["sleep_inhibited"], false);
            fixture.state.lock().unwrap().preparing = false;
            let recovering = wait_status(runtime, "recovering").await;
            assert_eq!(recovering["resume_count"], 1);
            assert_eq!(recovering["sleep_inhibited"], true);
            fixture.control.state.lock().unwrap().confirmed = true;
            wait_status(runtime, "awake").await;
        } => {}
    }
}

#[tokio::test(flavor = "current_thread")]
async fn reconnect_refreshes_hardware_without_inventing_a_sleep_count() {
    let mut fixture = Fixture::new().await;
    let mut status = Status {
        session_id: "previous".into(),
        ..Status::default()
    };
    let runtime = fixture.control.path.parent().unwrap();
    let shared = Shared::new();
    let (_sender, mut requests) = tokio::sync::mpsc::channel(1);
    tokio::select! {
        result = connected(&mut fixture.logind, runtime, &fixture.control.path, &mut status, &shared, &mut requests) => {
            panic!("coordinator ended early: {result:?}");
        }
        () = async {
            let recovering = wait_status(runtime, "recovering").await;
            assert_eq!(recovering["sleep_count"], 0);
            assert_eq!(recovering["resume_count"], 0);
            assert_eq!(recovering["sleep_inhibited"], true);
            assert!(fixture.control.state.lock().unwrap().commands.starts_with(&["lock-shield".into(), "session-resume".into()]));
            fixture.control.state.lock().unwrap().confirmed = true;
            wait_status(runtime, "awake").await;
        } => {}
    }
}

#[tokio::test(flavor = "current_thread")]
async fn authenticated_unlock_completes_only_an_acknowledged_resume() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let mut status = Status::default();
    let snapshot = fixture.logind.snapshot().await.unwrap();
    cycle
        .sleep_event(
            true,
            &snapshot,
            &fixture.logind,
            &fixture.control.path,
            &mut status,
        )
        .await
        .unwrap();
    cycle
        .sleep_event(
            false,
            &snapshot,
            &fixture.logind,
            &fixture.control.path,
            &mut status,
        )
        .await
        .unwrap();
    fixture.control.state.lock().unwrap().locked = false;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(status.phase, "awake");
    assert!(
        !status.lock_confirmed,
        "unlock is not evidence of a presented lock"
    );
    assert!(cycle.recovering.is_none());

    cycle.recovering = Some(Instant::now());
    cycle.resume_accepted = false;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(
        status.phase, "degraded",
        "failed resume must not be cleared by old status"
    );
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert_eq!(status.phase, "degraded");
}

#[tokio::test(flavor = "current_thread")]
async fn a_presented_lock_cannot_hide_failed_input_recovery() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let mut status = Status::default();
    let snapshot = fixture.logind.snapshot().await.unwrap();
    cycle.recovering = Some(Instant::now() + Duration::from_secs(12));
    {
        let mut control = fixture.control.state.lock().unwrap();
        control.locked = true;
        control.confirmed = true;
        control.recovery_failed = true;
    }
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(status.lock_confirmed);
    assert!(!status.recovery_ready);
    assert!(cycle.recovering.is_some());
    assert_ne!(status.phase, "awake");
    fixture.control.state.lock().unwrap().recovery_failed = false;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(status.recovery_ready);
    assert_eq!(status.phase, "awake");
}

#[path = "manual_tests.rs"]
mod manual_tests;
