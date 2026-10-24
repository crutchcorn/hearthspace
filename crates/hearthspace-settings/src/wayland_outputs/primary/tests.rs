use super::*;
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
};

fn output(name: &str) -> OutputInfo {
    OutputInfo {
        global_name: 1,
        name: Some(name.into()),
        description: None,
        position: None,
        size: Some((1920, 1080)),
        refresh_millihertz: Some(60_000),
        scale: 1,
    }
}

#[test]
fn primary_recovers_when_endpoint_appears_without_another_output_event() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("control.sock");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (send, receive) = watch::channel(None);
        let (published, mut snapshots) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(deliver(receive, Ok(socket.clone()), move |snapshot| {
            published.send(snapshot).is_ok()
        }));
        send.send(Some(vec![output("DP-1")])).unwrap();
        let initial = tokio::time::timeout(Duration::from_secs(2), snapshots.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(initial.primary, None);
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut command = [0; 15];
            stream.read_exact(&mut command).unwrap();
            assert_eq!(&command, b"output-primary\n");
            let body = r#"{"version":1,"primary":"DP-1"}"#;
            write!(stream, "ok {}\n{body}", body.len()).unwrap();
        });
        let recovered = tokio::time::timeout(Duration::from_secs(3), snapshots.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovered.outputs, initial.outputs);
        assert_eq!(recovered.primary.as_deref(), Some("DP-1"));
        task.abort();
        server.join().unwrap();
    });
}

#[test]
fn metadata_failure_keeps_connected_primary_but_valid_none_and_disconnect_clear_it() {
    let outputs = vec![output("eDP-1"), output("DP-1")];
    let mut primary = Primary::default();
    assert!(primary.update(Ok(Some("DP-1".into())), &outputs));
    assert!(!primary.update(Err(io::ErrorKind::TimedOut.into()), &outputs));
    assert_eq!(primary.known.as_deref(), Some("DP-1"));
    assert!(primary.update(Ok(None), &outputs));
    assert_eq!(primary.known, None);
    primary.update(Ok(Some("DP-1".into())), &outputs);
    primary.update(
        Err(io::ErrorKind::ConnectionRefused.into()),
        &[output("eDP-1")],
    );
    assert_eq!(primary.known, None);
}
