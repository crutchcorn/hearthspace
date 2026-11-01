use super::{CapturedFrame, render::PresenterElement};
use crate::compositor::capture::read_pixels;
use smithay::{
    backend::{
        allocator::{Fourcc, dmabuf::Dmabuf, gbm::GbmBuffer},
        drm::{compositor::RenderFrameResult, gbm::GbmFramebuffer},
        renderer::{
            Bind, Offscreen,
            gles::{GlesRenderer, GlesTexture},
        },
    },
    utils::{Buffer, Physical, Rectangle, Size, Transform},
};

pub(super) fn read_submission(
    renderer: &mut GlesRenderer,
    result: &RenderFrameResult<'_, GbmBuffer, GbmFramebuffer, PresenterElement>,
    size: Size<i32, Physical>,
    scene: &Dmabuf,
    scratch: &mut Option<GlesTexture>,
) -> Result<CapturedFrame, String> {
    // The same validation as ordinary readback, before allocating a texture.
    let pixels = i64::from(size.w) * i64::from(size.h);
    if size.w <= 0 || size.h <= 0 || pixels > 16_777_216 {
        return Err("capture exceeds 16 megapixel limit or has zero size".into());
    }
    if scratch.is_none() {
        *scratch = Some(
            renderer
                .create_buffer(
                    Fourcc::Abgr8888,
                    Size::<i32, Buffer>::from((size.w, size.h)),
                )
                .map_err(|error| error.to_string())?,
        );
    }
    let pixels = {
        let mut framebuffer = renderer
            .bind(scratch.as_mut().unwrap())
            .map_err(|error| error.to_string())?;
        let _sync = result
            .blit_frame_result(
                size,
                Transform::Normal,
                1.0,
                renderer,
                &mut framebuffer,
                [Rectangle::from_size(size)],
                std::iter::empty(),
            )
            .map_err(|error| error.to_string())?;
        // read_pixels maps the readback; the GPU completion is observed here.
        read_pixels(renderer, &framebuffer, size, false).map_err(|error| error.to_string())?
    };
    let clean_pixels = {
        let mut scene = scene.clone();
        let framebuffer = renderer
            .bind(&mut scene)
            .map_err(|error| error.to_string())?;
        read_pixels(renderer, &framebuffer, size, false).map_err(|error| error.to_string())?
    };
    Ok(CapturedFrame {
        size,
        pixels,
        clean_pixels,
    })
}
