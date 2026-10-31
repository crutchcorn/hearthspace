//! Compose every scene on the canonical GLES context, transferring only the
//! completed image. Target-local GBM buffers remain owned by their KMS device.
pub(super) mod cache;
pub(super) mod vulkan;
use super::{
    device::KmsOutputSurface,
    gpu::{GpuApi, GpuState},
};
use crate::compositor::{
    App, capture::read_pixels, output::OutputRenderView, rendering::RenderedFrame,
};
use smithay::{
    backend::{
        allocator::{Fourcc, dmabuf::Dmabuf},
        drm::DrmNode,
        renderer::{
            Bind, Color32F, ContextId, Frame, Offscreen, Renderer, RendererSuper, Texture,
            damage::OutputDamageTracker,
            gles::{GlesRenderer, GlesTexture},
            multigpu::MultiTexture,
            sync::SyncPoint,
        },
    },
    utils::{Buffer, Logical, Physical, Rectangle, Size, Transform},
};
use std::time::Instant;
use tracing::{info, trace, warn};

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
    pub(super) software_cursor: bool,
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
    cursorless: bool,
) -> Result<OutputPass, Box<dyn std::error::Error>> {
    let source = gpus.scene_node.ok_or("native scene renderer unavailable")?;
    let started = Instant::now();
    let size = output.target.output_size();
    let rect = Rectangle::new(view.location, view.size);
    if gpus.bridges.scanout_blocked(dmabuf) {
        // A later frame must not turn a submitted failure into an immediate
        // fallback overwrite. Recovery may allocate a different scanout while
        // the retired slot retains this exact DMA-BUF until completion.
        return Err("scanout still belongs to an unresolved Vulkan transfer".into());
    }
    if source == target {
        output.reset_scene();
        // Preserve the single-GPU path: no intermediate allocation or blit.
        let mut renderer = gpus.manager.single_renderer(&source)?;
        let renderer = renderer.as_mut();
        let mut framebuffer = renderer.bind(dmabuf)?;
        let rendered = if cursorless {
            state.render_cursorless_frame_at(
                renderer,
                &mut framebuffer,
                &output.damage_tracker,
                rect,
                view.scale,
            )
        } else {
            state.render_frame_at(
                renderer,
                &mut framebuffer,
                &mut output.damage_tracker,
                if force { 0 } else { age },
                rect,
                view.scale,
            )
        }?;
        let pixels = capture
            .then(|| read_pixels(renderer, &framebuffer, size, false))
            .transpose()
            .map_err(|error| error.to_string());
        return Ok(OutputPass {
            sync: rendered.sync,
            software_cursor: rendered.software_cursor,
            pixels,
        });
    }

    // Preserve direct sharing in either GPU direction, even when its usable
    // copy Fourcc differs from scanout. Vulkan only fills an empty intersection.
    let direct = vulkan::direct_copy_format(
        &mut gpus.manager,
        source,
        target,
        output.gbm_surface.format(),
    )?;
    let target_context = gpus.manager.single_renderer(&target)?.as_ref().context_id();
    let (image, native_image, scene_sync, software_cursor, copy_format) = {
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
                .or_else(|| transfer_format(output.gbm_surface.format(), &formats))
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
            if cursorless {
                state.render_cursorless_frame_at(
                    renderer,
                    &mut framebuffer,
                    &output.damage_tracker,
                    rect,
                    view.scale,
                )
            } else {
                state.render_frame_at(
                    renderer,
                    &mut framebuffer,
                    &mut output.damage_tracker,
                    scene_age,
                    rect,
                    view.scale,
                )
            }?
        }; // Release the texture write lock before cloning/sampling it.
        scene.valid = true;
        let image = MultiTexture::from_native_texture::<GpuApi>(&context, scene.texture.clone())
            .ok_or("native scene texture has no transferable format")?;
        (
            image,
            scene.texture.clone(),
            rendered.sync,
            rendered.software_cursor,
            direct.unwrap_or(scene.copy_format),
        )
    };
    let scene_duration = started.elapsed();
    let transfer_started = Instant::now();

    if direct.is_none()
        && let Some(mut pass) = vulkan::render(
            gpus,
            vulkan::Request {
                source,
                target,
                output: output.target.connector_name(),
                size,
                copy_format,
                scene: &native_image,
                scene_sync: &scene_sync,
                dmabuf,
                capture,
            },
        )?
    {
        pass.software_cursor = software_cursor;
        trace!(scene_node = ?source, target_node = ?target, name = output.target.connector_name(),
                scene_us = scene_duration.as_micros() as u64,
                transfer_us = transfer_started.elapsed().as_micros() as u64,
                "completed native Vulkan image transfer");
        return Ok(pass);
    }

    // A readiness poll can retire earlier work without submitting this frame.
    // Recheck its scanout ownership before accepting the setup fallback.
    if gpus.bridges.scanout_blocked(dmabuf) {
        return Err("fallback scanout still belongs to a retired Vulkan transfer".into());
    }
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
    Ok(OutputPass {
        sync,
        pixels,
        software_cursor,
    })
}

impl App {
    /// A temporary capture must not advance the presented scene's damage
    /// history: two renders into one acquired GBM buffer still age it once.
    /// The caller must fully restore the visible scene before queueing it.
    pub(in crate::compositor) fn render_cursorless_frame_at(
        &mut self,
        renderer: &mut GlesRenderer,
        framebuffer: &mut <GlesRenderer as RendererSuper>::Framebuffer<'_>,
        presented_damage: &OutputDamageTracker,
        output_rect: Rectangle<i32, Logical>,
        output_scale: i32,
    ) -> Result<RenderedFrame, Box<dyn std::error::Error>> {
        // Retain the exact physical mode: a rounded logical size multiplied by
        // scale need not equal the scanout buffer's pixel dimensions.
        let mut temporary_damage =
            OutputDamageTracker::from_mode_source(presented_damage.mode().clone());
        let visible = std::mem::replace(&mut self.software_cursor_visible, false);
        let result = self.render_frame_at(
            renderer,
            framebuffer,
            &mut temporary_damage,
            0,
            output_rect,
            output_scale,
        );
        self.software_cursor_visible = visible;
        result
    }
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
