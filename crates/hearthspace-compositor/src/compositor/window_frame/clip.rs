//! Clip only compositor-decorated windows. Client decorations retain their shape.
use std::hash::{DefaultHasher, Hash, Hasher};

use smithay::{
    backend::renderer::{
        Color32F, Frame,
        element::{
            Element, Id, Kind, RenderElement,
            surface::{WaylandSurfaceRenderElement, WaylandSurfaceTexture},
        },
        gles::{
            GlesError, GlesFrame, GlesPixelProgram, GlesRenderer, GlesTexProgram, Uniform,
            UniformName, UniformType,
        },
        utils::{CommitCounter, DamageSet, OpaqueRegions},
    },
    utils::{Buffer, Logical, Physical, Rectangle, Scale, Transform, user_data::UserDataMap},
};

#[derive(Clone, Debug)]
pub(super) struct ClipPrograms {
    texture: GlesTexProgram,
    solid: GlesPixelProgram,
}

impl ClipPrograms {
    pub fn new(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        let mut uniforms = vec![
            UniformName::new("clip_x", UniformType::_3f),
            UniformName::new("clip_y", UniformType::_3f),
            UniformName::new("clip_size", UniformType::_2f),
            UniformName::new("clip_radius", UniformType::_1f),
        ];
        let texture = renderer.compile_custom_texture_shader(
            format!(
                "{}\n{}\n{}",
                include_str!("texture.frag"),
                include_str!("clip.frag"),
                include_str!("texture_main.frag")
            ),
            &uniforms,
        )?;
        uniforms.push(UniformName::new("color", UniformType::_4f));
        let solid = renderer.compile_custom_pixel_shader(
            format!(
                "precision highp float;\nuniform vec4 color;\nuniform float alpha;\n{}\n\
                 void main() {{ gl_FragColor = color * alpha * window_mask(); }}",
                include_str!("clip.frag")
            ),
            &uniforms,
        )?;
        Ok(Self { texture, solid })
    }
}

#[derive(Clone, Debug)]
pub(in crate::compositor) struct WindowClip {
    programs: ClipPrograms,
    window: Rectangle<i32, Logical>,
    radius: f64,
}

impl WindowClip {
    pub(super) fn new(
        programs: ClipPrograms,
        window: Rectangle<i32, Logical>,
        radius: f64,
    ) -> Self {
        Self {
            programs,
            window,
            radius,
        }
    }

    /// The element is already in the same scene coordinates as `window`.
    pub fn wrap<E: Element>(&self, inner: E, solid: Option<Color32F>) -> RoundedElement<E> {
        let geometry = inner.geometry(1.0.into());
        let relative = Rectangle::<f64, Physical>::new(
            (
                f64::from(self.window.loc.x) - f64::from(geometry.loc.x),
                f64::from(self.window.loc.y) - f64::from(geometry.loc.y),
            )
                .into(),
            self.window.size.to_f64().to_physical(1.0),
        );
        // A window resize can change the mask of an otherwise unchanged
        // subsurface. A namespaced ID makes that change visible to damage.
        // Moving the complete window preserves the relative mask and its ID.
        let mut key = DefaultHasher::new();
        "hearthspace-rounded-window".hash(&mut key);
        for value in [
            relative.loc.x,
            relative.loc.y,
            relative.size.w,
            relative.size.h,
            self.radius,
        ] {
            value.to_bits().hash(&mut key);
        }
        let id = inner.id().namespaced(key.finish() as usize);
        RoundedElement {
            inner,
            id,
            programs: self.programs.clone(),
            relative,
            radius: self.radius,
            solid,
        }
    }
}

pub(in crate::compositor) fn solid_color(
    surface: &WaylandSurfaceRenderElement<GlesRenderer>,
) -> Option<Color32F> {
    match surface.texture() {
        WaylandSurfaceTexture::SolidColor(color) => Some(*color),
        WaylandSurfaceTexture::Texture(_) => None,
    }
}

#[derive(Debug)]
pub(in crate::compositor) struct RoundedElement<E> {
    inner: E,
    id: Id,
    programs: ClipPrograms,
    relative: Rectangle<f64, Physical>,
    radius: f64,
    solid: Option<Color32F>,
}

