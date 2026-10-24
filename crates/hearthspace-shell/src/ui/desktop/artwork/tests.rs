use super::*;
use crate::desktop_items::DesktopItem;
use hearthspace_desktop::artwork::RasterImage;
use std::{path::PathBuf, sync::Arc};

fn item(id: &str) -> DesktopItem {
    DesktopItem {
        id: id.into(),
        title: "song.flac".into(),
        kind: DesktopItemKind::File {
            path: PathBuf::from(format!("/unused/{id}.flac")),
        },
    }
}

fn pixels(id: &str, color: [u8; 4]) -> Images {
    BTreeMap::from([(
        id.into(),
        Arc::new(RasterImage {
            width: 1,
            height: 1,
            rgba: color.to_vec(),
        }),
    )])
}

#[test]
fn artwork_reload_preserves_surface_identity_and_unchanged_image_brushes() {
    let mut state = super::super::State::default();
    state.replace_items(vec![item("song")]);
    let window = state.items[0].window_id;
    let requested = state.artwork.requested.clone();
    apply(
        &mut state,
        requested.clone(),
        pixels("song", [220, 20, 30, 255]),
    );
    let brush = state.artwork.images["song"].clone();
    apply(
        &mut state,
        requested.clone(),
        pixels("song", [220, 20, 30, 255]),
    );
    assert!(state.artwork.images["song"] == brush);
    apply(&mut state, requested, pixels("song", [20, 30, 220, 255]));
    assert!(state.artwork.images["song"] != brush);
    assert_eq!(state.items[0].window_id, window);
}

#[test]
fn late_artwork_cannot_restore_removed_items_or_replace_current_results() {
    let mut state = super::super::State::default();
    state.replace_items(vec![item("old")]);
    let previous = state.artwork.requested.clone();
    state.replace_items(vec![item("new")]);
    let current = state.artwork.requested.clone();
    apply(&mut state, current, pixels("new", [20, 30, 220, 255]));
    apply(&mut state, previous, pixels("old", [220, 20, 30, 255]));
    assert!(state.artwork.images.contains_key("new"));
    assert!(!state.artwork.images.contains_key("old"));
    state.replace_items(vec![]);
    assert!(state.artwork.images.is_empty());
    assert!(state.artwork.pixels.is_empty());
}

#[test]
fn coalesced_removal_and_restoration_republishes_identical_cached_artwork() {
    let mut state = super::super::State::default();
    state.replace_items(vec![item("song")]);
    let (sender, mut receiver) = watch::channel(state.artwork.requested.clone());
    state.artwork.requests = Some(sender);
    let cached = pixels("song", [220, 20, 30, 255]);
    let mut previous = None;
    let request = receiver.borrow_and_update().clone();
    let (request, images) = publication(&mut previous, request, cached.clone()).unwrap();
    apply(&mut state, request.clone(), images);
    assert!(state.artwork.images.contains_key("song"));

    // The worker starts another poll, then both directory changes arrive before
    // it can consume the watch channel. Its cache never observes the empty list.
    let in_flight = receiver.borrow_and_update().clone();
    state.replace_items(vec![]);
    assert!(state.artwork.images.is_empty());
    state.replace_items(vec![item("song")]);
    assert!(state.artwork.images.is_empty());
    assert!(receiver.has_changed().unwrap());

    // Even a completion already queued for the original request is stale when
    // the same path is restored; it cannot refill the UI before the new pass.
    apply(&mut state, in_flight, cached.clone());
    assert!(state.artwork.images.is_empty());
    let restored = receiver.borrow_and_update().clone();
    assert!(restored.files == request.files);
    assert_ne!(restored.revision, request.revision);
    let (restored, images) = publication(&mut previous, restored, cached.clone())
        .expect("a new request revision republishes unchanged cached pixels");
    apply(&mut state, restored.clone(), images);
    assert!(state.artwork.images.contains_key("song"));
    assert_eq!(state.artwork.pixels, cached);
    assert!(
        publication(&mut previous, restored, cached).is_none(),
        "unchanged polls within the same revision still avoid a repaint"
    );
}
