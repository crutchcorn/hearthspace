//! Cached low-resolution Gaussian sampling and full-resolution rounded masks.
use super::*;

impl Backdrop {
    pub(super) fn new(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        sample_scale: f64,
    ) -> Result<Self> {
        let low = Size::<i32, Buffer>::from((
            (f64::from(size.w) / sample_scale).ceil() as i32,
            (f64::from(size.h) / sample_scale).ceil() as i32,
        ));
        let padded = (low.w + 2 * PADDING, low.h + 2 * PADDING);
        // Only the visible output intersection is allocated at full resolution.
        // The source and Gaussian passes stay small even at high output scale.
        let scene = renderer.create_buffer(Fourcc::Abgr8888, padded.into())?;
        let intermediate = renderer.create_buffer(Fourcc::Abgr8888, padded.into())?;
        let low_blur = renderer.create_buffer(Fourcc::Abgr8888, low)?;
        let resolved = renderer.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())?;
        Ok(Self {
            size,
            sample_scale,
            scene,
            intermediate,
            low_blur,
            resolved: TextureRenderBuffer::from_texture(
                renderer,
                resolved,
                1,
                Transform::Normal,
                None,
            ),
            damage: OutputDamageTracker::new(padded, 1.0, Transform::Normal),
            mask: None,
            saturation: 1.0,
            radius: 19.0,
            #[cfg(test)]
            blur_passes: 0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update(
        &mut self,
        renderer: &mut GlesRenderer,
        shader: &WindowBackdropShader,
        output: Rectangle<i32, Physical>,
        window: Rectangle<i32, Physical>,
        visible: Rectangle<i32, Physical>,
        radius: f64,
        saturation: f32,
        source: &[TranslatedHearthspaceRenderElement],
    ) -> Result<()> {
        let elements: Vec<_> = source
            .iter()
            .map(|element| {
                RelocateRenderElement::from_element(
                    RescaleRenderElement::from_element(
                        RelocateRenderElement::from_element(
                            element,
                            (-visible.loc.x, -visible.loc.y),
                            Relocate::Relative,
                        ),
                        Point::from((0, 0)),
                        1.0 / self.sample_scale,
                    ),
                    (PADDING, PADDING),
                    Relocate::Relative,
                )
            })
            .collect();
        let changed = {
            let mut target = renderer.bind(&mut self.scene)?;
            self.damage
                .render_output(
                    renderer,
                    &mut target,
                    usize::from(self.mask.is_some()),
                    &elements,
                    Color32F::new(0.0, 0.0, 0.0, 1.0),
                )?
                .damage
                .is_some()
        };
        let next_mask = (window, visible);
        if !changed
            && self.mask == Some(next_mask)
            && self.saturation == saturation
            && self.radius == radius
        {
            return Ok(());
        }
        let low = Size::from((
            (f64::from(self.size.w) / self.sample_scale).ceil() as i32,
            (f64::from(self.size.h) / self.sample_scale).ceil() as i32,
        ));
        // Clamp each blur tap at the physical output boundary. Unmapped space
        // outside a monitor must not produce black fringes around maximized UI.
        let padded_size = (low.w + PADDING * 2, low.h + PADDING * 2);
        let left = (f64::from(-visible.loc.x) / self.sample_scale + f64::from(PADDING)).max(0.0);
        let top = (f64::from(-visible.loc.y) / self.sample_scale + f64::from(PADDING)).max(0.0);
        let right = (f64::from(output.size.w - visible.loc.x) / self.sample_scale
            + f64::from(PADDING))
        .min(f64::from(padded_size.0));
        let bottom = (f64::from(output.size.h - visible.loc.y) / self.sample_scale
            + f64::from(PADDING))
        .min(f64::from(padded_size.1));
        // A vertical SSD resize changes its whole-window corner mask, but not
        // the sampled header strip. Reuse the Gaussian result for mask/tint
        // changes; moving the sampled region also changes its boundary clamps.
        if changed || self.mask.is_none_or(|(_, previous)| previous != visible) {
            shader.blur.render_region(
                renderer,
                &self.scene,
                &mut self.low_blur,
                &mut self.intermediate,
                low,
                PADDING,
                0.0,
                Some(Rectangle::new(
                    (left, top).into(),
                    ((right - left).max(1.0), (bottom - top).max(1.0)).into(),
                )),
            )?;
            #[cfg(test)]
            {
                self.blur_passes += 1;
            }
        }
        self.resolved.render().draw(|target| {
            shader.resolve(
                renderer,
                &self.low_blur,
                target,
                self.size,
                window,
                visible,
                self.sample_scale,
                radius,
                saturation,
            )?;
            Ok::<_, smithay::backend::renderer::gles::GlesError>(vec![Rectangle::from_size(
                (self.size.w, self.size.h).into(),
            )])
        })?;
        self.mask = Some(next_mask);
        self.saturation = saturation;
        self.radius = radius;
        Ok(())
    }
}
