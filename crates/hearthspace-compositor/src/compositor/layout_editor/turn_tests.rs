//! Exercise the actual texture shader, including output scaling and damage geometry.
use super::*;
use crate::compositor::{capture::read_pixels, rendering::HearthspaceRenderElement};
use smithay::backend::{
    allocator::Fourcc,
    egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
    renderer::{
        Bind, Color32F, Offscreen, Renderer,
        damage::OutputDamageTracker,
        element::{texture::TextureRenderBuffer, utils::RescaleRenderElement},
        gles::GlesRenderbuffer,
    },
};

#[rstest::rstest]
#[case(1, std::f64::consts::FRAC_PI_2)]
#[case(2, -std::f64::consts::FRAC_PI_2)]
#[case(2, std::f64::consts::FRAC_PI_4)]
fn native_toolbar_turn_keeps_pixels_and_pointer_pivot(#[case] scale: i32, #[case] angle: f64) {
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
    let context = EGLContext::new(&display).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context).unwrap() };
    let mut texture: GlesTexture = renderer
        .create_buffer(Fourcc::Abgr8888, (80, 20).into())
        .unwrap();
    {
        let mut target = renderer.bind(&mut texture).unwrap();
        let mut frame = renderer
            .render(&mut target, (80, 20).into(), Transform::Normal)
            .unwrap();
        frame
            .clear(
                Color32F::new(1., 0., 0., 1.),
                &[Rectangle::from_size((40, 20).into())],
            )
            .unwrap();
        frame
            .clear(
                Color32F::new(0., 0., 1., 1.),
                &[Rectangle::new((40, 0).into(), (40, 20).into())],
            )
            .unwrap();
        frame.finish().unwrap().wait().unwrap();
    }
    let buffer = TextureRenderBuffer::from_texture(&renderer, texture, 1, Transform::Normal, None);
    let inner = TextureRenderElement::from_texture_render_buffer(
        (80., 80.),
        &buffer,
        None,
        None,
        None,
        Kind::Unspecified,
    );
    let pivot = Point { x: 90., y: 90. };
    let element = TurnElement::new(
        inner,
        compile(&mut renderer).unwrap(),
        Rect {
            x: 80.,
            y: 80.,
            width: 80.,
            height: 20.,
        },
        pivot,
        angle,
    );
    let elements = [RescaleRenderElement::from_element(
        HearthspaceRenderElement::from(element),
        (0, 0).into(),
        f64::from(scale),
    )];
    let size = (200 * scale, 200 * scale).into();
    let mut target: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (200 * scale, 200 * scale).into())
        .unwrap();
    let mut framebuffer = renderer.bind(&mut target).unwrap();
    OutputDamageTracker::new(size, 1., Transform::Normal)
        .render_output(
            &mut renderer,
            &mut framebuffer,
            0,
            &elements,
            Color32F::new(0., 0., 0., 1.),
        )
        .unwrap();
    let pixels = read_pixels(&mut renderer, &framebuffer, size, false).unwrap();
    let sample = |x: f64, y: f64| {
        let (sin, cos) = angle.sin_cos();
        let px = ((pivot.x + cos * (x - pivot.x) - sin * (y - pivot.y)) * f64::from(scale)).round()
            as usize;
        let py = ((pivot.y + sin * (x - pivot.x) + cos * (y - pivot.y)) * f64::from(scale)).round()
            as usize;
        let offset = (py * 200 * scale as usize + px) * 4;
        &pixels[offset..offset + 4]
    };
    assert_eq!(
        sample(90., 90.),
        [255, 0, 0, 255],
        "the grabbed pixel stays under the pivot"
    );
    assert_eq!(
        sample(145., 90.),
        [0, 0, 255, 255],
        "far-end artwork follows the true turn without cropping"
    );
    assert_eq!(
        sample(120., 50.),
        [0, 0, 0, 255],
        "transparent extent never smears texture edges"
    );
}
