//! A late native reflow must not crop pixels outside the new toolbar footprint.
use super::*;
use smithay::backend::{
    egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
    renderer::{
        element::solid::{SolidColorBuffer, SolidColorRenderElement},
        gles::GlesRenderbuffer,
    },
};

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn toolbar_capture_preserves_a_native_buffer_pending_reflow(#[case] scale: i32) {
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
    let context = EGLContext::new(&display).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context).unwrap() };
    let blue = SolidColorBuffer::new((140, 20), [0., 0., 1., 1.]);
    let source: TranslatedHearthspaceRenderElement = RescaleRenderElement::from_element(
        RelocateRenderElement::from_element(
            HearthspaceRenderElement::from(SolidColorRenderElement::from_buffer(
                &blue,
                (80, 80),
                1.,
                1.,
                Kind::Unspecified,
            )),
            (0, 0),
            Relocate::Relative,
        ),
        (0, 0).into(),
        f64::from(scale),
    )
    .into();
    let mut cache = TurnCache::default();
    let angle = std::f64::consts::FRAC_PI_4;
    let factor = f64::from(scale);
    let element = cache
        .element(
            &mut renderer,
            "toolbar",
            &[&source],
            Rect {
                x: 80. * factor,
                y: 80. * factor,
                width: 54. * factor,
                height: 180. * factor,
            },
            Point {
                x: 90. * factor,
                y: 90. * factor,
            },
            angle,
            scale,
        )
        .unwrap()
        .unwrap();
    let elements = [HearthspaceRenderElement::from(element)];
    let size = (400 * scale, 400 * scale).into();
    let mut buffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (400 * scale, 400 * scale).into())
        .unwrap();
    let mut target = renderer.bind(&mut buffer).unwrap();
    OutputDamageTracker::new(size, 1., Transform::Normal)
        .render_output(
            &mut renderer,
            &mut target,
            0,
            &elements,
            Color32F::new(0., 0., 0., 1.),
        )
        .unwrap();
    let pixels =
        crate::compositor::capture::read_pixels(&mut renderer, &target, size, false).unwrap();
    let x = ((90. + angle.cos() * 110.) * factor).round() as usize;
    let y = ((90. + angle.sin() * 110.) * factor).round() as usize;
    let offset = (y * 400 * scale as usize + x) * 4;
    assert_eq!(
        &pixels[offset..offset + 4],
        [0, 0, 255, 255],
        "the old buffer's far end survives until its vertical reflow commits"
    );
}
