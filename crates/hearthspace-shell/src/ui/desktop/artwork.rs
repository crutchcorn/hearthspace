//! Artwork has its own worker: media parsing cannot delay desktop actions.
use std::{collections::BTreeMap, time::Duration};

use tokio::sync::watch;
use xilem::{
    core::{MessageProxy, NoElement, View},
    masonry::peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat},
    view::task_raw,
};

use super::ShellState;
use crate::desktop_items::{
    DesktopItemKind,
    artwork::{Cache, Files, Images},
};

#[derive(Default)]
pub(super) struct State {
    pub(super) images: BTreeMap<String, ImageBrush>,
    pixels: Images,
    requested: Request,
    requests: Option<watch::Sender<Request>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    revision: u64,
    files: Files,
}

fn files(state: &super::State) -> Files {
    state
        .items
        .iter()
        .filter_map(|item| match &item.content.kind {
            DesktopItemKind::File { path } => Some((item.content.id.clone(), path.clone())),
            _ => None,
        })
        .collect()
}

pub(super) fn refresh(state: &mut super::State) {
    let files = files(state);
    state.artwork.images.retain(|id, _| files.contains_key(id));
    state.artwork.pixels.retain(|id, _| files.contains_key(id));
    if state.artwork.requested.files == files {
        return;
    }
    state.artwork.requested = Request {
        revision: state.artwork.requested.revision.wrapping_add(1),
        files,
    };
    if let Some(sender) = &state.artwork.requests {
        sender.send_replace(state.artwork.requested.clone());
    }
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<(Request, Images)>, state: &mut ShellState| {
            let (sender, mut receiver) = watch::channel(state.desktop.artwork.requested.clone());
            state.desktop.artwork.requests = Some(sender);
            async move {
                let mut cache = Cache::default();
                let mut previous = None;
                let mut poll = tokio::time::interval(Duration::from_secs(2));
                poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        changed = receiver.changed() => {
                            if changed.is_err() { break; }
                        }
                        _ = poll.tick() => {}
                    }
                    let request = receiver.borrow_and_update().clone();
                    let worker_files = request.files.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let images = cache.refresh(&worker_files);
                        (cache, images)
                    })
                    .await;
                    let Ok((next_cache, images)) = result else {
                        break;
                    };
                    cache = next_cache;
                    // A directory change arrived during decoding. Publish only
                    // the next pass for that request, never obsolete artwork.
                    if receiver.has_changed().unwrap_or(true) {
                        continue;
                    }
                    if let Some(update) = publication(&mut previous, request, images)
                        && proxy.message(update).is_err()
                    {
                        break;
                    }
                }
            }
        },
        |state, (requested, images)| apply(&mut state.desktop, requested, images),
    )
}

fn publication(
    previous: &mut Option<(u64, Images)>,
    request: Request,
    images: Images,
) -> Option<(Request, Images)> {
    // Watch requests can coalesce a removal and restoration into the original
    // file list. The UI has already dropped that artwork, so each revision must
    // publish once even when the worker still has identical cached pixels.
    if previous
        .as_ref()
        .is_some_and(|(revision, pixels)| *revision == request.revision && *pixels == images)
    {
        return None;
    }
    *previous = Some((request.revision, images.clone()));
    Some((request, images))
}

fn apply(state: &mut super::State, requested: Request, images: Images) {
    if requested != state.artwork.requested {
        return;
    }
    state.artwork.images.retain(|id, _| images.contains_key(id));
    for (id, pixmap) in &images {
        if state.artwork.pixels.get(id) == Some(pixmap) {
            continue;
        }
        let image: ImageBrush = ImageData {
            data: pixmap.rgba.clone().into(),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: pixmap.width,
            height: pixmap.height,
        }
        .into();
        state.artwork.images.insert(id.clone(), image);
    }
    state.artwork.pixels = images;
}

#[cfg(test)]
mod tests;
