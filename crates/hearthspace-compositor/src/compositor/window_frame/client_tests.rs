//! CSD fallback preserves client pixels instead of guessing a corner radius.
use super::*;
use crate::compositor::window_frame::client_shape::for_coverage;
use smithay::wayland::compositor::{RectangleKind, RegionAttributes};

fn client_pixels(rounded: bool, opacity: u8) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let radius = if rounded { 8.0f64 } else { 0.0 };
            let dx = (f64::from(x) + 0.5 - 32.).abs() - (24. - radius);
            let dy = (f64::from(y) + 0.5 - 32.).abs() - (24. - radius);
            let distance = dx.max(0.).hypot(dy.max(0.)) + dx.max(dy).min(0.) - radius;
            let coverage = (0.5 - distance).clamp(0., 1.);
            let alpha = (coverage * f64::from(opacity)).round() as u8;
            pixels.extend([32, 96, 160].map(|color| (color * u16::from(alpha) / 255) as u8));
            pixels.push(alpha);
        }
    }
    pixels
}

fn client_frame(
    cache: &mut WindowFrameCache,
    renderer: &mut GlesRenderer,
    kind: FrameKind,
    window: Rectangle<i32, Logical>,
    zoom: f64,
) -> Option<WindowFrameElements> {
    cache.elements_with_opacity(
        renderer, 1, window, zoom, true, false, kind, false, false, 1.,
    )
}

fn scene(
    frame: Option<WindowFrameElements>,
    client: TestElement,
    popup: &SolidColorBuffer,
    background: &SolidColorBuffer,
) -> Vec<TestElement> {
    let mut elements = vec![solid(popup, (120, 58)).into()];
    if let Some(frame) = &frame {
        assert!(frame.clip.is_none(), "CSD must never acquire an SSD clip");
        assert!(frame.header.is_none());
        elements.push(frame.rim.clone().into());
    }
    elements.push(client);
    if let Some(frame) = frame {
        elements.push(frame.shadow.into());
    }
    elements.push(solid(background, (0, 0)).into());
    elements
}

#[rstest::rstest]
#[case(1., Transform::Normal, Transform::Normal, false)]
#[case(0.5, Transform::_90, Transform::Normal, true)]
#[case(1.25, Transform::Flipped180, Transform::_90, true)]
fn rounded_client_pixels_and_popups_survive_zoom_source_crop_and_transform(
    #[case] zoom: f64,
    #[case] buffer_transform: Transform,
    #[case] output_transform: Transform,
    #[case] cropped: bool,
) {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let buffer = TextureRenderBuffer::from_memory(
        &mut renderer,
        &client_pixels(true, 255),
        Fourcc::Abgr8888,
        (64, 64),
        false,
        1,
        buffer_transform,
        None,
    )
    .unwrap();
    let offset = if cropped { 0 } else { 8 };
    let geometry = Rectangle::new((offset, offset).into(), (48, 48).into());
    let opaque = RegionAttributes {
        rects: vec![(
            RectangleKind::Add,
            Rectangle::new((offset + 8, offset + 8).into(), (32, 32).into()),
        )],
    };
    let size = if cropped { (48, 48) } else { (64, 64) };
    let kind = for_coverage(size.into(), geometry, Some(true), Some(&opaque), None);
    let window = Rectangle::new(
        (
            72 + (f64::from(offset) * zoom).round() as i32,
            52 + (f64::from(offset) * zoom).round() as i32,
        )
            .into(),
        ((48. * zoom).round() as i32, (48. * zoom).round() as i32).into(),
    );
    let client = || {
        let texture = TextureRenderElement::from_texture_render_buffer(
            (72., 52.),
            &buffer,
            None,
            cropped.then(|| Rectangle::new((8., 8.).into(), (48., 48.).into())),
            None,
            Kind::Unspecified,
        );
        RescaleRenderElement::from_element(texture, (72, 52).into(), zoom).into()
    };
    let popup = SolidColorBuffer::new((8, 8), [0., 0., 1., 1.]);
    let background = SolidColorBuffer::new((256, 256), [0.04, 0.04, 0.04, 1.]);
    let expected = capture(
        &mut renderer,
        (256, 256).into(),
        output_transform,
        &scene(None, client(), &popup, &background),
    );
    let frame = client_frame(&mut cache, &mut renderer, kind, window, zoom);
    let actual = capture(
        &mut renderer,
        (256, 256).into(),
        output_transform,
        &scene(frame, client(), &popup, &background),
    );
    assert_eq!(
        actual, expected,
        "uncertain CSD chrome must change no client or scene pixels"
    );
}

