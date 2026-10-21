use super::*;

#[test]
fn tooltip_corner_mask_changes_without_reblurring_unchanged_desktop() {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::from_size((240, 180).into());
    let white = SolidColorBuffer::new((240, 180), [1.; 4]);
    let mut request = BackdropRequest {
        id: 1,
        front: 0,
        after: 0,
        window: Rectangle::new((80, 60).into(), (100, 80).into()),
        region: None,
        radius: 19.,
        modal: false,
        zoom: 1.,
        opacity: 1.,
    };
    let mut elements = vec![source(&white, (0, 0), 1)];
    assert_eq!(
        cache.render(&mut renderer, output, 1, 1.35, &[request], &mut elements),
        [1, 2]
    );
    let pixels = capture(&mut renderer, output.size.to_physical(1), &elements[..1]);
    assert_eq!(pixel(&pixels, 240, 84, 64), [255, 0, 0, 255]);
    let key = (request.id, output_key(output));
    let passes = cache.renderers[&renderer.context_id()].windows[&key].blur_passes;
    request.radius = 9.;
    let mut elements = vec![source(&white, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut elements);
    let pixels = capture(&mut renderer, output.size.to_physical(1), &elements[..1]);
    assert_eq!(pixel(&pixels, 240, 84, 64), [255, 255, 255, 255]);
    assert_eq!(
        cache.renderers[&renderer.context_id()].windows[&key].blur_passes,
        passes
    );
}

#[test]
fn modal_background_has_the_reference_narrower_blur_and_square_output_mask() {
    let mut renderer = renderer();
    let mut cache = WindowBackdropCache::default();
    let output = Rectangle::from_size((240, 180).into());
    let white = SolidColorBuffer::new((120, 180), [1.; 4]);
    let black = SolidColorBuffer::new((240, 180), [0., 0., 0., 1.]);
    let mut request = BackdropRequest {
        id: 1,
        front: 0,
        after: 0,
        window: output,
        region: None,
        radius: 0.,
        modal: false,
        zoom: 1.,
        opacity: 1.,
    };
    let mut elements = vec![source(&white, (0, 0), 1), source(&black, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut elements);
    let glass = capture(&mut renderer, output.size.to_physical(1), &elements[..1]);
    request.id = u64::MAX;
    request.modal = true;
    let mut elements = vec![source(&white, (0, 0), 1), source(&black, (0, 0), 1)];
    cache.render(&mut renderer, output, 1, 1.35, &[request], &mut elements);
    let modal = capture(&mut renderer, output.size.to_physical(1), &elements[..1]);
    assert!(pixel(&modal, 240, 90, 90)[0] > pixel(&glass, 240, 90, 90)[0] + 20);
    assert_eq!(pixel(&modal, 240, 0, 0), [255, 255, 255, 255]);
    assert_eq!(pixel(&modal, 240, 239, 179), [0, 0, 0, 255]);
}
