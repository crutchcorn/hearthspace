use super::*;
use smithay::backend::renderer::{
    element::Id,
    utils::{CommitCounter, OpaqueRegions},
};

struct Surface {
    id: Id,
    alpha: f32,
    transform: Transform,
    src: Rectangle<f64, smithay::utils::Buffer>,
    opaque: Vec<Rectangle<i32, Physical>>,
}
impl Default for Surface {
    fn default() -> Self {
        Self {
            id: Id::new(),
            alpha: 1.,
            transform: Transform::Normal,
            src: Rectangle::from_size((1920, 1080).into()).to_f64(),
            opaque: vec![Rectangle::from_size((1920, 1080).into())],
        }
    }
}
impl Element for Surface {
    fn id(&self) -> &Id {
        &self.id
    }
    fn current_commit(&self) -> CommitCounter {
        CommitCounter::default()
    }
    fn src(&self) -> Rectangle<f64, smithay::utils::Buffer> {
        self.src
    }
    fn geometry(&self, _: smithay::utils::Scale<f64>) -> Rectangle<i32, Physical> {
        Rectangle::from_size((1920, 1080).into())
    }
    fn alpha(&self) -> f32 {
        self.alpha
    }
    fn transform(&self) -> Transform {
        self.transform
    }
    fn opaque_regions(&self, _: smithay::utils::Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.opaque.iter().copied().collect()
    }
}
fn eligible(surface: &Surface) -> bool {
    let output = Rectangle::from_size((1920, 1080).into());
    native_opaque_surface(surface, output, output, (1920, 1080).into())
}
#[test]
fn opacity_must_cover_the_entire_surface_without_holes() {
    let mut surface = Surface::default();
    assert!(eligible(&surface));
    surface.opaque = vec![
        Rectangle::from_size((960, 1080).into()),
        Rectangle::new((960, 0).into(), (960, 1080).into()),
    ];
    assert!(eligible(&surface));
    surface.opaque[1].loc.x += 1;
    assert!(!eligible(&surface));
    surface.opaque = vec![Rectangle::from_size((1920, 1080).into())];
    surface.alpha = 0.9;
    assert!(!eligible(&surface));
}
#[test]
fn crop_scale_transform_and_output_mismatch_require_composition() {
    let output = Rectangle::from_size((1920, 1080).into());
    let mut surface = Surface::default();
    surface.src.size.w -= 1.;
    assert!(!eligible(&surface));
    surface.src.size.w += 1.;
    surface.transform = Transform::_180;
    assert!(!eligible(&surface));
    surface.transform = Transform::Normal;
    assert!(!native_opaque_surface(
        &surface,
        output,
        output,
        (3840, 2160).into()
    ));
    let offset = Rectangle::new((1, 0).into(), output.size);
    assert!(!native_opaque_surface(
        &surface,
        offset,
        output,
        (1920, 1080).into()
    ));
}
