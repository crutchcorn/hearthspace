//! Smithay retains texture import and output transforms; only the transient texture
//! sample coordinates rotate. No client buffer readback or CPU rasterization occurs.
use super::turn::rotated_bounds;
use hearthspace_common::frame_layout::{Point, Rect};
use smithay::{
    backend::renderer::{
        Frame,
        element::{Element, Id, Kind, RenderElement, texture::TextureRenderElement},
        gles::{
            GlesError, GlesFrame, GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName,
            UniformType,
        },
        utils::{CommitCounter, DamageSet, OpaqueRegions},
    },
    utils::{Buffer, Physical, Rectangle, Scale, Transform, user_data::UserDataMap},
};
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Debug)]
pub(in crate::compositor) struct TurnElement {
    inner: TextureRenderElement<GlesTexture>,
    shader: GlesTexProgram,
    id: Id,
    bounds: Rect,
    rotated: Rect,
    pivot: Point,
    angle: f64,
}

pub(super) fn compile(renderer: &mut GlesRenderer) -> Result<GlesTexProgram, GlesError> {
    renderer.compile_custom_texture_shader(
        include_str!("turn.frag"),
        &[
            UniformName::new("turn_x", UniformType::_3f),
            UniformName::new("turn_y", UniformType::_3f),
            UniformName::new("turn_size", UniformType::_2f),
            UniformName::new("turn_aa", UniformType::_2f),
        ],
    )
}

impl TurnElement {
    pub(super) fn new(
        inner: TextureRenderElement<GlesTexture>,
        shader: GlesTexProgram,
        bounds: Rect,
        pivot: Point,
        angle: f64,
    ) -> Self {
        let mut hash = DefaultHasher::new();
        "hearthspace-layout-turn".hash(&mut hash);
        for value in [
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            pivot.x,
            pivot.y,
            angle,
        ] {
            value.to_bits().hash(&mut hash);
        }
        let id = inner.id().namespaced(hash.finish() as usize);
        Self {
            inner,
            shader,
            id,
            bounds,
            pivot,
            angle,
            rotated: rotated_bounds(bounds, pivot, angle),
        }
    }
}

impl Element for TurnElement {
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
        Rectangle::new(
            (self.rotated.x * scale.x, self.rotated.y * scale.y).into(),
            (self.rotated.width * scale.x, self.rotated.height * scale.y).into(),
        )
        .to_i32_up()
    }
    fn damage_since(
        &self,
        scale: Scale<f64>,
        _: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        [Rectangle::from_size(self.geometry(scale).size)]
            .into_iter()
            .collect()
    }
    fn opaque_regions(&self, _: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        Default::default()
    }
    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }
    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl RenderElement<GlesRenderer> for TurnElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let geometry = self.geometry(1.0.into());
        let scale = Point {
            x: f64::from(dst.size.w) / f64::from(geometry.size.w.max(1)),
            y: f64::from(dst.size.h) / f64::from(geometry.size.h.max(1)),
        };
        let offset = Point {
            x: f64::from(dst.loc.x) - f64::from(geometry.loc.x) * scale.x,
            y: f64::from(dst.loc.y) - f64::from(geometry.loc.y) * scale.y,
        };
        let bounds = Rect {
            x: self.bounds.x * scale.x + offset.x,
            y: self.bounds.y * scale.y + offset.y,
            width: self.bounds.width * scale.x,
            height: self.bounds.height * scale.y,
        };
        let pivot = Point {
            x: self.pivot.x * scale.x + offset.x,
            y: self.pivot.y * scale.y + offset.y,
        };
        let uniforms = uniforms(frame, bounds, pivot, self.angle, dst);
        frame.override_default_tex_program(self.shader.clone(), uniforms);
        let result = <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(
            &self.inner,
            frame,
            src,
            dst,
            damage,
            &[],
            cache,
        );
        frame.clear_tex_program_override();
        result
    }
}

fn uniforms(
    frame: &GlesFrame<'_, '_>,
    bounds: Rect,
    pivot: Point,
    angle: f64,
    dst: Rectangle<i32, Physical>,
) -> Vec<Uniform<'static>> {
    let matrix = frame.projection();
    let viewport = frame.transformation().transform_size(frame.output_size());
    let a = f64::from(matrix[0]) * f64::from(viewport.w) / 2.;
    let b = f64::from(matrix[3]) * f64::from(viewport.w) / 2.;
    let c = f64::from(matrix[1]) * f64::from(viewport.h) / 2.;
    let d = f64::from(matrix[4]) * f64::from(viewport.h) / 2.;
    let tx = f64::from(matrix[6] + 1.) * f64::from(viewport.w) / 2.;
    let ty = f64::from(matrix[7] + 1.) * f64::from(viewport.h) / 2.;
    let determinant = a * d - b * c;
    let (xx, xy, yx, yy) = (
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
    );
    let (sin, cos) = angle.sin_cos();
    let nx = (cos * xx + sin * yx) / bounds.width;
    let ny = (cos * xy + sin * yy) / bounds.width;
    let mx = (-sin * xx + cos * yx) / bounds.height;
    let my = (-sin * xy + cos * yy) / bounds.height;
    let ox =
        (pivot.x - cos * pivot.x - sin * pivot.y - bounds.x) / bounds.width - nx * tx - ny * ty;
    let oy =
        (pivot.y + sin * pivot.x - cos * pivot.y - bounds.y) / bounds.height - mx * tx - my * ty;
    vec![
        Uniform::new("turn_x", (nx as f32, ny as f32, ox as f32)),
        Uniform::new("turn_y", (mx as f32, my as f32, oy as f32)),
        Uniform::new("turn_size", (dst.size.w as f32, dst.size.h as f32)),
        Uniform::new(
            "turn_aa",
            ((0.5 / bounds.width) as f32, (0.5 / bounds.height) as f32),
        ),
    ]
}

#[cfg(all(test, feature = "e2e"))]
#[path = "turn_tests.rs"]
mod tests;
