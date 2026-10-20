//! GPU-only blur of the desktop pixels behind a shell corner.
use hearthspace_common::SHELL_CORNER_RADIUS;
use smithay::{
    backend::renderer::{
        Bind, Color32F, Frame, Renderer, Texture,
        gles::{
            GlesError, GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
        },
    },
    utils::{Buffer, Physical, Rectangle, Size, Transform},
};

/// The kernel reaches twelve logical pixels beyond each corner edge.
pub(super) const BLUR_PADDING: i32 = 12;

#[derive(Debug)]
pub(super) struct CornerBlurShader {
    program: GlesTexProgram,
}

impl CornerBlurShader {
    pub(super) fn new(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        let program = renderer.compile_custom_texture_shader(
            include_str!("corner_blur.frag"),
            &[
                UniformName::new("step_uv", UniformType::_2f),
                UniformName::new("texture_size", UniformType::_2f),
                UniformName::new("corner_size", UniformType::_2f),
                UniformName::new("crop_origin", UniformType::_2f),
                UniformName::new("radius", UniformType::_1f),
                UniformName::new("rounded", UniformType::_1f),
            ],
        )?;
        Ok(Self { program })
    }

    /// All sizes and padding are physical pixels. The scene and intermediate
    /// textures cover the padded rectangle; target covers only the corner.
    /// Textures must be distinct and belong to this renderer's context.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render(
        &self,
        renderer: &mut GlesRenderer,
        scene: &GlesTexture,
        target: &mut GlesTexture,
        intermediate: &mut GlesTexture,
        corner_size: Size<i32, Physical>,
        padding: i32,
        scale: i32,
    ) -> Result<(), GlesError> {
        // Smithay's Offscreen allocation leaves the GL default REPEAT wrap.
        // GLES2 requires CLAMP_TO_EDGE for non-power-of-two textures. Keep the
        // existing active unit and binding intact while configuring our buffers.
        renderer.with_context(|gl| unsafe {
            use smithay::backend::renderer::gles::ffi;
            let mut previous = 0;
            gl.GetIntegerv(ffi::TEXTURE_BINDING_2D, &mut previous);
            for texture in [scene.tex_id(), intermediate.tex_id(), target.tex_id()] {
                gl.BindTexture(ffi::TEXTURE_2D, texture);
                gl.TexParameteri(
                    ffi::TEXTURE_2D,
                    ffi::TEXTURE_WRAP_S,
                    ffi::CLAMP_TO_EDGE as i32,
                );
                gl.TexParameteri(
                    ffi::TEXTURE_2D,
                    ffi::TEXTURE_WRAP_T,
                    ffi::CLAMP_TO_EDGE as i32,
                );
            }
            gl.BindTexture(ffi::TEXTURE_2D, previous as u32);
        })?;

        let size = scene.size();
        let stride = 3.0 * scale.max(1) as f32;
        let uniforms = |direction: (f32, f32), rounded: f32| {
            vec![
                Uniform::new("step_uv", direction),
                Uniform::new("texture_size", (size.w as f32, size.h as f32)),
                Uniform::new("corner_size", (corner_size.w as f32, corner_size.h as f32)),
                Uniform::new("crop_origin", (padding as f32, padding as f32)),
                Uniform::new("radius", SHELL_CORNER_RADIUS as f32 * scale.max(1) as f32),
                Uniform::new("rounded", rounded),
            ]
        };
        self.pass(
            renderer,
            scene,
            intermediate,
            Rectangle::from_size(size.to_f64()),
            uniforms((stride / size.w as f32, 0.0), 0.0),
        )?;
        self.pass(
            renderer,
            intermediate,
            target,
            Rectangle::new(
                (f64::from(padding), f64::from(padding)).into(),
                (f64::from(corner_size.w), f64::from(corner_size.h)).into(),
            ),
            uniforms((0.0, stride / size.h as f32), 1.0),
        )
    }

    fn pass(
        &self,
        renderer: &mut GlesRenderer,
        source: &GlesTexture,
        target: &mut GlesTexture,
        source_rect: Rectangle<f64, Buffer>,
        uniforms: Vec<Uniform<'static>>,
    ) -> Result<(), GlesError> {
        let size = Size::<i32, Physical>::from((target.width() as i32, target.height() as i32));
        let full = Rectangle::from_size(size);
        let mut framebuffer = renderer.bind(target)?;
        let mut frame = renderer.render(&mut framebuffer, size, Transform::Normal)?;
        // Rounded alpha must replace the last result rather than accumulate it.
        frame.clear(Color32F::TRANSPARENT, &[full])?;
        frame.render_texture_from_to(
            source,
            source_rect,
            full,
            &[full],
            &[],
            Transform::Normal,
            1.0,
            Some(&self.program),
            &uniforms,
        )?;
        // Both passes and final composition use this same GL context, whose
        // command ordering makes a cross-context fence wait unnecessary.
        let _sync = frame.finish()?;
        Ok(())
    }
}
