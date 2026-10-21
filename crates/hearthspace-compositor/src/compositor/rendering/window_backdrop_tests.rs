//! These render real GPU pixels and retain caches across frames. Solid sources
//! keep window ownership, blur damage, and coordinate assertions deterministic.
use smithay::backend::{
    egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
    renderer::{
        element::solid::{SolidColorBuffer, SolidColorRenderElement},
        gles::GlesRenderbuffer,
    },
};

use super::*;

fn renderer() -> GlesRenderer {
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
    let context = EGLContext::new(&display).unwrap();
    unsafe { GlesRenderer::new(context).unwrap() }
}

fn source(
    buffer: &SolidColorBuffer,
    local: (i32, i32),
    scale: i32,
) -> TranslatedHearthspaceRenderElement {
    let element = HearthspaceRenderElement::from(SolidColorRenderElement::from_buffer(
        buffer,
        local,
        1.0,
        1.0,
        Kind::Unspecified,
    ));
    RescaleRenderElement::from_element(
        RelocateRenderElement::from_element(element, (0, 0), Relocate::Relative),
        (0, 0).into(),
        f64::from(scale),
    )
    .into()
}

fn capture(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    elements: &[TranslatedHearthspaceRenderElement],
) -> Vec<u8> {
    let mut framebuffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())
        .unwrap();
    let mut target = renderer.bind(&mut framebuffer).unwrap();
    OutputDamageTracker::new(size, 1.0, Transform::Normal)
        .render_output(
            renderer,
            &mut target,
            0,
            elements,
            Color32F::new(1.0, 0.0, 0.0, 1.0),
        )
        .unwrap();
    crate::compositor::capture::read_pixels(renderer, &target, size, false).unwrap()
}

fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> &[u8] {
    let offset = ((y * width + x) * 4) as usize;
    &pixels[offset..offset + 4]
}

#[test]
fn settings_glass_only_tracks_damage_below_its_owner_and_inside_its_halo() {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::from_size((480, 360).into());
    let request = BackdropRequest {
        id: 1,
        front: 1,
        after: 2,
        window: Rectangle::new((120, 120).into(), (160, 120).into()),
        region: None,
        radius: 19.,
        modal: false,
        zoom: 1.0,
        opacity: 1.,
    };
    let mut above = SolidColorBuffer::new((8, 8), [0.0, 0.0, 1.0, 1.0]);
    let mut owner = SolidColorBuffer::new((160, 120), [0.25, 0.0, 0.0, 0.25]);
    let mut stripe = SolidColorBuffer::new((12, 240), [1.0, 1.0, 1.0, 1.0]);
    let mut distant = SolidColorBuffer::new((8, 8), [0.0, 1.0, 0.0, 1.0]);
    let background = SolidColorBuffer::new((480, 360), [0.1, 0.1, 0.1, 1.0]);
    let scene = |above: &_, owner: &_, stripe: &_, distant: &_| {
        vec![
            source(above, (400, 20), 1),
            source(owner, (120, 120), 1),
            source(distant, (450, 330), 1),
            source(stripe, (104, 40), 1),
            source(&background, (0, 0), 1),
        ]
    };
    let mut first = scene(&above, &owner, &stripe, &distant);
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut first);
    assert_eq!(first.len(), 6, "the GPU backdrop must be inserted");
    let initial_commit = first[2].current_commit();
    let before = capture(&mut renderer, output.size.to_physical(1), &first[2..3]);
    let white_from_halo = pixel(&before, 480, 122, 170)[0];
    assert!(
        white_from_halo > 35,
        "the stripe outside Settings must blur into it"
    );

    let mut steady = scene(&above, &owner, &stripe, &distant);
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut steady);
    assert_eq!(steady[2].current_commit(), initial_commit);
    // Changing the owner and a higher window must not feed back into the blur.
    above.set_color([0.0, 1.0, 0.0, 1.0]);
    owner.set_color([0.0, 0.25, 0.0, 0.25]);
    distant.set_color([0.0, 0.0, 1.0, 1.0]);
    let mut unrelated = scene(&above, &owner, &stripe, &distant);
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut unrelated);
    assert_eq!(unrelated[2].current_commit(), initial_commit);

    stripe.set_color([0.1, 0.1, 0.1, 1.0]);
    let mut changed = scene(&above, &owner, &stripe, &distant);
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut changed);
    assert_ne!(changed[2].current_commit(), initial_commit);
    let after = capture(&mut renderer, output.size.to_physical(1), &changed[2..3]);
    assert!(
        pixel(&after, 480, 122, 170)[0] + 5 < white_from_halo,
        "nearby desktop damage must change the real backdrop pixels"
    );
    let composed = capture(&mut renderer, output.size.to_physical(1), &changed);
    assert_eq!(pixel(&composed, 480, 402, 22), &[0, 255, 0, 255]);
    let body = pixel(&composed, 480, 200, 180);
    assert!(
        body[1] > body[0] + 40,
        "the translucent owner must remain above its blur"
    );
}

