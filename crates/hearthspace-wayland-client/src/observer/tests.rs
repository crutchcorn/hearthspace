use std::{
    io::{self, Write},
    os::fd::AsFd,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
    time::Duration,
};

use support::{Peer, bounded, spawn_watcher};

mod snapshots;
mod support;

#[tokio::test]
async fn idle_connection_parks_instead_of_repeatedly_waking_itself() {
    let (connection, mut peer) = Peer::pair();
    let mut queue = connection.new_event_queue();
    let mut watcher = super::OutputWatcher::default();
    connection.display().sync(&queue.handle(), ());
    // SAFETY: this local registration is dropped before connection.
    let socket = unsafe { super::AsyncFd::register(connection.as_fd()) }.unwrap();
    let (dispatched, ()) = tokio::join!(
        super::dispatch_outputs(&mut queue, &socket, &mut watcher),
        async {
            let callback = peer.sync().await;
            peer.complete_sync(callback).await;
        },
    );
    dispatched.unwrap();
    assert!(watcher.sync_done);

    let wakes = Arc::new(AtomicUsize::new(0));
    tokio::select! {
        result = support::counted_wakes(
            super::dispatch_outputs(&mut queue, &socket, &mut watcher), wakes.clone()
        ) => panic!("idle peer unexpectedly produced events: {result:?}"),
        _ = async {
            for _ in 0..64 { tokio::task::yield_now().await; }
        } => {}
    }
    assert!(
        wakes.load(Ordering::Relaxed) < 16,
        "idle watcher kept scheduling itself"
    );
}

#[tokio::test]
async fn continuously_ready_connection_yields_to_other_tasks() {
    const LIMIT: usize = 1024;
    let (connection, mut peer) = Peer::pair();
    let mut writer = peer.writer();
    let output = Arc::new(AtomicU32::new(0));
    let sent = Arc::new(AtomicUsize::new(0));
    let (names_tx, mut names) = tokio::sync::mpsc::unbounded_channel();
    let (probe_tx, probe_rx) = tokio::sync::oneshot::channel();
    let task_output = output.clone();
    let task_sent = sent.clone();
    let mut probe_tx = Some(probe_tx);
    let task = tokio::spawn(super::watch_wayland_outputs(connection, move |names| {
        names_tx.send(names).map_err(io::Error::other)?;
        let output = task_output.load(Ordering::Relaxed);
        if output == 0 {
            return Ok(());
        }
        let count = task_sent.fetch_add(1, Ordering::Relaxed);
        if let Some(probe_tx) = probe_tx.take() {
            let sent = task_sent.clone();
            tokio::spawn(async move {
                let _ = probe_tx.send(sent.load(Ordering::Relaxed));
            });
        }
        // Refill the real socket before the watcher polls again. Each read
        // succeeds, so an EAGAIN-only yield cannot make the probe run early.
        if count < LIMIT {
            writer.write_all(&support::name_message(output, &format!("DP-{count}")))?;
        }
        Ok(())
    }));
    let id = peer.bootstrap(&[(11, "DP-initial")]).await[0];
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-initial"]);
    output.store(id, Ordering::Relaxed);
    peer.name(id, "DP-start").await;
    let observed = bounded(probe_rx).await.unwrap();
    task.abort();
    assert!(bounded(task).await.unwrap_err().is_cancelled());
    assert!(
        observed < LIMIT,
        "watcher drained the entire stream before yielding"
    );
}

