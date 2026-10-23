//! Detached glass surfaces need an unclipped halo, never a darkened interior.
use super::*;

#[test]
fn search_panel_shadow_extends_beyond_surface_with_an_empty_rounded_interior() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let panel = Rectangle::new((120, 100).into(), (240, 72).into());
    let shadow = cache
        .panel_shadow(&mut renderer, 1, panel, 1., false, 1.)
        .unwrap();
    let image = capture(
        &mut renderer,
        (480, 320).into(),
        Transform::Normal,
        &[shadow.into()],
    );
    for (x, y) in [(240, 180), (240, 200), (110, 150)] {
        assert!(
            pixel(&image, 480, x, y)[3] > 0,
            "panel halo was clipped to its native buffer at {x},{y}"
        );
    }
    for (x, y) in [(240, 135), (121, 135), (140, 101), (240, 171)] {
        assert_eq!(
            pixel(&image, 480, x, y),
            &[0, 0, 0, 0],
            "glass must not composite its own shadow at {x},{y}"
        );
    }
    assert!(
        pixel(&image, 480, 121, 101)[3] > 0,
        "the external halo follows the rounded corner, not a rectangular cutout"
    );
    assert_eq!(pixel(&image, 480, 240, 319), &[0, 0, 0, 0]);
}

#[test]
fn search_panel_shadow_tracks_emphasis_opacity_and_scaled_geometry() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let panel = Rectangle::new((120, 100).into(), (240, 72).into());
    let idle = cache
        .panel_shadow(&mut renderer, 1, panel, 1., false, 1.)
        .unwrap();
    let unchanged = cache
        .panel_shadow(&mut renderer, 1, panel, 1., false, 1.)
        .unwrap();
    assert_eq!(idle.id(), unchanged.id());
    assert_eq!(idle.current_commit(), unchanged.current_commit());
    let raised = cache
        .panel_shadow(&mut renderer, 1, panel, 1., true, 1.)
        .unwrap();
    assert_eq!(idle.id(), raised.id());
    assert_ne!(idle.current_commit(), raised.current_commit());
    let raised_image = capture(
        &mut renderer,
        (480, 320).into(),
        Transform::Normal,
        &[raised.into()],
    );
    let idle_image = capture(
        &mut renderer,
        (480, 320).into(),
        Transform::Normal,
        &[idle.into()],
    );
    assert_ne!(
        pixel(&idle_image, 480, 240, 190),
        pixel(&raised_image, 480, 240, 190),
        "hover and keyboard selection must update the visible halo"
    );
    let fading = cache
        .panel_shadow(&mut renderer, 1, panel, 1., true, 0.5)
        .unwrap();
    let faded_image = capture(
        &mut renderer,
        (480, 320).into(),
        Transform::Normal,
        &[fading.into()],
    );
    for (full, faded) in
        pixel(&raised_image, 480, 240, 190)
            .iter()
            .zip(pixel(&faded_image, 480, 240, 190))
    {
        assert!((i16::from(*full) - i16::from(*faded) * 2).abs() <= 1);
    }
    let scaled = cache
        .panel_shadow(
            &mut renderer,
            1,
            Rectangle::new((60, 50).into(), (120, 36).into()),
            0.5,
            true,
            1.,
        )
        .unwrap();
    let scaled_image = capture(
        &mut renderer,
        (240, 160).into(),
        Transform::Normal,
        &[scaled.into()],
    );
    assert_eq!(pixel(&scaled_image, 240, 120, 68), &[0, 0, 0, 0]);
    assert!(pixel(&scaled_image, 240, 120, 95)[3] > 0);
    for (full, scaled) in
        pixel(&raised_image, 480, 240, 190)
            .iter()
            .zip(pixel(&scaled_image, 240, 120, 95))
    {
        assert!((i16::from(*full) - i16::from(*scaled)).abs() <= 1);
    }
}
