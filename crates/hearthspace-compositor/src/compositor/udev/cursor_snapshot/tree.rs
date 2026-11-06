use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Color32F, Offscreen,
            damage::OutputDamageTracker,
            element::{Kind, memory::MemoryBuffer, surface::WaylandSurfaceRenderElement},
            gles::{GlesRenderbuffer, GlesRenderer},
            utils::{Buffer, CommitCounter, RendererSurfaceStateUserData, SurfaceView},
        },
    },
    utils::{Logical, Physical, Point, Rectangle, Transform},
    wayland::{
        alpha_modifier::AlphaModifierSurfaceCachedState,
        compositor::{TraversalAction, with_states, with_surface_tree_downward},
    },
};
use wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface};

use super::{SnapshotResult, pixels};
use crate::compositor::capture::read_pixels;

#[derive(Clone, PartialEq)]
pub(super) struct SurfaceKey {
    surface: ObjectId,
    buffer: ObjectId,
    commit: CommitCounter,
    view: SurfaceView,
    scale: i32,
    transform: Transform,
    alpha: f32,
    location: Point<f64, Physical>,
}

pub(super) struct Entry {
    surface: WlSurface,
    buffer: Buffer,
    key: SurfaceKey,
}

pub(super) struct SurfaceTree {
    pub(super) entries: Vec<Entry>,
    pub(super) bounds: Rectangle<i32, Physical>,
}

impl SurfaceTree {
    pub(super) fn collect(surface: &WlSurface, scale: i32) -> SnapshotResult<Self> {
        let mut entries = Vec::new();
        let mut unknown_buffer = false;
        with_surface_tree_downward(
            surface,
            Point::<f64, Physical>::from((0., 0.)),
            |_, states, location| {
                let view = states
                    .data_map
                    .get::<RendererSurfaceStateUserData>()
                    .and_then(|data| data.lock().unwrap().view());
                match view {
                    Some(view) => TraversalAction::DoChildren(
                        *location + view.offset.to_f64().to_physical(f64::from(scale)),
                    ),
                    None => TraversalAction::SkipChildren,
                }
            },
            |surface, states, location| {
                let Some(data) = states.data_map.get::<RendererSurfaceStateUserData>() else {
                    return;
                };
                let data = data.lock().unwrap();
                if data.buffer().is_some() && data.view().is_none() {
                    unknown_buffer = true;
                    return;
                }
                let (Some(view), Some(buffer)) = (data.view(), data.buffer()) else {
                    return;
                };
                let alpha = states
                    .cached_state
                    .get::<AlphaModifierSurfaceCachedState>()
                    .current()
                    .multiplier_f32()
                    .unwrap_or(1.);
                entries.push(Entry {
                    surface: surface.clone(),
                    buffer: buffer.clone(),
                    key: SurfaceKey {
                        surface: surface.id(),
                        buffer: buffer.id(),
                        commit: data.current_commit(),
                        view,
                        scale: data.buffer_scale(),
                        transform: data.buffer_transform(),
                        alpha,
                        location: *location + view.offset.to_f64().to_physical(f64::from(scale)),
                    },
                });
            },
            |_, _, _| true,
        );
        if unknown_buffer {
            return Err("committed cursor buffer has unrecognized dimensions".into());
        }
        let mut bounds = None;
        for entry in &entries {
            let key = &entry.key;
            let origin = pixels::position(key.location, (0., 0.).into(), (0, 0).into())?;
            let width = key
                .view
                .dst
                .w
                .checked_mul(scale)
                .ok_or("cursor tree width overflow")?;
            let height = key
                .view
                .dst
                .h
                .checked_mul(scale)
                .ok_or("cursor tree height overflow")?;
            if width <= 0 || height <= 0 {
                return Err("cursor tree has invalid surface size".into());
            }
            let right = origin
                .x
                .checked_add(width)
                .ok_or("cursor tree coordinate overflow")?;
            let bottom = origin
                .y
                .checked_add(height)
                .ok_or("cursor tree coordinate overflow")?;
            bounds = Some(match bounds {
                None => (origin.x, origin.y, right, bottom),
                Some((left, top, old_right, old_bottom)) => (
                    origin.x.min(left),
                    origin.y.min(top),
                    right.max(old_right),
                    bottom.max(old_bottom),
                ),
            });
        }
        let bounds = if let Some((left, top, right, bottom)) = bounds {
            let size = (
                right
                    .checked_sub(left)
                    .ok_or("cursor tree width overflow")?,
                bottom
                    .checked_sub(top)
                    .ok_or("cursor tree height overflow")?,
            )
                .into();
            pixels::dimensions(size)?;
            Rectangle::new((left, top).into(), size)
        } else {
            Rectangle::default()
        };
        Ok(Self { entries, bounds })
    }

    pub(super) fn keys(&self) -> Vec<SurfaceKey> {
        self.entries.iter().map(|entry| entry.key.clone()).collect()
    }

    pub(super) fn image(
        &self,
        renderer: &mut GlesRenderer,
        scale: i32,
    ) -> SnapshotResult<MemoryBuffer> {
        if let [entry] = self.entries.as_slice() {
            let key = &entry.key;
            // Admit only a 1:1 full-buffer copy. Smithay handles all cropping,
            // transformation, density conversion and alpha in the fallback.
            if key.transform == Transform::Normal
                && key.alpha == 1.
                && key.scale == scale
                && key.view.src == Rectangle::<f64, Logical>::from_size(key.view.dst.to_f64())
                && let Some(image) = pixels::shm(&entry.buffer, self.bounds.size)?
            {
                return Ok(image);
            }
        }
        let mut elements = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let element = with_states(&entry.surface, |states| {
                WaylandSurfaceRenderElement::from_surface(
                    renderer,
                    &entry.surface,
                    states,
                    entry.key.location - self.bounds.loc.to_f64(),
                    1.,
                    Kind::Cursor,
                )
            })?;
            elements.push(element.ok_or("committed cursor buffer could not be imported")?);
        }
        let size = self.bounds.size;
        let mut buffer: GlesRenderbuffer =
            renderer.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())?;
        let mut framebuffer = renderer.bind(&mut buffer)?;
        let mut damage = OutputDamageTracker::new(size, f64::from(scale), Transform::Normal);
        damage.render_output(
            renderer,
            &mut framebuffer,
            0,
            &elements,
            Color32F::TRANSPARENT,
        )?;
        // copy_framebuffer/map_texture synchronize this small image before its
        // immutable CPU storage is handed to another renderer/thread.
        pixels::rgba(read_pixels(renderer, &framebuffer, size, false)?, size)
    }
}
