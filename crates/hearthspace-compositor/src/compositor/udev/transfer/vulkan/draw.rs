use super::{Request, Result, slot::Resources};
use crate::compositor::{
    capture::read_pixels,
    udev::{
        gpu::GpuApi,
        transfer::OutputPass,
        vulkan_bridge::{CopyAcquire, NativeFence},
    },
};
use smithay::{
    backend::{
        egl::fence::EGLFence,
        renderer::{Bind, Frame, Renderer, gles::GlesRenderer, multigpu::GpuManager},
    },
    utils::{Buffer, Physical, Rectangle, Transform},
};

fn wait_gpu(renderer: &mut GlesRenderer, dependency: NativeFence) -> Result<()> {
    let NativeFence::SyncFile(fd) = dependency else {
        return Ok(());
    };
    let display = renderer.egl_context().display().clone();
    // Both importing and waiting require the owning current EGL context. The
    // offscreen proof caught EGL_BAD_MATCH when import preceded with_context.
    renderer.with_context(|_| EGLFence::import(&display, fd)?.wait(&display))??;
    Ok(())
}

pub(super) fn copy(
    manager: &mut GpuManager<GpuApi>,
    work: &mut Resources,
    request: &mut Request<'_>,
) -> Result<OutputPass> {
    let full = Rectangle::<i32, Physical>::from_size(work.size);
    let source_rect =
        Rectangle::<i32, Buffer>::from_size((work.size.w, work.size.h).into()).to_f64();
    let source_ready = {
        let mut renderer = manager.single_renderer(&request.source)?;
        let renderer = renderer.as_mut();
        renderer.wait(request.scene_sync)?;
        wait_gpu(renderer, work.source_released.try_clone()?)?;
        let mut framebuffer = renderer.bind(&mut work.source_dma)?;
        work.unknown_external = true;
        let mut frame = renderer.render(&mut framebuffer, work.size, Transform::Normal)?;
        let drawn = Frame::render_texture_from_to(
            &mut frame,
            request.scene,
            source_rect,
            full,
            &[full],
            &[full],
            Transform::Normal,
            1.0,
        );
        let finished = frame.finish()?;
        let fence = NativeFence::from_sync_point(&finished)?;
        work.source_released = fence.try_clone()?;
        work.unknown_external = false;
        drawn?;
        fence
    };
    let done = work.bridge.as_mut().expect("active bridge").submit(
        &work.source,
        &work.target,
        CopyAcquire {
            source_ready,
            target_released: work.target_released.try_clone()?,
        },
    )?;
    work.source_released = done.source_released;
    let mut renderer = manager.single_renderer(&request.target)?;
    let renderer = renderer.as_mut();
    wait_gpu(renderer, done.target_ready)?;
    let mut framebuffer = renderer.bind(request.dmabuf)?;
    work.unknown_external = true;
    let mut frame = renderer.render(&mut framebuffer, work.size, Transform::Normal)?;
    let drawn = Frame::render_texture_from_to(
        &mut frame,
        &work.target_texture,
        source_rect,
        full,
        &[full],
        &[full],
        Transform::Normal,
        1.0,
    );
    let sync = frame.finish()?;
    // This fence is retained before draw/readback/KMS errors can return. It is
    // both the next Vulkan writer's dependency and this scanout's completion.
    work.target_released = NativeFence::from_sync_point(&sync)?;
    work.unknown_external = false;
    drawn?;
    let pixels = request
        .capture
        .then(|| read_pixels(renderer, &framebuffer, work.size, false))
        .transpose()
        .map_err(|error| error.to_string());
    Ok(OutputPass { sync, pixels })
}
