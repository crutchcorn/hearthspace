//! A 40-logical-pixel Gaussian at quarter logical resolution, then a sharp mask.
use smithay::{
    backend::renderer::{
        Bind, Color32F, Frame, Renderer, Texture,
        gles::{
            GlesError, GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
        },
    },
    utils::{Buffer, Physical, Rectangle, Size, Transform},
};

use super::blur_shader::{CornerBlurShader, configure_textures};

pub(super) const DOWNSAMPLE: f64 = 4.0;
pub(super) const PADDING: i32 = 30;

pub(super) struct WindowBackdropShader {
    pub blur: CornerBlurShader,
    resolve: GlesTexProgram,
}

impl WindowBackdropShader {
    pub fn new(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        Ok(Self {
            blur: CornerBlurShader::with_kernel(renderer, PADDING as usize, 10.0)?,
            resolve: renderer.compile_custom_texture_shader(
                include_str!("window_backdrop_resolve.frag"),
                &[
                    UniformName::new("source_size", UniformType::_2f),
                    UniformName::new("sample_scale", UniformType::_1f),
                    UniformName::new("window_size", UniformType::_2f),
                    UniformName::new("window_offset", UniformType::_2f),
                    UniformName::new("radius", UniformType::_1f),
                    UniformName::new("saturation", UniformType::_1f),
                ],
            )?,
        })
    }

    /// The resolve pass does one bilinear fetch per output pixel. Rounding is
    /// applied here, at output resolution, so downsampling never softens edges.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &self,
        renderer: &mut GlesRenderer,
        source: &GlesTexture,
        target: &mut GlesTexture,
        size: Size<i32, Physical>,
        window: Rectangle<i32, Physical>,
        visible: Rectangle<i32, Physical>,
        sample_scale: f64,
        radius: f64,
        saturation: f32,
    ) -> Result<(), GlesError> {
        configure_textures(renderer, &[source, target])?;
        let source_size = source.size();
        let uniforms = [
            Uniform::new("source_size", (source_size.w as f32, source_size.h as f32)),
            Uniform::new("sample_scale", sample_scale as f32),
            Uniform::new("window_size", (window.size.w as f32, window.size.h as f32)),
            Uniform::new(
                "window_offset",
                (
                    (visible.loc.x - window.loc.x) as f32,
                    (visible.loc.y - window.loc.y) as f32,
                ),
            ),
            Uniform::new("radius", radius as f32),
            Uniform::new("saturation", saturation),
        ];
        let full = Rectangle::from_size(size);
        let src = Rectangle::<f64, Buffer>::from_size(
            (
                f64::from(size.w) / sample_scale,
                f64::from(size.h) / sample_scale,
            )
                .into(),
        );
        let mut framebuffer = renderer.bind(target)?;
        let mut frame = renderer.render(&mut framebuffer, size, Transform::Normal)?;
        frame.clear(Color32F::TRANSPARENT, &[full])?;
        frame.render_texture_from_to(
            source,
            src,
            full,
            &[full],
            &[],
            Transform::Normal,
            1.0,
            Some(&self.resolve),
            &uniforms,
        )?;
        let _sync = frame.finish()?;
        Ok(())
    }
}
