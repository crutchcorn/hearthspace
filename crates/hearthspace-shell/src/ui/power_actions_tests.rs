use super::*;
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
};

#[test]
fn old_suspend_status_replies_leave_new_actions_unavailable() {
    let reply: Reply =
        serde_json::from_str(r#"{"ok":true,"available":true,"busy":false,"error":null}"#).unwrap();
    assert!(reply.status.available);
    assert!(!reply.status.reboot_available && !reply.status.power_off_available);
    assert_eq!(reply.status.reboot_error, None);
    assert_eq!(reply.status.power_off_error, None);
}

#[test]
fn shutdown_transport_uses_private_commands_once_and_preserves_logind_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("power-control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let worker = thread::spawn(move || {
        for command in ["reboot", "power_off"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut received = String::new();
            stream.read_to_string(&mut received).unwrap();
            assert_eq!(received, format!("{{\"command\":\"{command}\"}}\n"));
            let reply = if command == "reboot" {
                r#"{"ok":true,"available":false,"reboot_available":true,"power_off_available":true,"busy":true,"error":null}"#
            } else {
                r#"{"ok":false,"available":false,"busy":false,"error":"Shutdown is blocked by Editor: Unsaved work"}"#
            };
            writeln!(stream, "{reply}").unwrap();
        }
    });
    let status = request(&socket, "reboot").unwrap();
    assert!(!status.available && status.reboot_available && status.power_off_available);
    assert_eq!(
        request(&socket, "power_off").unwrap_err(),
        "Shutdown is blocked by Editor: Unsaved work"
    );
    worker.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_socket_io_yields_the_ui_executor_and_does_not_retry() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("power-control.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let serving = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut command = String::new();
        stream.read_to_string(&mut command).await.unwrap();
        assert_eq!(command, "{\"command\":\"power_off\"}\n");
        // The very same single-thread executor must remain free to serve I/O
        // while the shell worker is waiting for this delayed response.
        tokio::time::sleep(Duration::from_millis(20)).await;
        stream.write_all(b"{\"ok\":false,\"available\":false,\"busy\":false,\"error\":\"Permission denied\"}\n").await.unwrap();
        drop(stream);
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(query(socket, "power_off"), serving)
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err(), "Permission denied");
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}