#[test]
fn proven_square_client_keeps_its_accent_and_opaque_corners() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let geometry = Rectangle::new((8, 8).into(), (48, 48).into());
    let opaque = RegionAttributes {
        rects: vec![(RectangleKind::Add, geometry)],
    };
    let kind = for_coverage((64, 64).into(), geometry, Some(true), Some(&opaque), None);
    let buffer = TextureRenderBuffer::from_memory(
        &mut renderer,
        &client_pixels(false, 255),
        Fourcc::Abgr8888,
        (64, 64),
        false,
        1,
        Transform::Normal,
        None,
    )
    .unwrap();
    let client = TextureRenderElement::from_texture_render_buffer(
        (72., 52.),
        &buffer,
        None,
        None,
        None,
        Kind::Unspecified,
    );
    let frame = client_frame(
        &mut cache,
        &mut renderer,
        kind,
        Rectangle::new((80, 60).into(), (48, 48).into()),
        1.,
    );
    let popup = SolidColorBuffer::new((8, 8), [0., 0., 1., 1.]);
    let background = SolidColorBuffer::new((256, 256), [0.04, 0.04, 0.04, 1.]);
    let image = capture(
        &mut renderer,
        (256, 256).into(),
        Transform::Normal,
        &scene(frame, client.into(), &popup, &background),
    );
    let rim = pixel(&image, 256, 79, 84);
    assert!(
        rim[0] > rim[1] + 20,
        "square clients retain their accent rim"
    );
    assert_eq!(pixel(&image, 256, 104, 84), &[32, 96, 160, 255]);
    assert_eq!(pixel(&image, 256, 80, 60)[3], 255);
    assert_eq!(pixel(&image, 256, 122, 60), &[0, 0, 255, 255]);
}

#[test]
fn uncertain_translucent_client_keeps_its_original_composition() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let geometry = Rectangle::new((8, 8).into(), (48, 48).into());
    let kind = for_coverage((64, 64).into(), geometry, Some(true), None, None);
    let buffer = TextureRenderBuffer::from_memory(
        &mut renderer,
        &client_pixels(false, 128),
        Fourcc::Abgr8888,
        (64, 64),
        false,
        1,
        Transform::Normal,
        None,
    )
    .unwrap();
    let client = || {
        TextureRenderElement::from_texture_render_buffer(
            (72., 52.),
            &buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .into()
    };
    let popup = SolidColorBuffer::new((8, 8), [0., 0., 1., 1.]);
    let background = SolidColorBuffer::new((256, 256), [0.04, 0.04, 0.04, 1.]);
    let expected = capture(
        &mut renderer,
        (256, 256).into(),
        Transform::Normal,
        &scene(None, client(), &popup, &background),
    );
    let frame = client_frame(
        &mut cache,
        &mut renderer,
        kind,
        Rectangle::new((80, 60).into(), (48, 48).into()),
        1.,
    );
    let actual = capture(
        &mut renderer,
        (256, 256).into(),
        Transform::Normal,
        &scene(frame, client(), &popup, &background),
    );
    assert_eq!(actual, expected);
}

#[test]
fn coverage_changes_clear_old_chrome_without_repainting_an_unchanged_scene() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let size = Size::from((256, 256));
    let window = Rectangle::new((80, 60).into(), (48, 48).into());
    let body = SolidColorBuffer::new((48, 48), [0.2, 0.2, 0.2, 1.]);
    let popup = SolidColorBuffer::new((8, 8), [0., 0., 1., 1.]);
    let background = SolidColorBuffer::new((256, 256), [0.04, 0.04, 0.04, 1.]);
    let mut framebuffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (256, 256).into())
        .unwrap();
    let mut tracker = OutputDamageTracker::new(size, 1., Transform::Normal);
    // Same client pixels throughout: losing or restoring an opaque hint alone
    // must retire or reintroduce the frame, including all of its shadow damage.
    for (kind, age, changed) in [
        (FrameKind::RectangularClient, 0, true),
        (FrameKind::RectangularClient, 1, false),
        (FrameKind::UncertainClient, 1, true),
        (FrameKind::UncertainClient, 1, false),
        (FrameKind::RectangularClient, 1, true),
    ] {
        let frame = client_frame(&mut cache, &mut renderer, kind, window, 1.);
        let elements = scene(frame, solid(&body, (80, 60)).into(), &popup, &background);
        let mut target = renderer.bind(&mut framebuffer).unwrap();
        assert_eq!(
            tracker
                .render_output(
                    &mut renderer,
                    &mut target,
                    age,
                    &elements,
                    Color32F::TRANSPARENT
                )
                .unwrap()
                .damage
                .is_some(),
            changed,
        );
        if kind == FrameKind::UncertainClient {
            let pixels =
                crate::compositor::capture::read_pixels(&mut renderer, &target, size, false)
                    .unwrap();
            assert_eq!(pixel(&pixels, 256, 79, 84), &[10, 10, 10, 255]);
            assert_eq!(pixel(&pixels, 256, 104, 128), &[10, 10, 10, 255]);
        }
    }
}