#[tokio::test]
async fn split_event_is_delivered_after_its_remaining_bytes_arrive() {
    let (connection, mut peer) = Peer::pair();
    let fd = connection.as_fd().try_clone_to_owned().unwrap();
    let (task, mut names) = spawn_watcher(connection);
    let output = peer.bootstrap(&[(11, "DP-1")]).await[0];
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-1"]);
    let bytes = support::name_message(output, "DP-2");
    peer.write_bytes(&bytes[..4]).await;
    bounded(async {
        while rustix::io::ioctl_fionread(&fd).unwrap() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(names.try_recv().is_err());
    peer.write_bytes(&bytes[4..]).await;
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-2"]);
    task.abort();
    assert!(bounded(task).await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn disconnect_with_a_truncated_event_terminates_watcher() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    let output = peer.bootstrap(&[(11, "DP-1")]).await[0];
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-1"]);
    peer.write_bytes(&support::name_message(output, "DP-2")[..4])
        .await;
    drop(peer);
    assert!(bounded(task).await.unwrap().is_err());
}

#[tokio::test]
async fn initial_names_and_hotplug_preserve_order_and_suppress_duplicates() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    let outputs = peer.bootstrap(&[(11, "DP-1"), (22, "HDMI-A-1")]).await;
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-1", "HDMI-A-1"]);

    peer.name(outputs[0], "DP-1").await;
    peer.advertise(33).await;
    let added = peer.bind(33).await;
    // Observing this bind proves the preceding duplicate name was dispatched.
    assert!(names.try_recv().is_err());
    peer.name(added, "DP-3").await;
    assert_eq!(
        bounded(names.recv()).await.unwrap(),
        ["DP-1", "HDMI-A-1", "DP-3"]
    );
    peer.name(outputs[0], "DP-2").await;
    assert_eq!(
        bounded(names.recv()).await.unwrap(),
        ["DP-2", "HDMI-A-1", "DP-3"]
    );

    peer.remove(22).await;
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-2", "DP-3"]);
    peer.remove(11).await;
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-3"]);
    peer.remove(33).await;
    assert!(bounded(names.recv()).await.unwrap().is_empty());

    task.abort();
    assert!(bounded(task).await.unwrap_err().is_cancelled());
    peer.expect_eof().await;
}

#[tokio::test]
async fn first_bootstrap_barrier_is_cancellable() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    peer.registry().await;
    peer.sync().await;
    assert!(names.try_recv().is_err());

    task.abort();
    assert!(bounded(task).await.unwrap_err().is_cancelled());
    peer.expect_eof().await;
}

#[tokio::test]
async fn second_bootstrap_barrier_is_cancellable_before_initial_names() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    peer.registry().await;
    peer.advertise(11).await;
    let first = peer.sync().await;
    peer.complete_sync(first).await;
    let output = peer.bind(11).await;
    peer.name(output, "DP-1").await;
    peer.sync().await;
    peer.advertise(22).await;
    peer.bind(22).await;
    // This later bind proves the name and all prior server bytes were read;
    // the already-buffered second sync request alone cannot prove that.
    assert!(names.try_recv().is_err());

    task.abort();
    assert!(bounded(task).await.unwrap_err().is_cancelled());
    peer.expect_eof().await;
}

#[tokio::test]
async fn idle_watcher_cancellation_closes_connection() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    peer.bootstrap(&[]).await;
    assert!(bounded(names.recv()).await.unwrap().is_empty());

    task.abort();
    assert!(bounded(task).await.unwrap_err().is_cancelled());
    peer.expect_eof().await;
}

#[tokio::test]
async fn disconnect_during_bootstrap_terminates_watcher() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    peer.registry().await;
    peer.sync().await;
    drop(peer);

    assert!(bounded(task).await.unwrap().is_err());
    assert!(names.recv().await.is_none());
}

#[tokio::test]
async fn disconnect_while_idle_terminates_watcher() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    peer.bootstrap(&[]).await;
    assert!(bounded(names.recv()).await.unwrap().is_empty());
    drop(peer);

    assert!(bounded(task).await.unwrap().is_err());
}

#[tokio::test]
async fn message_delivery_failure_terminates_watcher() {
    let (connection, mut peer) = Peer::pair();
    let (task, mut names) = spawn_watcher(connection);
    let output = peer.bootstrap(&[(11, "DP-1")]).await[0];
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-1"]);
    drop(names);
    peer.name(output, "DP-2").await;

    let error = bounded(task).await.unwrap().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    peer.expect_eof().await;
}

#[tokio::test]
async fn shell_unwind_drops_runtime_with_idle_wayland_connection() {
    let (connection, mut peer) = Peer::pair();
    let (names_tx, mut names) = tokio::sync::mpsc::unbounded_channel();
    let (panic_tx, panic_rx) = tokio::sync::oneshot::channel();
    let (stopped_tx, stopped_rx) = tokio::sync::oneshot::channel();
    let thread = std::thread::spawn(move || {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                tokio::spawn(super::watch_wayland_outputs(connection, move |names| {
                    names_tx.send(names).map_err(io::Error::other)
                }));
                let _ = panic_rx.await;
                panic!("simulate shell panic while watcher is idle");
            });
            // Runtime unwinding must cancel the watcher without waiting for
            // compositor traffic or a blocking pool worker to finish.
        }))
        .is_err();
        let _ = stopped_tx.send(panicked);
    });
    peer.bootstrap(&[(11, "DP-1")]).await;
    assert_eq!(bounded(names.recv()).await.unwrap(), ["DP-1"]);
    panic_tx.send(()).unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(3), stopped_rx).await;
    // Release a regressed blocking reader before joining so test failure does
    // not itself leave an indefinitely blocked runtime worker behind.
    drop(peer);
    thread.join().unwrap();
    assert!(
        stopped
            .expect("runtime drop waited for Wayland traffic")
            .unwrap()
    );
}
