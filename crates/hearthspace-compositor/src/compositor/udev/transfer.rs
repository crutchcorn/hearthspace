//! Compose every scene on the canonical GLES context, transferring only the
//! completed image. Target-local GBM buffers remain owned by their KMS device.
use super::{
    device::KmsOutputSurface,
    gpu::{GpuApi, GpuState},
};
use crate::compositor::{App, capture::read_pixels, output::OutputRenderView};
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

pub(super) struct SceneBuffer {
    texture: GlesTexture,
    context: ContextId<GlesTexture>,
    size: Size<i32, Physical>,
    scale: i32,
    copy_format: Fourcc,
    valid: bool,
}

pub(super) struct OutputPass {
    pub(super) sync: SyncPoint,
    pub(super) pixels: Result<Option<Vec<u8>>, String>,
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
    capture: bool,
) -> Result<OutputPass, Box<dyn std::error::Error>> {
    let source = gpus.scene_node.ok_or("native scene renderer unavailable")?;
    let started = Instant::now();
    let size = output.target.output_size();
    let rect = Rectangle::new(view.location, view.size);
    if source == target {
        // Preserve the single-GPU path: no intermediate allocation or blit.
        let mut renderer = gpus.manager.single_renderer(&source)?;
        let renderer = renderer.as_mut();
        let mut framebuffer = renderer.bind(dmabuf)?;
        let rendered = state.render_frame_at(
            renderer,
            &mut framebuffer,
            &mut output.damage_tracker,
            if force { 0 } else { age },
            rect,
            view.scale,
        )?;
        let pixels = capture
            .then(|| read_pixels(renderer, &framebuffer, size, false))
            .transpose()
            .map_err(|error| error.to_string());
        return Ok(OutputPass {
            sync: rendered.sync,
            pixels,
        });
    }

    let (image, scene_sync, copy_format) = {
        let mut renderer = gpus.manager.single_renderer(&source)?;
        let renderer = renderer.as_mut();
        if let Some(previous) = gpus.transfers.get(&(source, target)) {
            renderer.wait(previous)?;
        }
        let context = renderer.context_id();
        let recreate = output.scene.as_ref().is_none_or(|scene| {
            scene.context != context || scene.size != size || scene.scale != view.scale
        });
        if recreate {
            let formats = <GlesRenderer as Bind<Dmabuf>>::supported_formats(renderer)
                .unwrap_or_default()
                .into_iter()
                .map(|format| format.code)
                .collect::<Vec<_>>();
            let copy_format = transfer_format(output.gbm_surface.format(), &formats)
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
                size,
                scale: view.scale,
                copy_format,
                valid: false,
            });
            output.damage_tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        }
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
        (image, rendered.sync, scene.copy_format)
    };
    let scene_duration = started.elapsed();
    let transfer_started = Instant::now();

    let mut renderer = gpus.manager.renderer(&source, &target, copy_format)?;
    let mut framebuffer = renderer.bind(dmabuf)?;
    let full = Rectangle::<i32, Physical>::from_size(size);
    let (draw_result, finish_result) = {
        let mut frame = renderer.render(&mut framebuffer, size, Transform::Normal)?;
        let draw_result = (|| {
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
        })();
        // Always finish explicitly, even on a draw error, to retain the fence
        // for work that may already have reached the transfer queue.
        (draw_result, frame.finish())
    };
    let sync = match finish_result {
        Ok(sync) => sync,
        Err(error) => {
            // No trustworthy completion fence exists. Retire the cached pair
            // buffer instead of overwriting storage a target might still read.
            drop(framebuffer);
            if let Err(cleanup) = renderer.invalidate_caches() {
                warn!(%cleanup, "failed to retire interrupted transfer cache");
            }
            return Err(error.into());
        }
    };
    // Retain the target fence even if readback or the later KMS queue fails.
    // The pair's cached intermediate may still be read by the target GPU.
    gpus.transfers.insert((source, target), sync.clone());
    draw_result?;
    let pixels = if capture {
        read_pixels(
            renderer
                .target_as_mut()
                .ok_or("transfer target renderer missing")?,
            framebuffer.as_ref(),
            size,
            false,
        )
        .map(Some)
        .map_err(|error| error.to_string())
    } else {
        Ok(None)
    };
    trace!(scene_node = ?source, target_node = ?target, name = output.target.connector_name(),
        scene_us = scene_duration.as_micros() as u64,
        transfer_us = transfer_started.elapsed().as_micros() as u64,
        "completed native cross-GPU image transfer");
    Ok(OutputPass { sync, pixels })
}

fn persistent_scene_age(valid: bool, force: bool) -> usize {
    usize::from(valid && !force)
}

fn transfer_format(scanout: Fourcc, source_formats: &[Fourcc]) -> Option<Fourcc> {
    // The intermediate is allocated on the source GPU; it need not have the
    // target's scanout format. Smithay negotiates DMA import or CPU fallback.
    [
        scanout,
        Fourcc::Argb8888,
        Fourcc::Xrgb8888,
        Fourcc::Abgr8888,
        Fourcc::Xbgr8888,
    ]
    .into_iter()
    .find(|format| source_formats.contains(format))
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

    #[test]
    fn target_scanout_format_does_not_require_source_framebuffer_support() {
        assert_eq!(
            transfer_format(Fourcc::Xrgb8888, &[Fourcc::Argb8888]),
            Some(Fourcc::Argb8888)
        );
        assert_eq!(
            transfer_format(Fourcc::Xrgb8888, &[Fourcc::Argb8888, Fourcc::Xrgb8888]),
            Some(Fourcc::Xrgb8888)
        );
        assert_eq!(transfer_format(Fourcc::Xrgb8888, &[]), None);
    }
}