#[rstest::rstest]
#[case(1, 1.0)]
#[case(2, 0.5)]
#[case(2, 2.0)]
fn settings_glass_clips_to_outputs_without_dark_edges_or_scaled_mask_drift(
    #[case] scale: i32,
    #[case] zoom: f64,
) {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::new((-400, 50).into(), (240, 180).into());
    let request = BackdropRequest {
        id: 8,
        front: 0,
        after: 0,
        window: Rectangle::new((-420, 30).into(), (160, 120).into()),
        region: None,
        radius: 19.,
        modal: false,
        zoom,
        opacity: 1.,
    };
    let white = SolidColorBuffer::new((240, 180), [1.0, 1.0, 1.0, 1.0]);
    let mut elements = vec![source(&white, (0, 0), scale)];
    cache.render(
        &mut renderer,
        output,
        scale,
        1.35,
        &[request],
        &mut elements,
    );
    assert_eq!(elements.len(), 2);
    let size = output.size.to_physical(scale);
    let actual = capture(&mut renderer, size, &elements[..1]);
    // These boundaries cut through the window, so they must not be rounded or
    // darkened by sampling the clear color outside the physical monitor.
    assert_eq!(pixel(&actual, size.w, 0, 40 * scale), &[255, 255, 255, 255]);
    assert_eq!(pixel(&actual, size.w, 40 * scale, 0), &[255, 255, 255, 255]);
    assert_eq!(
        pixel(&actual, size.w, 140 * scale - 1, 100 * scale - 1),
        &[255, 0, 0, 255],
        "the actual bottom-right corner remains sharply rounded"
    );
    let entry = &cache.renderers[&renderer.context_id()].windows[&(8, output_key(output))];
    assert_eq!(entry.size, (140 * scale, 100 * scale).into());
    assert_eq!(entry.sample_scale, 4.0 * f64::from(scale) * zoom);
}

#[test]
fn overlapping_window_backdrops_are_resolved_back_to_front() {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::from_size((240, 180).into());
    let top = SolidColorBuffer::new((240, 180), [0.0, 0.0, 0.0, 0.0]);
    let lower = SolidColorBuffer::new((240, 180), [0.0, 0.5, 0.0, 0.5]);
    let bottom = SolidColorBuffer::new((240, 180), [0.2, 0.2, 0.2, 1.0]);
    let requests = [
        BackdropRequest {
            id: 1,
            front: 0,
            after: 1,
            window: output,
            region: None,
            radius: 19.,
            modal: false,
            zoom: 1.0,
            opacity: 1.,
        },
        BackdropRequest {
            id: 2,
            front: 1,
            after: 2,
            window: output,
            region: None,
            radius: 19.,
            modal: false,
            zoom: 1.0,
            opacity: 1.,
        },
    ];
    let mut elements = vec![
        source(&top, (0, 0), 1),
        source(&lower, (0, 0), 1),
        source(&bottom, (0, 0), 1),
    ];
    cache.render(&mut renderer, output, 1, 1.35, &requests, &mut elements);
    assert_eq!(elements.len(), 5);
    let actual = capture(&mut renderer, output.size.to_physical(1), &elements);
    let center = pixel(&actual, 240, 120, 90);
    assert!(
        center[1] > center[0] + 100,
        "the upper backdrop must include the lower window's resolved glass"
    );
    assert_eq!(cache.renderers[&renderer.context_id()].windows.len(), 2);
}

