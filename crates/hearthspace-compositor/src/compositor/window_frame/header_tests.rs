//! The resize path paints variable-width material without a CPU image upload.
use super::*;

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn header_material_preserves_translucency_grip_divider_and_top_corner_mask(#[case] scale: i32) {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((-180, -100).into(), (220, 160).into());
    let header = |cache: &mut WindowFrameCache, renderer: &mut GlesRenderer, photo| {
        cache
            .elements(renderer, 1, window, 1.0, false, false, true, photo)
            .unwrap()
            .header
            .unwrap()
    };
    let placed = |element| {
        RescaleRenderElement::from_element(
            RelocateRenderElement::from_element(element, (200, 120), Relocate::Relative),
            (0, 0).into(),
            f64::from(scale),
        )
        .into()
    };
    let normal = header(&mut cache, &mut renderer, false);
    let size = Size::from((280 * scale, 220 * scale));
    let normal_pixels = capture(&mut renderer, size, Transform::Normal, &[placed(normal)]);
    let at = |x, y| pixel(&normal_pixels, size.w, (20 + x) * scale, (20 + y) * scale);
    let clear = &[0, 0, 0, 0];
    assert_eq!(at(0, 0), clear, "the top corner must remain transparent");
    assert_eq!(at(110, 44), clear, "material must not cover the app body");
    assert!(
        at(0, 43)[3] > 150,
        "the bottom edge must meet the app squarely"
    );
    // Outside the radial wash: warm 54% tint, then white 41/255 control tint.
    let base = at(190, 10);
    for (actual, expected) in base.iter().zip([152u8, 152, 149, 157]) {
        assert!(
            actual.abs_diff(expected) <= 2,
            "unexpected header material: {base:?}"
        );
    }
    let grip = at(110, 22);
    assert!(
        grip[0] + 8 < base[0] && grip[3] > base[3],
        "missing muted grip: {grip:?}"
    );
    assert!(
        at(110, 18)[3] < grip[3],
        "the grip must remain four logical pixels high"
    );
    assert!(at(190, 43)[0] + 5 < base[0], "missing one-pixel divider");
    assert!(
        at(30, 10)[3] > base[3],
        "the upper-left orange wash must remain"
    );

    let photo = header(&mut cache, &mut renderer, true);
    let photo_pixels = capture(&mut renderer, size, Transform::Normal, &[placed(photo)]);
    let photo_base = pixel(&photo_pixels, size.w, 210 * scale, 30 * scale);
    assert!(
        photo_base[3] > base[3] + 40 && photo_base[3] < 210,
        "photo material should be more opaque without hiding its backdrop: {photo_base:?}"
    );
}

#[test]
fn header_retains_its_id_on_resize_and_ignores_unrelated_focus_damage() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((40, 40).into(), (220, 160).into());
    let first = cache
        .elements(&mut renderer, 1, window, 1.0, false, false, true, false)
        .unwrap()
        .header
        .unwrap();
    for (active, hovered) in [(false, true), (true, false), (true, true)] {
        let next = cache
            .elements(&mut renderer, 1, window, 1.0, active, hovered, true, false)
            .unwrap()
            .header
            .unwrap();
        assert_eq!(first.id(), next.id());
        assert_eq!(first.current_commit(), next.current_commit());
    }
    let resized = cache
        .elements(
            &mut renderer,
            1,
            Rectangle::new(window.loc, (300, 240).into()),
            1.0,
            true,
            false,
            true,
            false,
        )
        .unwrap()
        .header
        .unwrap();
    assert_eq!(
        first.id(),
        resized.id(),
        "resizing must retain the material element"
    );
    assert_ne!(first.current_commit(), resized.current_commit());
    assert_eq!(resized.geometry(1.0.into()).size, (300, 44).into());
    assert!(
        cache
            .elements(&mut renderer, 1, window, 1.0, true, false, false, false)
            .unwrap()
            .header
            .is_none(),
        "client decorations must not acquire an SSD header"
    );
}
