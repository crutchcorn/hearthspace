use super::*;
use crate::power::logind::test_support::State;

async fn perform(fixture: &Fixture, cycle: &mut Cycle, action: Action) -> Reply {
    let (request, reply) = Request::test_action(action);
    cycle
        .request_power(
            request,
            &fixture.logind,
            &fixture.control.path,
            &mut Status::default(),
        )
        .await;
    reply.await.unwrap()
}

#[tokio::test]
async fn shutdown_capabilities_are_independent_and_challenge_never_authorizes() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    let mut status = Status::default();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    fixture.state.lock().unwrap().can_suspend = Some("no");
    for capability in ["no", "na", "challenge", "unexpected"] {
        fixture.state.lock().unwrap().can_reboot = Some(capability);
        let reply = cycle
            .capabilities(&fixture.logind, &snapshot, &status)
            .await;
        assert!(!reply.available && !reply.reboot_available && reply.power_off_available);
        if capability == "challenge" {
            assert!(
                reply
                    .reboot_error
                    .unwrap()
                    .contains("administrator authorization")
            );
        }
        assert!(!perform(&fixture, &mut cycle, Action::Reboot).await.ok);
    }
    fixture.state.lock().unwrap().can_reboot = Some("yes");
    fixture.state.lock().unwrap().can_power_off = Some("challenge");
    let reply = cycle
        .capabilities(&fixture.logind, &snapshot, &status)
        .await;
    assert!(reply.reboot_available && !reply.power_off_available);
    assert!(!perform(&fixture, &mut cycle, Action::PowerOff).await.ok);
    assert!(fixture.state.lock().unwrap().shutdown_calls.is_empty());
}

#[tokio::test]
async fn reboot_and_power_off_dispatch_once_with_inhibitor_checks_and_share_busy_gate() {
    for action in [Action::Reboot, Action::PowerOff] {
        let fixture = Fixture::new().await;
        let mut cycle = fixture.cycle().await;
        assert!(perform(&fixture, &mut cycle, action).await.ok);
        assert!(cycle.shared.shutdown_dispatched());
        for other in [Action::Reboot, Action::PowerOff, Action::Suspend] {
            assert!(!perform(&fixture, &mut cycle, other).await.ok);
        }
        let snapshot = fixture.logind.snapshot().await.unwrap();
        let reply = cycle
            .capabilities(&fixture.logind, &snapshot, &Status::default())
            .await;
        assert!(reply.busy);
        assert!(!reply.available && !reply.reboot_available && !reply.power_off_available);
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.shutdown_calls, [(action, 1)]);
        assert!(state.suspend_calls.is_empty());
        assert_eq!(fixture.control.state.lock().unwrap().commands, ["status"]);
    }
}

#[tokio::test]
async fn shutdown_rechecks_its_own_active_local_session_after_authorization() {
    for initial in [
        State::default(),
        State {
            active: true,
            remote: true,
            ..State::default()
        },
        State {
            active: true,
            deactivate_on_shutdown_query: true,
            ..State::default()
        },
        State {
            active: true,
            preparing: true,
            ..State::default()
        },
        State {
            active: true,
            preparing_shutdown: true,
            ..State::default()
        },
    ] {
        let fixture = Fixture::with_state(initial).await;
        let mut cycle = fixture.cycle().await;
        assert!(!perform(&fixture, &mut cycle, Action::Reboot).await.ok);
        assert!(fixture.state.lock().unwrap().shutdown_calls.is_empty());
    }
}

#[tokio::test]
async fn external_shutdown_and_pending_suspend_disable_other_power_actions() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    let mut status = Status::default();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    fixture.state.lock().unwrap().preparing_shutdown = true;
    let reply = cycle
        .capabilities(&fixture.logind, &snapshot, &status)
        .await;
    assert!(
        reply.busy && !reply.available && !reply.reboot_available && !reply.power_off_available
    );
    fixture.state.lock().unwrap().preparing_shutdown = false;
    let (request, _reply) = Request::test_request();
    cycle
        .request_power(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    assert!(cycle.manual.busy());
    assert!(!perform(&fixture, &mut cycle, Action::Reboot).await.ok);
    assert!(!perform(&fixture, &mut cycle, Action::PowerOff).await.ok);
    assert!(fixture.state.lock().unwrap().shutdown_calls.is_empty());
}

#[tokio::test]
async fn shutdown_rejects_unavailable_compositor_cancelled_request_and_lost_bus_owner() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    fixture.control.state.lock().unwrap().inactive = true;
    assert!(!perform(&fixture, &mut cycle, Action::PowerOff).await.ok);
    fixture.control.state.lock().unwrap().inactive = false;
    fixture.control.state.lock().unwrap().recovery_failed = true;
    assert!(!perform(&fixture, &mut cycle, Action::PowerOff).await.ok);
    fixture.control.state.lock().unwrap().recovery_failed = false;
    let (request, reply) = Request::test_action(Action::PowerOff);
    drop(reply);
    cycle
        .request_power(
            request,
            &fixture.logind,
            &fixture.control.path,
            &mut Status::default(),
        )
        .await;
    fixture.release_logind_name().await;
    assert!(!perform(&fixture, &mut cycle, Action::PowerOff).await.ok);
    assert!(fixture.state.lock().unwrap().shutdown_calls.is_empty());
}