#[test]
fn an_opaque_higher_window_skips_blur_and_each_output_keeps_its_own_cache() {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::from_size((240, 180).into());
    let request = BackdropRequest {
        id: 1,
        front: 1,
        after: 1,
        window: Rectangle::new((10, 10).into(), (100, 100).into()),
        region: None,
        radius: 19.,
        modal: false,
        zoom: 1.0,
        opacity: 1.,
    };
    let background = SolidColorBuffer::new((240, 180), [0.2, 0.2, 0.2, 1.0]);
    let mut above = SolidColorBuffer::new((240, 180), [0.0, 0.0, 0.0, 1.0]);
    let mut elements = vec![source(&above, (0, 0), 1), source(&background, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut elements);
    assert_eq!(elements.len(), 2);
    assert!(cache.renderers[&renderer.context_id()].windows.is_empty());

    above.set_color([0.0, 0.0, 0.0, 0.0]);
    let mut elements = vec![source(&above, (0, 0), 1), source(&background, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut elements);
    let other_output = Rectangle::new((-240, 0).into(), output.size);
    let mut other_request = request;
    other_request.window.loc.x -= 240;
    let mut other_elements = vec![source(&above, (0, 0), 2), source(&background, (0, 0), 2)];
    cache.render(
        &mut renderer,
        other_output,
        2,
        1.35,
        &[other_request],
        &mut other_elements,
    );
    assert_eq!(cache.renderers[&renderer.context_id()].windows.len(), 2);
    assert_eq!(elements[1].geometry(1.0.into()).size, (100, 100).into());
    assert_eq!(
        other_elements[1].geometry(1.0.into()).size,
        (200, 200).into()
    );
}

#[test]
fn photographic_material_reduces_saturation_without_waiting_for_source_damage() {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::from_size((120, 100).into());
    let request = BackdropRequest {
        id: 1,
        front: 0,
        after: 0,
        window: output,
        region: None,
        radius: 19.,
        modal: false,
        zoom: 1.0,
        opacity: 1.,
    };
    let background = SolidColorBuffer::new((120, 100), [0.3, 0.5, 0.3, 1.0]);
    let mut normal = vec![source(&background, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut normal);
    let normal_pixels = capture(&mut renderer, output.size.to_physical(1), &normal[..1]);
    let mut photo = vec![source(&background, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.15, &[request], &mut photo);
    assert_ne!(normal[0].current_commit(), photo[0].current_commit());
    let photo_pixels = capture(&mut renderer, output.size.to_physical(1), &photo[..1]);
    let normal_rgb = pixel(&normal_pixels, 120, 60, 50);
    let photo_rgb = pixel(&photo_pixels, 120, 60, 50);
    assert!(normal_rgb[1] - normal_rgb[0] > photo_rgb[1] - photo_rgb[0]);
    assert_eq!(
        cache.renderers[&renderer.context_id()].windows[&(1, output_key(output))].blur_passes,
        1,
        "a saturation change must resolve the existing blur without repeating it"
    );

    let mut steady = vec![source(&background, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.15, &[request], &mut steady);
    assert_eq!(photo[0].current_commit(), steady[0].current_commit());
}

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn server_titlebar_blurs_only_the_header_without_rounding_its_bottom(#[case] scale: i32) {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::from_size((320, 240).into());
    let request = BackdropRequest {
        id: 1,
        front: 0,
        after: 0,
        window: Rectangle::new((40, 40).into(), (220, 160).into()),
        region: Some(Rectangle::new((40, 40).into(), (220, 44).into())),
        radius: 19.,
        modal: false,
        zoom: 1.0,
        opacity: 1.,
    };
    let mut background = SolidColorBuffer::new((320, 240), [1.0, 1.0, 1.0, 1.0]);
    let mut elements = vec![source(&background, (0, 0), scale)];
    cache.render(
        &mut renderer,
        output,
        scale,
        1.35,
        &[request],
        &mut elements,
    );
    assert_eq!(elements.len(), 2);
    let size = output.size.to_physical(scale);
    let pixels = capture(&mut renderer, size, &elements[..1]);
    assert_eq!(
        pixel(&pixels, size.w, 40 * scale, 40 * scale),
        &[255, 0, 0, 255],
        "the real top window corner stays rounded"
    );
    assert_eq!(
        pixel(&pixels, size.w, 40 * scale, 83 * scale),
        &[255, 255, 255, 255],
        "the header meets the app with square bottom corners"
    );
    assert_eq!(
        pixel(&pixels, size.w, 150 * scale, 100 * scale),
        &[255, 0, 0, 255],
        "an ordinary application's body must not acquire a blur surface"
    );
    let entry = &cache.renderers[&renderer.context_id()].windows[&(1, output_key(output))];
    assert_eq!(entry.size, (220 * scale, 44 * scale).into());
    let commit = elements[0].current_commit();
    let mut steady = vec![source(&background, (0, 0), scale)];
    cache.render(&mut renderer, output, scale, 1.35, &[request], &mut steady);
    assert_eq!(steady[0].current_commit(), commit);

    let mut resized_request = request;
    resized_request.window.size.h = 120;
    let mut resized = vec![source(&background, (0, 0), scale)];
    cache.render(
        &mut renderer,
        output,
        scale,
        1.35,
        &[resized_request],
        &mut resized,
    );
    assert_eq!(elements[0].id(), resized[0].id());
    assert_ne!(
        commit,
        resized[0].current_commit(),
        "the changed mask must be resolved"
    );
    assert_eq!(
        cache.renderers[&renderer.context_id()].windows[&(1, output_key(output))].blur_passes,
        1,
        "a height-only resize must reuse the unchanged header blur"
    );
    background.set_color([0.2, 0.2, 0.2, 1.0]);
    let mut changed = vec![source(&background, (0, 0), scale)];
    cache.render(
        &mut renderer,
        output,
        scale,
        1.35,
        &[resized_request],
        &mut changed,
    );
    assert_eq!(
        cache.renderers[&renderer.context_id()].windows[&(1, output_key(output))].blur_passes,
        2,
        "source damage must still recompute the blur"
    );
    let changed_pixels = capture(&mut renderer, size, &changed[..1]);
    assert!(
        pixel(&changed_pixels, size.w, 150 * scale, 60 * scale)[0] < 80,
        "the reused allocation must receive changed backdrop pixels"
    );
}

#[path = "window_backdrop_layout_tests.rs"]
mod layout;
