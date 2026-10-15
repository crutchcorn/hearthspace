use super::*;

#[test]
fn scale_must_be_integer() {
    assert_eq!(scale_to_integer(None).unwrap(), None);
    assert_eq!(scale_to_integer(Some(1.0)).unwrap(), None);
    assert_eq!(scale_to_integer(Some(2.0)).unwrap(), Some(2));
    assert!(scale_to_integer(Some(1.5)).is_err());
}

#[test]
fn maps_common_keysyms_to_evdev() {
    assert_eq!(keysym_to_evdev(b'a' as u32).unwrap(), 30);
    assert_eq!(keysym_to_evdev(b'A' as u32).unwrap(), 30);
    assert_eq!(keysym_to_evdev(0xff0d).unwrap(), 28);
    assert!(keysym_to_evdev(0x2603).is_err());
}

#[test]
fn validates_resolution_shape() {
    assert!(validate_resolution("800x600").is_ok());
    assert!(validate_resolution("800X600").is_ok());
    assert!(validate_resolution("800").is_err());
    assert!(validate_resolution("0x600").is_err());
}

#[tokio::test]
async fn attached_stop_and_drop_preserve_external_session() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixListener,
    };

    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("custom-control.sock");
    let sentinel = directory.path().join("external-file");
    std::fs::write(&sentinel, "owned by session").unwrap();
    let listener = UnixListener::bind(&socket).unwrap();
    let mut attached = HearthspaceCompositor::attach(directory.path(), "wayland-native", &socket);
    assert_eq!(attached.ownership(), SessionOwnership::Attached);
    assert_eq!(attached.wayland_display(), "wayland-native");
    assert!(attached.start(Some("800x600"), None).await.is_err());

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 7];
        stream.read_exact(&mut request).await.unwrap();
        assert_eq!(&request, b"status\n");
        stream.write_all(b"ok 2\n{}").await.unwrap();
        // Detaching must not open another connection to send quit.
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    attached.start(None, None).await.unwrap();
    assert_eq!(attached.state().unwrap().command_socket_path(), socket);
    attached.stop().await.unwrap();
    assert!(attached.state().is_err());
    drop(attached);
    server.await.unwrap();
    assert!(sentinel.exists());
    assert!(socket.exists());
}

#[tokio::test]
async fn transport_reads_fragmented_payload_and_rejects_truncation() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixListener,
    };
    for complete in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("control.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut command = [0; 7];
            stream.read_exact(&mut command).await.unwrap();
            stream.write_all(b"ok 6\n").await.unwrap();
            for byte in if complete {
                b"abcdef".as_slice()
            } else {
                b"abc".as_slice()
            } {
                stream.write_all(&[*byte]).await.unwrap();
                tokio::task::yield_now().await;
            }
        });
        let response = send_text_command(&socket, "status").await;
        if complete {
            assert_eq!(response.unwrap(), b"abcdef");
        } else {
            assert!(response.is_err());
        }
        server.await.unwrap();
    }
}
