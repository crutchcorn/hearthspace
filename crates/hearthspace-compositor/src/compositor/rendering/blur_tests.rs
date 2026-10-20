//! Keep the framebuffer and its age-one damage tracker alive across updates:
//! a fresh screenshot tracker would conceal stale incremental blur damage.
use smithay::{
    backend::{
        egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{
            element::{
                Element,
                solid::{SolidColorBuffer, SolidColorRenderElement},
                utils::RescaleRenderElement,
            },
            gles::GlesRenderbuffer,
        },
    },
    utils::Point,
};

use super::*;
use crate::compositor::rendering::HearthspaceRenderElement;

fn source(
    buffer: &SolidColorBuffer,
    local: (i32, i32),
    output: Point<i32, Physical>,
    scale: i32,
) -> TranslatedHearthspaceRenderElement {
    let element = HearthspaceRenderElement::from(SolidColorRenderElement::from_buffer(
        buffer,
        output + Point::<i32, Physical>::from(local),
        1.0,
        1.0,
        Kind::Unspecified,
    ));
    RescaleRenderElement::from_element(
        RelocateRenderElement::from_element(element, (-output.x, -output.y), Relocate::Relative),
        (0, 0).into(),
        f64::from(scale),
    )
    .into()
}

fn incremental_blur(scale: i32, output: Point<i32, Physical>) {
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
    let context = EGLContext::new(&display).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context).unwrap() };
    let mut cache = CornerBlurCache::default();
    let size = Size::<i32, Physical>::from((128 * scale, 128 * scale));
    let mut framebuffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())
        .unwrap();
    let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
    let corner = CornerRegion {
        id: 1,
        rect: Rectangle::new(
            (30 * scale, 30 * scale).into(),
            (48 * scale, 48 * scale).into(),
        ),
    };
    let background = SolidColorBuffer::new((128, 128), [0.0, 0.0, 0.0, 1.0]);
    // This white stripe is outside the visible corner but inside its blur halo.
    let mut stripe = SolidColorBuffer::new((4, 128), [1.0, 1.0, 1.0, 1.0]);
    let mut distant = SolidColorBuffer::new((8, 8), [0.0, 1.0, 0.0, 1.0]);
    let scene = |stripe: &SolidColorBuffer, distant: &SolidColorBuffer| {
        vec![
            source(distant, (110, 110), output, scale),
            source(stripe, (25, 0), output, scale),
            source(&background, (0, 0), output, scale),
        ]
    };
    let first = cache.render(&mut renderer, &[corner], scale, &scene(&stripe, &distant));
    assert_eq!(first.len(), 1, "GPU blur must be available");
    let mut target = renderer.bind(&mut framebuffer).unwrap();
    assert!(
        tracker
            .render_output(&mut renderer, &mut target, 0, &first, [1.0, 0.0, 0.0, 1.0])
            .unwrap()
            .damage
            .is_some()
    );
    let before =
        crate::compositor::capture::read_pixels(&mut renderer, &target, size, true).unwrap();
    drop(target);

    let same = cache.render(&mut renderer, &[corner], scale, &scene(&stripe, &distant));
    assert_eq!(first[0].current_commit(), same[0].current_commit());
    let mut target = renderer.bind(&mut framebuffer).unwrap();
    assert!(
        tracker
            .render_output(&mut renderer, &mut target, 1, &same, [1.0, 0.0, 0.0, 1.0])
            .unwrap()
            .damage
            .is_none(),
        "a stationary desktop must reuse the blur without output damage"
    );
    drop(target);

    distant.set_color([0.0, 0.0, 1.0, 1.0]);
    let unchanged = cache.render(&mut renderer, &[corner], scale, &scene(&stripe, &distant));
    assert_eq!(
        same[0].current_commit(),
        unchanged[0].current_commit(),
        "updates outside the padded capture must not recompute the blur"
    );

    stripe.set_color([0.0, 0.0, 0.0, 1.0]);
    let changed = cache.render(&mut renderer, &[corner], scale, &scene(&stripe, &distant));
    assert_ne!(
        unchanged[0].current_commit(),
        changed[0].current_commit(),
        "updates outside the corner but inside its halo must invalidate it"
    );
    let mut target = renderer.bind(&mut framebuffer).unwrap();
    let damage = tracker
        .render_output(
            &mut renderer,
            &mut target,
            1,
            &changed,
            [1.0, 0.0, 0.0, 1.0],
        )
        .unwrap()
        .damage
        .cloned()
        .unwrap();
    assert!(damage.iter().any(|rect| rect.overlaps(corner.rect)));
    assert!(damage.iter().all(|rect| corner.rect.contains_rect(*rect)));
    let after =
        crate::compositor::capture::read_pixels(&mut renderer, &target, size, true).unwrap();
    assert_ne!(
        before, after,
        "incremental presentation must change actual pixels"
    );
}

#[test]
fn nearby_desktop_damage_updates_glass_without_redrawing_a_static_scene() {
    incremental_blur(1, (0, 0).into());
}

#[test]
fn glass_damage_uses_output_local_pixels_on_scaled_negative_origin_outputs() {
    incremental_blur(2, (-400, 50).into());
}
