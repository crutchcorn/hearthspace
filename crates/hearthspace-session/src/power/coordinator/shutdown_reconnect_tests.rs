use super::*;
use std::{os::unix::fs::PermissionsExt, sync::Mutex};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

async fn request(path: &Path, command: &str) -> serde_json::Value {
    let mut stream = UnixStream::connect(path).await.unwrap();
    stream
        .write_all(format!("{{\"command\":\"{command}\"}}\n").as_bytes())
        .await
        .unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        Duration::from_secs(4),
        BufReader::new(stream).read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    serde_json::from_str(&line).unwrap()
}

async fn wait_until(condition: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

fn assert_busy(reply: &serde_json::Value) {
    assert_eq!(reply["busy"], true);
    for capability in ["available", "reboot_available", "power_off_available"] {
        assert_eq!(reply[capability], false);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn lost_reboot_reply_retains_manual_and_lid_gate_after_real_coordinator_reconnect() {
    let fixture = Fixture::new().await;
    fixture.state.lock().unwrap().slow_shutdown = true;
    let runtime = fixture.control.path.parent().unwrap();
    std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (server, shared, requests) =
        crate::power::api::Server::bind(runtime, &fixture.control.path).unwrap();
    let endpoint = runtime.join("power-control.sock");
    let server = tokio::spawn(server.run());
    let connections = Mutex::new(Vec::<zbus::Connection>::new());
    tokio::select! {
        () = run_with_connector(
            runtime.to_owned(), fixture.control.path.clone(), shared.clone(), requests,
            || async {
                let (logind, connection) = fixture.connect_logind().await?;
                connections.lock().unwrap().push(connection);
                Ok(logind)
            },
            || false,
        ) => panic!("coordinator stopped"),
        result = tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                if request(&endpoint, "status").await["reboot_available"] == true {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let reboot = tokio::spawn({
                let endpoint = endpoint.clone();
                async move { request(&endpoint, "reboot").await }
            });
            wait_until(|| !fixture.state.lock().unwrap().shutdown_calls.is_empty()).await;
            // The fake service has received the reboot and keeps its reply
            // pending. Close only the client's real private D-Bus transport.
            let connection = connections.lock().unwrap()[0].clone();
            connection.close().await.unwrap();
            let reply = reboot.await.unwrap();
            assert_eq!(reply["ok"], false);
            assert!(reply["error"].as_str().unwrap().contains("not be retried automatically"));
            wait_status(runtime, "degraded").await;
            assert_busy(&request(&endpoint, "status").await);

            // Exercise the actual supervisor reconnect loop and recovery, not
            // a hand-constructed replacement Cycle. The old call is unresolved.
            wait_until(|| connections.lock().unwrap().len() >= 2).await;
            wait_status(runtime, "recovering").await;
            fixture.control.state.lock().unwrap().confirmed = true;
            let recovered = wait_status(runtime, "awake").await;
            assert_eq!(recovered["recovery_ready"], true);
            assert_eq!(recovered["lid_owned"], true);
            assert!(!fixture.state.lock().unwrap().preparing_shutdown);
            assert_busy(&request(&endpoint, "status").await);
            for action in ["reboot", "power_off", "suspend"] {
                let reply = request(&endpoint, action).await;
                assert_eq!(reply["ok"], false);
                assert_busy(&reply);
            }

            let commands = fixture.control.state.lock().unwrap().commands.len();
            let reads = {
                let mut state = fixture.state.lock().unwrap();
                state.closed = true;
                state.lid_reads
            };
            // Two subsequent snapshots prove that the recovered, active local
            // seat processed the open-to-closed edge with no external display.
            wait_until(|| fixture.state.lock().unwrap().lid_reads >= reads + 2).await;
            let state = fixture.state.lock().unwrap();
            assert_eq!(state.shutdown_calls, [(Action::Reboot, 1)]);
            assert!(state.suspend_calls.is_empty());
            assert!(!fixture.control.state.lock().unwrap().commands[commands..]
                .iter().any(|command| command == "lock-shield"));
        }) => result.unwrap(),
    }
    server.abort();
    let _ = server.await;
    assert!(!endpoint.exists());
}
