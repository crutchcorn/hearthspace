use super::*;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
use tokio::net::UnixListener;

static ID: AtomicU64 = AtomicU64::new(0);

async fn reply(response: Vec<u8>, trickle: bool) -> io::Result<Value> {
    let path = std::env::temp_dir().join(format!(
        "hearthspace-power-control-{}-{}.sock",
        std::process::id(),
        ID.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut peer, _) = listener.accept().await.unwrap();
        let mut request = [0; 7];
        peer.read_exact(&mut request).await.unwrap();
        assert_eq!(&request, b"status\n");
        if trickle {
            for byte in response {
                if peer.write_all(&[byte]).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } else {
            let _ = peer.write_all(&response).await;
        }
    });
    let result = command(&path, "status").await;
    server.abort();
    let _ = server.await;
    std::fs::remove_file(path).unwrap();
    result
}

#[tokio::test(flavor = "current_thread")]
async fn presented_lock_requires_both_flags_in_a_complete_reply() {
    let payload = br#"{"locked":true,"lock_confirmed":true}"#;
    let bytes = [format!("ok {}\n", payload.len()).as_bytes(), payload].concat();
    assert!(confirmed(&reply(bytes, false).await.unwrap()));
    for value in [
        serde_json::json!({"locked":true}),
        serde_json::json!({"lock_confirmed":true}),
        Value::Null,
    ] {
        assert!(!confirmed(&value));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_oversized_and_truncated_control_responses_fail_closed() {
    for bytes in [
        b"err no locker\n".to_vec(),
        b"ok 999999999\n".to_vec(),
        b"ok 10\n{}".to_vec(),
        vec![b'x'; 2049],
        b"ok 1\nx".to_vec(),
    ] {
        assert!(reply(bytes, false).await.is_err());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn trickling_server_cannot_extend_the_absolute_deadline() {
    let started = Instant::now();
    let error = reply(vec![b'x'; 100], true).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(2));
}
