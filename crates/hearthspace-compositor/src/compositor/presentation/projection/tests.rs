use super::*;
use crate::compositor::canvas_preview::Pose;
use std::time::{Duration, Instant};

fn base(scale: f64, inset: (f64, f64)) -> Projection {
    // Visible geometry starts at (300, 200), independently of shadow insets.
    Projection {
        origin: (300. - inset.0 * scale, 200. - inset.1 * scale).into(),
        bounds: Rectangle::new(
            (300, 200).into(),
            ((640. * scale) as i32, (480. * scale) as i32).into(),
        ),
        scale,
        opacity: 1.,
        preview: None,
    }
}

#[test]
fn preview_inverse_transform_preserves_csd_insets_at_fractional_zoom() {
    let base = base(1.25, (8., 6.));
    let pose = Rect {
        x: 97.25,
        y: -32.5,
        width: 360.,
        height: 270.,
    };
    let projected = base.with_preview(pose, 0.4, false);
    let visible_origin = projected.surface_to_screen((8., 6.).into());
    assert_eq!(visible_origin, (97.25, -32.5).into());
    assert_eq!(projected.scale, 0.5625);
    assert_eq!(
        projected.bounds,
        Rectangle::new((97, -33).into(), (360, 270).into())
    );
    assert_eq!(projected.opacity, 0.4_f32);
    for point in [(0., 0.), (8., 6.), (328., 246.), (648., 486.)] {
        let local = Point::from(point);
        let roundtrip = projected.screen_to_surface(projected.surface_to_screen(local));
        assert!((roundtrip.x - local.x).abs() < 1e-10);
        assert!((roundtrip.y - local.y).abs() < 1e-10);
    }
}

#[test]
fn ssd_content_offset_and_opacity_use_the_same_fixed_animation_sample() {
    let now = Instant::now();
    let base = base(0.75, (0., -32.));
    let pose = Pose {
        from: Rect {
            x: 300.,
            y: 200.,
            width: 480.,
            height: 360.,
        },
        to: Rect {
            x: 40.,
            y: 60.,
            width: 240.,
            height: 180.,
        },
        from_opacity: 1.,
        to_opacity: 0.3,
        started: now,
        natural: Rect::default(),
        matches: true,
        reveal: false,
    };
    for time in [0, 45, 110, 220, 360, 900] {
        let (bounds, opacity) = pose.current(now + Duration::from_millis(time));
        let projection = base.with_preview(bounds, opacity, false);
        assert_eq!(projection.opacity, opacity as f32);
        assert!((projection.origin.y - bounds.y - 32. * projection.scale).abs() < 1e-10);
        assert_eq!(projection.bounds, rounded_bounds(bounds));
        let local = projection.screen_to_surface(projection.origin);
        assert_eq!(local, (0., 0.).into());
    }
    // Sampling is a view operation; canonical placement remains untouched.
    assert_eq!(base.bounds.loc, (300, 200).into());
}

#[test]
fn search_result_projection_retains_its_native_240_pixel_width() {
    let projection = base(1., (0., 0.)).with_preview(
        Rect {
            x: -100.,
            y: 80.,
            width: 180.,
            height: 135.,
        },
        1.,
        true,
    );
    assert_eq!(projection.scale, 0.75);
    assert_eq!(
        projection.surface_to_screen((240., 0.).into()),
        (80., 80.).into()
    );
}
