//! Immutable cursor artwork for the presentation thread. No Wayland resources or
//! renderer objects cross the thread boundary, and motion never re-reads pixels.
use std::sync::Mutex;

use smithay::{
    backend::renderer::{
        element::{Id, memory::MemoryBuffer},
        gles::GlesRenderer,
    },
    input::pointer::CursorImageAttributes,
    utils::{Logical, Physical, Point, Rectangle},
    wayland::compositor::with_states,
};
use wayland_server::{Resource, backend::ObjectId};

use crate::compositor::{
    App,
    cursor::{CursorIcon, cursor_image_for_icon},
    output::OutputRenderView,
};

mod pixels;
#[cfg(test)]
mod tests;
mod tree;

type SnapshotResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug)]
pub(super) struct CursorSnapshot {
    pub(super) image_id: Id,
    pub(super) revision: u64,
    /// Tightly packed, native-endian, premultiplied ARGB8888.
    pub(super) image: MemoryBuffer,
    /// Final output-local physical top-left, including hotspot and tree bounds.
    pub(super) location: Point<i32, Physical>,
}

#[derive(PartialEq)]
enum ImageKey {
    Named(CursorIcon, i32),
    Surface(ObjectId, i32, Vec<tree::SurfaceKey>),
}

struct CachedImage {
    key: ImageKey,
    image: MemoryBuffer,
    /// Tree bounding origin, or negative named-cursor hotspot, in physical pixels.
    offset: Point<i32, Physical>,
}

pub(super) struct SnapshotBuilder {
    image_id: Id,
    revision: u64,
    cached: Option<CachedImage>,
}

impl Default for SnapshotBuilder {
    fn default() -> Self {
        Self {
            image_id: Id::new(),
            revision: 0,
            cached: None,
        }
    }
}

impl SnapshotBuilder {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn update(
        &mut self,
        app: &mut App,
        renderer: &mut GlesRenderer,
        view: &OutputRenderView,
    ) -> SnapshotResult<Option<CursorSnapshot>> {
        // This is deliberately independent of software_cursor_visible: scenes
        // remain cursor-free even while this method renders complex artwork.
        if !app.cursor_visible
            || !Rectangle::new(view.location, view.size)
                .to_f64()
                .contains(app.pointer_location)
        {
            return Ok(None);
        }
        if view.scale <= 0 {
            return Err("cursor output scale must be positive".into());
        }
        let pointer =
            (app.pointer_location - view.location.to_f64()).to_physical(f64::from(view.scale));
        let mut hotspot = Point::<i32, Logical>::from((0, 0));
        let surface = app
            .cursor_surface
            .clone()
            .filter(|surface| surface.is_alive());
        if let Some(surface) = surface {
            hotspot = with_states(&surface, |states| {
                states
                    .data_map
                    .get::<Mutex<CursorImageAttributes>>()
                    .map(|attributes| attributes.lock().unwrap().hotspot)
                    .unwrap_or_default()
            });
            let tree = tree::SurfaceTree::collect(&surface, view.scale)?;
            if tree.entries.is_empty() {
                return Ok(None);
            }
            let key = ImageKey::Surface(surface.id(), view.scale, tree.keys());
            if self.cached.as_ref().is_none_or(|cached| cached.key != key) {
                // Publish only a complete, successful snapshot. Failures retain
                // the old cache key so the next dispatch retries this image.
                let image = tree.image(renderer, view.scale)?;
                self.replace(CachedImage {
                    key,
                    image,
                    offset: tree.bounds.loc,
                });
            }
        } else {
            // Match the existing named fallback for a destroyed cursor surface.
            app.cursor_surface = None;
            let key = ImageKey::Named(app.cursor_icon, view.scale);
            if self.cached.as_ref().is_none_or(|cached| cached.key != key) {
                let source = cursor_image_for_icon(app.cursor_icon);
                let (image, offset) = pixels::named(source, view.scale)?;
                self.replace(CachedImage { key, image, offset });
            }
        }
        let cached = self
            .cached
            .as_ref()
            .expect("successful cursor image construction");
        let location = pixels::position(
            pointer,
            hotspot.to_f64().to_physical(f64::from(view.scale)),
            cached.offset,
        )?;
        Ok(Some(CursorSnapshot {
            image_id: self.image_id.clone(),
            revision: self.revision,
            image: cached.image.clone(),
            location,
        }))
    }

    fn replace(&mut self, cached: CachedImage) {
        // A new identity on wrap prevents an ancient image revision aliasing.
        if self.revision == u64::MAX {
            self.image_id = Id::new();
            self.revision = 0;
        }
        self.revision += 1;
        self.cached = Some(cached);
    }
}
