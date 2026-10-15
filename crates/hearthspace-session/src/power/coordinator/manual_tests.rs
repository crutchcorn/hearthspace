use super::*;

#[tokio::test]
async fn manual_suspend_works_without_lid_ownership_only_after_presented_lock() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    let mut status = Status::default();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    cycle.lid = None;
    assert!(
        cycle
            .manual_capability(&fixture.logind, &snapshot, &status)
            .await
            .available
    );
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    assert!(cycle.manual.busy());
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
    fixture.control.state.lock().unwrap().confirmed = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(reply.await.unwrap().ok);
    assert_eq!(fixture.state.lock().unwrap().suspend_calls, [false]);
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    assert!(!reply.await.unwrap().ok);
    assert_eq!(fixture.state.lock().unwrap().suspend_calls, [false]);
}

#[tokio::test]
async fn manual_suspend_rejects_unsupported_inactive_and_unhealthy_sessions_before_locking() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    let mut status = Status::default();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    for capability in ["no", "na", "challenge"] {
        fixture.state.lock().unwrap().can_suspend = Some(capability);
        assert!(
            !cycle
                .manual_capability(&fixture.logind, &snapshot, &status)
                .await
                .available
        );
        let (request, reply) = Request::test_request();
        cycle
            .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
            .await;
        assert!(!reply.await.unwrap().ok);
    }
    fixture.state.lock().unwrap().can_suspend = Some("yes");
    fixture.state.lock().unwrap().active = false;
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    assert!(!reply.await.unwrap().ok);
    fixture.state.lock().unwrap().active = true;
    fixture.control.state.lock().unwrap().recovery_failed = true;
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    assert!(!reply.await.unwrap().ok);
    assert!(
        !fixture
            .control
            .state
            .lock()
            .unwrap()
            .commands
            .contains(&"lock-shield".into())
    );
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
}

#[tokio::test]
async fn changed_active_session_and_failed_shield_cancel_the_request() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    let mut status = Status::default();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    fixture.control.state.lock().unwrap().fail_lock = true;
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    assert!(!reply.await.unwrap().ok);
    fixture.control.state.lock().unwrap().fail_lock = false;
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    fixture.control.state.lock().unwrap().confirmed = true;
    fixture.state.lock().unwrap().active = false;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(!reply.await.unwrap().ok);
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
}

#[tokio::test]
async fn denied_and_timed_out_suspend_surface_errors_without_retrying() {
    for slow in [false, true] {
        let fixture = Fixture::new().await;
        let mut cycle = fixture.cycle().await;
        let snapshot = fixture.logind.snapshot().await.unwrap();
        let mut status = Status::default();
        fixture.tick(&mut cycle, &snapshot, &mut status).await;
        let (request, reply) = Request::test_request();
        cycle
            .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
            .await;
        fixture.control.state.lock().unwrap().confirmed = true;
        {
            let mut state = fixture.state.lock().unwrap();
            state.slow_suspend = slow;
            state.fail_suspend = !slow;
        }
        fixture.tick(&mut cycle, &snapshot, &mut status).await;
        let reply = reply.await.unwrap();
        assert!(!reply.ok);
        assert!(reply.error.unwrap().contains("Suspend failed"));
        fixture.tick(&mut cycle, &snapshot, &mut status).await;
        assert_eq!(fixture.state.lock().unwrap().suspend_calls, [false]);
    }
}

#[tokio::test]
async fn expired_client_cannot_suspend_after_a_late_lock_confirmation() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    let mut status = Status::default();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    drop(reply);
    fixture.control.state.lock().unwrap().confirmed = true;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(!cycle.manual.busy());
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
}

#[tokio::test]
async fn lost_logind_owner_cancels_a_shielded_manual_suspend() {
    let fixture = Fixture::new().await;
    let mut cycle = fixture.cycle().await;
    let snapshot = fixture.logind.snapshot().await.unwrap();
    let mut status = Status::default();
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    let (request, reply) = Request::test_request();
    cycle
        .request_suspend(request, &fixture.logind, &fixture.control.path, &mut status)
        .await;
    fixture.control.state.lock().unwrap().confirmed = true;
    fixture.release_logind_name().await;
    fixture.tick(&mut cycle, &snapshot, &mut status).await;
    assert!(!reply.await.unwrap().ok);
    assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
}

#[tokio::test]
async fn private_endpoint_routes_one_suspend_through_the_live_coordinator() {
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
    let runtime = fixture.control.path.parent().unwrap();
    std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (server, shared, mut requests) =
        crate::power::api::Server::bind(runtime, &fixture.control.path).unwrap();
    let endpoint = runtime.join("power-control.sock");
    let server = tokio::spawn(server.run());
    let mut status = Status::default();
    tokio::select! {
        result = connected(&mut fixture.logind, runtime, &fixture.control.path, &mut status, &shared, &mut requests) => panic!("coordinator exited: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if request(&endpoint, "status").await["available"] == true { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let suspend = tokio::spawn({ let endpoint = endpoint.clone(); async move { request(&endpoint, "suspend").await } });
            loop {
                if fixture.control.state.lock().unwrap().locked { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(fixture.state.lock().unwrap().suspend_calls.is_empty());
            assert_eq!(request(&endpoint, "suspend").await["ok"], false);
            fixture.control.state.lock().unwrap().confirmed = true;
            assert_eq!(suspend.await.unwrap()["ok"], true);
            assert_eq!(fixture.state.lock().unwrap().suspend_calls, [false]);
        }) => result.unwrap(),
    }
    server.abort();
    let _ = server.await;
    assert!(!endpoint.exists());
}
