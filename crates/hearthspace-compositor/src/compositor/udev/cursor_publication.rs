//! Snapshot on the Wayland owner; publish immutable pixels to device presenters.
use super::{
    UdevBackendState,
    cursor_snapshot::SnapshotBuilder,
    presenter::{CursorUpdate, OutputKey},
};
use crate::compositor::App;
use smithay::{
    backend::renderer::element::Id,
    utils::{Physical, Point, Rectangle},
    wayland::compositor::{
        SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward,
    },
};
use wayland_server::{
    Resource,
    protocol::{wl_callback::WlCallback, wl_surface::WlSurface},
};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct CursorPublication {
    builder: SnapshotBuilder,
    sequence: u64,
    last: Option<PublishedCursor>,
    callbacks: Vec<PublishedCallbacks>,
}

struct PublishedCallbacks {
    surface: WlSurface,
    callbacks: Vec<WlCallback>,
}

#[derive(PartialEq)]
struct PublishedCursor {
    output: Option<OutputKey>,
    image: Option<(Id, u64)>,
    location: Option<Point<i32, Physical>>,
    surface: Option<WlSurface>,
}

impl CursorPublication {
    pub(super) fn reset(&mut self) {
        self.builder.reset();
        self.last = None;
        self.callbacks.clear();
    }

    /// Freeze callback ownership only after a successful image snapshot. Keep
    /// the originals in committed state so supersession, hiding and reset do
    /// not abandon requests that a later publication can still service.
    fn prepare(&mut self, published: PublishedCursor) -> Option<u64> {
        let unchanged = self.last.as_ref() == Some(&published);
        if unchanged && !self.callbacks.is_empty() {
            return None;
        }
        let callbacks = published
            .surface
            .as_ref()
            .map(published_callbacks)
            .unwrap_or_default();
        if unchanged && callbacks.is_empty() {
            return None;
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("cursor sequence exhausted");
        self.last = Some(published);
        self.callbacks = callbacks;
        Some(self.sequence)
    }

    pub(super) fn callbacks(
        &mut self,
        state: &App,
        key: OutputKey,
        image: &Option<(Id, u64)>,
        time: u32,
    ) -> usize {
        if !self.last.as_ref().is_some_and(|last| {
            last.output == Some(key) && &last.image == image && last.surface == state.cursor_surface
        }) {
            return 0;
        }
        let mut sent = 0;
        for batch in self.callbacks.drain(..) {
            if !batch.surface.is_alive() {
                continue;
            }
            with_states(&batch.surface, |states| {
                states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .frame_callbacks
                    .retain(|callback| {
                        if !batch.callbacks.contains(callback) {
                            return true;
                        }
                        // Another dispatcher may already have serviced a
                        // resource. New commits are never part of this batch.
                        if callback.is_alive() {
                            callback.done(time);
                            sent += 1;
                        }
                        false
                    });
            });
        }
        sent
    }
}

impl UdevBackendState {
    pub(super) fn sync_security(&mut self, state: &App) {
        let generation = state.lock_frame_generation();
        if self.lock_generation == generation {
            return;
        }
        self.lock_generation = generation;
        self.security_epoch = self
            .security_epoch
            .checked_add(1)
            .expect("security epoch exhausted");
        self.cursor_publication.reset();
        for device in self.opened_devices.values_mut() {
            device
                .presenter
                .set_security(self.security_epoch, generation);
            for output in &mut device.output_surfaces {
                output.presentation.invalidate();
                output.capture = None;
            }
        }
    }

    pub(in crate::compositor) fn publish_cursor(&mut self, state: &mut App) {
        // Native scene images never contain cursor pixels, even when a plane
        // rejects an image. The worker composites fallback over its cached scene.
        state.software_cursor_visible = false;
        self.sync_security(state);
        if self.drm_commits_paused {
            return;
        }
        let owner = self
            .opened_devices
            .values()
            .filter(|d| d.active)
            .find_map(|device| {
                device.output_surfaces.iter().find_map(|output| {
                    let view = state.output_render_view(output.target.connector_name())?;
                    Rectangle::new(view.location, view.size)
                        .contains(state.pointer_location.to_i32_floor())
                        .then_some((output.key, view))
                })
            });
        let snapshot = if let Some((_, view)) = owner.as_ref() {
            let Some(node) = self.gpus.scene_node else {
                return;
            };
            let result = self
                .gpus
                .manager
                .single_renderer(&node)
                .map_err(|e| e.to_string())
                .and_then(|mut renderer| {
                    self.cursor_publication
                        .builder
                        .update(state, renderer.as_mut(), view)
                        .map_err(|e| e.to_string())
                });
            match result {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    tracing::warn!(%error, "cursor snapshot unavailable; retaining prior presentation");
                    return;
                }
            }
        } else {
            None
        };
        let output = owner.as_ref().map(|(key, _)| *key);
        let published = PublishedCursor {
            output,
            image: snapshot.as_ref().map(|s| (s.image_id.clone(), s.revision)),
            location: snapshot.as_ref().map(|s| s.location),
            surface: state.cursor_surface.clone(),
        };
        let Some(sequence) = self.cursor_publication.prepare(published) else {
            return;
        };
        tracing::trace!(
            cursor_sequence = sequence,
            security_epoch = self.security_epoch,
            ?output,
            cursor_image = ?snapshot.as_ref().map(|image| (&image.image_id, image.revision)),
            location = ?snapshot.as_ref().map(|image| image.location),
            cursor_source = if snapshot.is_none() { "hidden" } else if state.cursor_surface.is_some() { "client" } else { "named" },
            "published native cursor snapshot"
        );
        let update = CursorUpdate {
            sequence,
            security_epoch: self.security_epoch,
            output,
            snapshot,
        };
        for device in self.opened_devices.values().filter(|d| d.active) {
            device.presenter.update_cursor(update.clone());
        }
    }
}

fn published_callbacks(surface: &WlSurface) -> Vec<PublishedCallbacks> {
    let mut callbacks = Vec::new();
    if !surface.is_alive() {
        return callbacks;
    }
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |surface, states, &()| {
            let pending = states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .clone();
            if !pending.is_empty() {
                callbacks.push(PublishedCallbacks {
                    surface: surface.clone(),
                    callbacks: pending,
                });
            }
        },
        |_, _, &()| true,
    );
    callbacks
}
