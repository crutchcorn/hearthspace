use super::*;
use tokio::io::{AsyncBufReadExt, BufReader};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hearthspace-power-api-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn exchange(path: &Path, request: &[u8]) -> serde_json::Value {
    let mut stream = UnixStream::connect(path).await.unwrap();
    stream.write_all(request).await.unwrap();
    let mut reply = String::new();
    tokio::time::timeout(
        Duration::from_secs(2),
        BufReader::new(stream).read_line(&mut reply),
    )
    .await
    .unwrap()
    .unwrap();
    serde_json::from_str(&reply).unwrap()
}

#[tokio::test]
async fn private_socket_status_rejection_and_cleanup() {
    let dir = Directory::new();
    let path = dir.0.join("power-control.sock");
    let (server, shared, _requests) =
        Server::bind(&dir.0, &dir.0.join("shell-command.sock")).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert!(Server::bind(&dir.0, &dir.0.join("shell-command.sock")).is_err());
    let task = tokio::spawn(server.run());
    let reply = exchange(&path, b"{\"command\":\"status\"}\n").await;
    assert_eq!(reply["available"], false);
    assert_eq!(
        exchange(&path, b"{\"command\":\"suspend\"}\n").await["ok"],
        false
    );
    assert_eq!(
        exchange(&path, b"{\"command\":\"hibernate\"}\n").await["ok"],
        false
    );
    let oversized = [b'x'; 258];
    assert_eq!(exchange(&path, &oversized).await["ok"], false);
    shared.update(Reply {
        ok: true,
        available: true,
        busy: false,
        error: None,
        ..Reply::default()
    });
    assert_eq!(
        exchange(&path, b"{\"command\":\"status\"}\n").await["available"],
        true
    );
    task.abort();
    let _ = task.await;
    assert!(!path.exists());
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Server::bind(&dir.0, &dir.0.join("shell-command.sock")).is_err());
}

#[tokio::test]
async fn duplicate_requests_are_rejected_while_first_is_owned_by_coordinator() {
    let shared = Shared::new();
    shared.update(Reply {
        ok: true,
        available: true,
        busy: false,
        error: None,
        ..Reply::default()
    });
    let (sender, mut requests) = mpsc::channel(1);
    let first = tokio::spawn(submit(shared.clone(), sender.clone(), Action::Suspend));
    let request = requests.recv().await.unwrap();
    let duplicate = submit(shared.clone(), sender, Action::Suspend).await;
    assert!(!duplicate.ok && duplicate.busy && !duplicate.available);
    request.finish(Err("injected logind refusal".into()));
    let reply = first.await.unwrap();
    assert!(!reply.ok);
    assert_eq!(reply.error.as_deref(), Some("injected logind refusal"));
    assert!(!shared.snapshot().busy);
}

#[tokio::test]
async fn all_power_commands_share_one_inflight_gate_and_keep_action_specific_errors() {
    let dir = Directory::new();
    let path = dir.0.join("power-control.sock");
    let (server, shared, mut requests) =
        Server::bind(&dir.0, &dir.0.join("shell-command.sock")).unwrap();
    shared.update(Reply {
        ok: true,
        available: false,
        reboot_available: true,
        power_off_available: false,
        error: Some("Suspend unsupported".into()),
        power_off_error: Some("Turn off requires administrator authorization".into()),
        ..Reply::default()
    });
    let server = tokio::spawn(server.run());
    let denied = exchange(&path, b"{\"command\":\"power_off\"}\n").await;
    assert_eq!(denied["ok"], false);
    assert_eq!(
        denied["error"],
        "Turn off requires administrator authorization"
    );
    assert!(requests.try_recv().is_err());
    let first = tokio::spawn({
        let path = path.clone();
        async move { exchange(&path, b"{\"command\":\"reboot\"}\n").await }
    });
    let request = requests.recv().await.unwrap();
    assert_eq!(request.action, Action::Reboot);
    for command in ["suspend", "reboot", "power_off"] {
        let reply = exchange(&path, format!("{{\"command\":\"{command}\"}}\n").as_bytes()).await;
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["busy"], true);
        assert_eq!(reply["available"], false);
        assert_eq!(reply["reboot_available"], false);
        assert_eq!(reply["power_off_available"], false);
    }
    request.finish(Ok(()));
    assert_eq!(first.await.unwrap()["ok"], true);
    assert!(requests.try_recv().is_err());
    assert!(!shared.snapshot().busy);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn slow_clients_do_not_stall_status_or_keep_socket_alive() {
    let dir = Directory::new();
    let path = dir.0.join("power-control.sock");
    let (server, _, _) = Server::bind(&dir.0, &dir.0.join("shell-command.sock")).unwrap();
    let task = tokio::spawn(server.run());
    let mut slow = UnixStream::connect(&path).await.unwrap();
    slow.write_all(b"{").await.unwrap();
    assert_eq!(
        exchange(&path, b"{\"command\":\"status\"}\n").await["ok"],
        true
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), slow.read_u8())
            .await
            .unwrap()
            .unwrap_err()
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
    task.abort();
    let _ = task.await;
    assert!(!path.exists());
}
