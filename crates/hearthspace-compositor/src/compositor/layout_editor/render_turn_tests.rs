//! A pending reflow holds the last complete GPU composition.
use super::super::turn_cache::TurnCache;
use super::*;
use smithay::backend::{
    egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
    renderer::{
        element::solid::{SolidColorBuffer, SolidColorRenderElement},
        gles::GlesRenderbuffer,
    },
};
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Color32F, Offscreen,
            damage::OutputDamageTracker,
            element::{Kind, utils::RescaleRenderElement},
        },
    },
    utils::Transform,
};

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn toolbar_capture_keeps_the_last_presented_composition_while_native_reflow_is_pending(
    #[case] scale: i32,
) {
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
    cache
        .element(
            &mut renderer,
            "toolbar",
            &[&source],
            Rect {
                x: 80. * factor,
                y: 80. * factor,
                width: 140. * factor,
                height: 20. * factor,
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
    // A pending native resize never draws new/partial input into this texture.
    // Repeated pending frames retain both its pixels and the previously rendered angle.
    cache.move_held(
        &renderer,
        "toolbar",
        scale,
        Point {
            x: 40. * factor,
            y: 20. * factor,
        },
    );
    let first = cache.held(&renderer, "toolbar", scale).unwrap();
    let element = cache.held(&renderer, "toolbar", scale).unwrap();
    use smithay::backend::renderer::element::Element;
    assert_eq!(first.geometry(1.0.into()), element.geometry(1.0.into()));
    assert_eq!(first.current_commit(), element.current_commit());
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
    let x = ((130. + angle.cos() * 110.) * factor).round() as usize;
    let y = ((110. + angle.sin() * 110.) * factor).round() as usize;
    let offset = (y * 400 * scale as usize + x) * 4;
    assert_eq!(
        &pixels[offset..offset + 4],
        [0, 0, 255, 255],
        "the last whole composition stays unchanged while native clients reflow"
    );
}