impl<E: Element> Element for RoundedElement<E> {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        let native = self.inner.geometry(1.0.into()).size;
        let scaled = self.inner.geometry(scale).size;
        let scale = Scale::from((
            f64::from(scaled.w) / f64::from(native.w.max(1)),
            f64::from(scaled.h) / f64::from(native.h.max(1)),
        ));
        let window = self.relative.upscale(scale).to_i32_down::<i32>();
        let radius = (self.radius * scale.x.max(scale.y)).ceil() as i32 + 1;
        // Conservative opaque cross: the four anti-aliased corners must allow
        // the underlying scene to be drawn, including during retained damage.
        let centers = [
            Rectangle::new(
                (window.loc.x + radius, window.loc.y).into(),
                ((window.size.w - 2 * radius).max(0), window.size.h).into(),
            ),
            Rectangle::new(
                (window.loc.x, window.loc.y + radius).into(),
                (window.size.w, (window.size.h - 2 * radius).max(0)).into(),
            ),
        ];
        self.inner
            .opaque_regions(scale)
            .into_iter()
            .flat_map(|opaque| {
                centers
                    .iter()
                    .filter_map(move |center| opaque.intersection(*center))
            })
            .filter(|rect| !rect.is_empty())
            .collect()
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl<E: RenderElement<GlesRenderer>> RenderElement<GlesRenderer> for RoundedElement<E> {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let full_src = self.inner.src();
        let crop = Rectangle::new(src.loc - full_src.loc, src.size).to_logical(
            1.0,
            self.transform(),
            &full_src.size,
        );
        let full = full_src.size.to_logical(1.0, self.transform());
        let native = self.inner.geometry(1.0.into()).size.to_f64();
        let logical_crop = Rectangle::<f64, Physical>::new(
            (
                crop.loc.x * native.w / full.w,
                crop.loc.y * native.h / full.h,
            )
                .into(),
            (
                crop.size.w * native.w / full.w,
                crop.size.h * native.h / full.h,
            )
                .into(),
        );
        let scale = Scale::from((
            f64::from(dst.size.w) / logical_crop.size.w,
            f64::from(dst.size.h) / logical_crop.size.h,
        ));
        let window = Rectangle::new(
            (self.relative.loc - logical_crop.loc).upscale(scale) + dst.loc.to_f64(),
            self.relative.size.upscale(scale),
        );
        let mut uniforms = projection_uniforms(frame, window, self.radius * scale.x.min(scale.y));
        if let Some(color) = self.solid {
            uniforms.push(Uniform::new(
                "color",
                (color.r(), color.g(), color.b(), color.a()),
            ));
            frame.render_pixel_shader_to(
                &self.programs.solid,
                src,
                dst,
                full_src.size.to_i32_round(),
                Some(damage),
                self.alpha(),
                &uniforms,
            )
        } else {
            // Smithay owns texture import, transform, alpha and external-image
            // variants. Only its default fragment program changes for this
            // draw; never leave the override active for the next window.
            frame.override_default_tex_program(self.programs.texture.clone(), uniforms);
            let result = self
                .inner
                .draw(frame, src, dst, damage, opaque_regions, cache);
            frame.clear_tex_program_override();
            result
        }
    }
}

fn projection_uniforms(
    frame: &GlesFrame<'_, '_>,
    window: Rectangle<f64, Physical>,
    radius: f64,
) -> Vec<Uniform<'static>> {
    let m = frame.projection();
    let viewport = frame.transformation().transform_size(frame.output_size());
    let a = m[0] * viewport.w as f32 / 2.0;
    let b = m[3] * viewport.w as f32 / 2.0;
    let c = m[1] * viewport.h as f32 / 2.0;
    let d = m[4] * viewport.h as f32 / 2.0;
    let tx = (m[6] + 1.0) * viewport.w as f32 / 2.0;
    let ty = (m[7] + 1.0) * viewport.h as f32 / 2.0;
    let determinant = a * d - b * c;
    let (xx, xy, yx, yy) = (
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
    );
    vec![
        Uniform::new("clip_x", (xx, xy, -xx * tx - xy * ty - window.loc.x as f32)),
        Uniform::new("clip_y", (yx, yy, -yx * tx - yy * ty - window.loc.y as f32)),
        Uniform::new("clip_size", (window.size.w as f32, window.size.h as f32)),
        Uniform::new("clip_radius", radius as f32),
    ]
}
