use super::*;
use xilem::tokio::io::AsyncWriteExt;

#[test]
fn a_second_launch_delivers_the_requested_page_to_the_existing_endpoint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.sock");
    let Startup::Primary(Some(endpoint)) = start_at(&path, SettingsCategory::Appearance).unwrap()
    else {
        panic!("first launch must own the listener")
    };
    let listener = endpoint.listener.try_clone().unwrap();
    let server = std::thread::spawn(move || {
        xilem::tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let listener = xilem::tokio::net::UnixListener::from_std(listener).unwrap();
                let (mut stream, _) = listener.accept().await.unwrap();
                let page = read_page(&mut stream).await.unwrap();
                stream.write_all(b"ok\n").await.unwrap();
                page
            })
    });
    assert!(matches!(
        start_at(&path, SettingsCategory::KeyboardShortcuts).unwrap(),
        Startup::Forwarded
    ));
    assert_eq!(server.join().unwrap(), SettingsCategory::KeyboardShortcuts);
    assert!(path.exists());
    drop(endpoint);
    assert!(!path.exists());
}

#[test]
fn stale_owned_socket_is_recovered_but_files_and_replacements_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.sock");
    drop(UnixListener::bind(&path).unwrap());
    let Startup::Primary(Some(endpoint)) = start_at(&path, SettingsCategory::Gestures).unwrap()
    else {
        panic!("closed stale socket must be recoverable")
    };
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"unrelated replacement").unwrap();
    drop(endpoint);
    assert_eq!(fs::read(&path).unwrap(), b"unrelated replacement");
    assert!(start_at(&path, SettingsCategory::Gestures).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"unrelated replacement");
}

#[test]
fn malformed_and_oversized_activation_frames_cannot_select_a_page() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for frame in [
            b"unknown\n".to_vec(),
            b"gestures\0\n".to_vec(),
            vec![b'a'; LIMIT],
        ] {
            let (mut client, mut server) = xilem::tokio::net::UnixStream::pair().unwrap();
            client.write_all(&frame).await.unwrap();
            assert!(read_page(&mut server).await.is_err());
        }
    });
}

#[test]
fn concurrent_stale_restarts_elect_one_primary_and_forward_every_other_launch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.sock");
    drop(UnixListener::bind(&path).unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
    let (sender, receiver) = std::sync::mpsc::channel();
    let threads: Vec<_> = (0..6)
        .map(|_| {
            let (path, barrier, sender) = (path.clone(), barrier.clone(), sender.clone());
            std::thread::spawn(move || {
                barrier.wait();
                match start_at(&path, SettingsCategory::Gestures).unwrap() {
                    Startup::Primary(Some(endpoint)) => {
                        sender.send(endpoint).unwrap();
                        true
                    }
                    Startup::Forwarded => false,
                    Startup::Primary(None) => panic!("explicit path must own a listener"),
                }
            })
        })
        .collect();
    drop(sender);
    let endpoint = receiver.recv_timeout(DEADLINE).unwrap();
    let listener = endpoint.listener.try_clone().unwrap();
    xilem::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let listener = xilem::tokio::net::UnixListener::from_std(listener).unwrap();
            for _ in 0..5 {
                let (mut stream, _) = timeout(DEADLINE, listener.accept()).await.unwrap().unwrap();
                assert_eq!(
                    read_page(&mut stream).await.unwrap(),
                    SettingsCategory::Gestures
                );
                stream.write_all(b"ok\n").await.unwrap();
            }
        });
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|primary| *primary)
            .count(),
        1
    );
    assert_eq!(socket_identity(&path).unwrap(), endpoint.identity);
}
