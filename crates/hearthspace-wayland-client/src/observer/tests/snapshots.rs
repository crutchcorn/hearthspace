use super::support::{Peer, bounded};
use crate::watch_outputs;

#[tokio::test]
async fn current_mode_scale_and_description_publish_together_at_done() {
    let (connection, mut peer) = Peer::pair();
    let (sender, mut snapshots) = tokio::sync::mpsc::unbounded_channel();
    let watcher = tokio::spawn(watch_outputs(connection, move |outputs| {
        sender.send(outputs).map_err(std::io::Error::other)
    }));
    let output = peer.bootstrap(&[(11, "DP-1")]).await[0];
    let initial = bounded(snapshots.recv()).await.unwrap();
    assert_eq!(initial[0].name.as_deref(), Some("DP-1"));
    assert_eq!(initial[0].size, None);

    peer.mode(output, 1, 2560, 1440, 59_940).await;
    peer.mode(output, 2, 3840, 2160, 144_000).await; // preferred, not current
    peer.scale(output, 2).await;
    peer.geometry(output, -1280, 200).await;
    peer.description(output, "Desk #1").await;
    // A later registry bind proves all pending mode bytes were consumed.
    peer.advertise(22).await;
    let second = peer.bind(22).await;
    assert!(
        snapshots.try_recv().is_err(),
        "partial state escaped before done"
    );
    peer.done(output).await;
    let updated = bounded(snapshots.recv()).await.unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].size, Some((2560, 1440)));
    assert_eq!(updated[0].scale, 2);
    assert_eq!(updated[0].refresh_millihertz, Some(59_940));
    assert_eq!(updated[0].description.as_deref(), Some("Desk #1"));
    assert_eq!(updated[0].position, Some((-1280, 200)));
    assert_eq!(updated[0].global_name, initial[0].global_name);

    peer.mode(second, 1, 1920, 1080, 0).await;
    peer.name(second, "HDMI-A-1").await;
    let added = bounded(snapshots.recv()).await.unwrap();
    assert_eq!(added.len(), 2);
    assert_eq!(added[0], updated[0]);
    assert_eq!(added[1].refresh_millihertz, None);
    peer.remove(11).await;
    let removed = bounded(snapshots.recv()).await.unwrap();
    assert_eq!(removed, [added[1].clone()]);
    watcher.abort();
    assert!(bounded(watcher).await.unwrap_err().is_cancelled());
    peer.expect_eof().await;
}

#[tokio::test]
async fn duplicate_mode_batches_do_not_publish_or_change_connector_identity() {
    let (connection, mut peer) = Peer::pair();
    let (sender, mut snapshots) = tokio::sync::mpsc::unbounded_channel();
    let watcher = tokio::spawn(watch_outputs(connection, move |outputs| {
        sender.send(outputs).map_err(std::io::Error::other)
    }));
    let output = peer.bootstrap(&[(11, "eDP-1")]).await[0];
    bounded(snapshots.recv()).await.unwrap();
    peer.mode(output, 1, 2256, 1504, 60_000).await;
    peer.done(output).await;
    let original = bounded(snapshots.recv()).await.unwrap();
    peer.mode(output, 1, 2256, 1504, 60_000).await;
    peer.done(output).await;
    peer.advertise(22).await;
    peer.bind(22).await;
    assert!(snapshots.try_recv().is_err());
    peer.mode(output, 1, 1920, 1080, 60_000).await;
    peer.done(output).await;
    let changed = bounded(snapshots.recv()).await.unwrap();
    assert_eq!(changed[0].connector(), original[0].connector());
    assert_eq!(changed[0].global_name, original[0].global_name);
    assert_eq!(changed[0].size, Some((1920, 1080)));
    watcher.abort();
    assert!(bounded(watcher).await.unwrap_err().is_cancelled());
}