#[tokio::test]
async fn shutdown_respects_block_and_weak_inhibitors_but_leaves_delay_to_logind() {
    for (what, mode, accepted) in [
        ("shutdown", "block", false),
        ("sleep:shutdown", "block-weak", false),
        ("shutdown", "delay", true),
        ("sleep", "block", true),
    ] {
        let fixture = Fixture::new().await;
        let mut cycle = fixture.cycle().await;
        fixture.state.lock().unwrap().shutdown_inhibitors.push((
            what.into(),
            "Document editor".into(),
            "Unsaved work".into(),
            mode.into(),
            1000,
            123,
        ));
        let snapshot = fixture.logind.snapshot().await.unwrap();
        let status = cycle
            .capabilities(&fixture.logind, &snapshot, &Status::default())
            .await;
        assert_eq!(status.reboot_available, accepted);
        assert_eq!(status.power_off_available, accepted);
        let reply = perform(&fixture, &mut cycle, Action::PowerOff).await;
        assert_eq!(reply.ok, accepted);
        if !accepted {
            assert!(reply.error.unwrap().contains("Unsaved work"));
            assert!(fixture.state.lock().unwrap().shutdown_calls.is_empty());
        }
    }
}

#[tokio::test]
async fn concrete_logind_rejection_is_reported_without_retry_and_allows_explicit_new_request() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    fixture.state.lock().unwrap().fail_shutdown = true;
    let reply = perform(&fixture, &mut cycle, Action::Reboot).await;
    assert!(!reply.ok);
    assert!(reply.error.unwrap().contains("injected shutdown denial"));
    assert!(!cycle.shared.shutdown_dispatched());
    let snapshot = fixture.logind.snapshot().await.unwrap();
    fixture
        .tick(&mut cycle, &snapshot, &mut Status::default())
        .await;
    assert_eq!(
        fixture.state.lock().unwrap().shutdown_calls,
        [(Action::Reboot, 1)]
    );
    fixture.state.lock().unwrap().fail_shutdown = false;
    assert!(perform(&fixture, &mut cycle, Action::PowerOff).await.ok);
}

#[tokio::test]
async fn ambiguous_shutdown_timeout_is_bounded_retains_busy_gate_and_never_retries() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    fixture.state.lock().unwrap().slow_shutdown = true;
    let reply = tokio::time::timeout(
        Duration::from_secs(4),
        perform(&fixture, &mut cycle, Action::Reboot),
    )
    .await
    .unwrap();
    assert!(!reply.ok);
    assert!(
        reply
            .error
            .unwrap()
            .contains("not be retried automatically")
    );
    assert!(cycle.shared.shutdown_dispatched());
    assert!(!perform(&fixture, &mut cycle, Action::PowerOff).await.ok);
    assert_eq!(
        fixture.state.lock().unwrap().shutdown_calls,
        [(Action::Reboot, 1)]
    );
}

#[tokio::test]
async fn private_endpoint_routes_power_off_without_suspend_capability() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    async fn request(path: &Path, command: &str) -> serde_json::Value {
        let mut stream = UnixStream::connect(path).await.unwrap();
        stream
            .write_all(format!("{{\"command\":\"{command}\"}}\n").as_bytes())
            .await
            .unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).await.unwrap();
        serde_json::from_str(&line).unwrap()
    }

    let mut fixture = Fixture::new().await;
    fixture.state.lock().unwrap().can_suspend = Some("no");
    let runtime = fixture.control.path.parent().unwrap();
    std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (server, shared, mut requests) =
        crate::power::api::Server::bind(runtime, &fixture.control.path).unwrap();
    let endpoint = runtime.join("power-control.sock");
    let server = tokio::spawn(server.run());
    let mut status = Status::default();
    tokio::select! {
        result = connected(&mut fixture.logind, runtime, &fixture.control.path, &mut status, &shared, &mut requests, || false) => panic!("coordinator exited: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let status = request(&endpoint, "status").await;
                if status["power_off_available"] == true {
                    assert_eq!(status["available"], false);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(request(&endpoint, "power_off").await["ok"], true);
            assert_eq!(request(&endpoint, "reboot").await["ok"], false);
            assert_eq!(request(&endpoint, "status").await["busy"], true);
            assert_eq!(fixture.state.lock().unwrap().shutdown_calls, [(Action::PowerOff, 1)]);
        }) => result.unwrap(),
    }
    server.abort();
    let _ = server.await;
    assert!(!endpoint.exists());
}
