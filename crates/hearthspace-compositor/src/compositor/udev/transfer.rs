//! Compose every scene on the canonical GLES context, transferring only the
//! completed image. Target-local GBM buffers remain owned by their KMS device.
mod buffer_age;
pub(super) mod cache;
mod formats;
use super::{
    device::KmsOutputSurface,
    gpu::{GpuApi, GpuState},
};
use crate::compositor::{App, output::OutputRenderView};
use smithay::{
    backend::{
        allocator::{Fourcc, dmabuf::Dmabuf},
        drm::DrmNode,
        renderer::{
            Bind, Color32F, ContextId, Frame, Offscreen, Renderer, Texture,
            damage::OutputDamageTracker,
            gles::{GlesRenderer, GlesTexture},
            multigpu::MultiTexture,
            sync::SyncPoint,
        },
    },
    utils::{Buffer, Physical, Rectangle, Size, Transform},
};
use std::time::Instant;
use tracing::{info, trace, warn};

use formats::{direct_copy_format, transfer_format};

pub(super) struct SceneBuffer {
    texture: GlesTexture,
    context: ContextId<GlesTexture>,
    target_context: ContextId<GlesTexture>,
    nodes: (DrmNode, DrmNode),
    size: Size<i32, Physical>,
    scale: i32,
    copy_format: Fourcc,
    valid: bool,
}

impl SceneBuffer {
    pub(super) fn invalidate(&mut self) {
        self.valid = false;
    }
}

pub(super) struct OutputPass {
    pub(super) sync: SyncPoint,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_pass(
    gpus: &mut GpuState,
    target: DrmNode,
    output: &mut KmsOutputSurface,
    dmabuf: &mut Dmabuf,
    state: &mut App,
    view: &OutputRenderView,
    age: usize,
    force: bool,
) -> Result<OutputPass, Box<dyn std::error::Error>> {
    let source = gpus.scene_node.ok_or("native scene renderer unavailable")?;
    let started = Instant::now();
    let size = output.target.output_size();
    let rect = Rectangle::new(view.location, view.size);
    if source == target {
        output.reset_scene();
        // Preserve the single-GPU path: no intermediate allocation or blit.
        let mut renderer = gpus.manager.single_renderer(&source)?;
        let renderer = renderer.as_mut();
        let age = buffer_age::scene_age(renderer, age, force)?;
        let mut framebuffer = renderer.bind(dmabuf)?;
        let rendered = state.render_frame_at(
            renderer,
            &mut framebuffer,
            &mut output.damage_tracker,
            age,
            rect,
            view.scale,
        )?;
        return Ok(OutputPass {
            sync: rendered.sync,
        });
    }

    // Preserve direct sharing in either GPU direction, even when its usable
    // copy Fourcc differs from scanout. Smithay owns allocation and CPU fallback.
    let direct = direct_copy_format(&mut gpus.manager, source, target, output.format)?;
    let target_context = gpus.manager.single_renderer(&target)?.as_ref().context_id();
    let (image, scene_sync, copy_format) = {
        let mut renderer = gpus.manager.single_renderer(&source)?;
        let renderer = renderer.as_mut();
        let context = renderer.context_id();
        let recreate = output.scene.as_ref().is_none_or(|scene| {
            scene.context != context
                || scene.target_context != target_context
                || scene.nodes != (source, target)
                || scene.size != size
                || scene.scale != view.scale
        });
        if recreate {
            output.reset_scene();
            let formats = <GlesRenderer as Bind<Dmabuf>>::supported_formats(renderer)
                .unwrap_or_default()
                .into_iter()
                .map(|format| format.code)
                .collect::<Vec<_>>();
            let copy_format = direct
                .or_else(|| transfer_format(output.format, &formats))
                .ok_or("canonical GPU has no supported image-transfer framebuffer format")?;
            info!(scene_node = ?source, target_node = ?target, name = output.target.connector_name(),
                ?size, ?copy_format, "created canonical scene for secondary GPU output");
            let texture: GlesTexture = renderer.create_buffer(
                Fourcc::Abgr8888,
                Size::<i32, Buffer>::from((size.w, size.h)),
            )?;
            output.scene = Some(SceneBuffer {
                texture,
                context: context.clone(),
                target_context,
                nodes: (source, target),
                size,
                scale: view.scale,
                copy_format,
                valid: false,
            });
            output.damage_tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        }
        output.transfer.prepare(renderer)?;
        let scene = output.scene.as_mut().expect("scene buffer initialized");
        // Source contents have age one after success, regardless of which
        // target swapchain slot is acquired. Failure invalidates those contents.
        let scene_age = persistent_scene_age(scene.valid, force);
        scene.valid = false;
        let rendered = {
            let mut framebuffer = renderer.bind(&mut scene.texture)?;
            state.render_frame_at(
                renderer,
                &mut framebuffer,
                &mut output.damage_tracker,
                scene_age,
                rect,
                view.scale,
            )?
        }; // Release the texture write lock before cloning/sampling it.
        scene.valid = true;
        let image = MultiTexture::from_native_texture::<GpuApi>(&context, scene.texture.clone())
            .ok_or("native scene texture has no transferable format")?;
        (image, rendered.sync, direct.unwrap_or(scene.copy_format))
    };
    let scene_duration = started.elapsed();
    let transfer_started = Instant::now();

    let mut renderer = gpus
        .manager
        .renderer(&source, &target, copy_format)?
        .with_transfer_cache(&mut output.transfer.cache);
    let mut framebuffer = renderer.bind(dmabuf)?;
    let full = Rectangle::<i32, Physical>::from_size(size);
    let completed = (|| {
        let frame = renderer.render(&mut framebuffer, size, Transform::Normal)?;
        output.transfer.completion.finish(frame, |frame| {
            frame.wait(&scene_sync)?;
            // Public MultiFrame operations record transfer damage. Drawing through
            // its GLES escape hatch bypasses copy damage and completion.
            frame.clear(Color32F::new(0.0, 0.0, 0.0, 1.0), &[full])?;
            frame.render_texture_from_to(
                &image,
                Rectangle::from_size(image.size()).to_f64(),
                full,
                &[full],
                &[full],
                Transform::Normal,
                1.0,
            )
        })
    })();
    let (draw_result, sync) = match completed {
        Ok(completed) => completed,
        Err(error) => {
            // No trustworthy completion fence exists. Drop this output's
            // storage instead of overwriting an intermediate still in use.
            drop(framebuffer);
            if let Err(cleanup) = renderer.invalidate_caches() {
                warn!(%cleanup, "failed to retire interrupted transfer cache");
            }
            drop(renderer);
            output.transfer.reset();
            return Err(error.into());
        }
    };
    // Completion is retained even if drawing, readback, or KMS queueing fails.
    draw_result?;
    trace!(scene_node = ?source, target_node = ?target, name = output.target.connector_name(),
        scene_us = scene_duration.as_micros() as u64,
        transfer_us = transfer_started.elapsed().as_micros() as u64,
        "completed native cross-GPU image transfer");
    Ok(OutputPass { sync })
}

fn persistent_scene_age(valid: bool, force: bool) -> usize {
    usize::from(valid && !force)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_scene_never_uses_target_swapchain_age() {
        // Newly allocated, reused, invalidated after failure, and restored after
        // a cursorless pass: source history is independent of target rotation.
        assert_eq!(persistent_scene_age(false, false), 0);
        assert_eq!(persistent_scene_age(true, false), 1);
        assert_eq!(persistent_scene_age(false, false), 0);
        assert_eq!(persistent_scene_age(true, true), 0);
    }
}
