//! Output-filling windows retain an inset accent edge above opaque client pixels.
use super::*;
use hearthspace_ipc::{AccentColor, Appearance, ColorMode};

#[test]
fn focused_rim_fills_four_output_edges_without_rounding_or_covering_content() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((0, 0).into(), (160, 100).into());
    let body = SolidColorBuffer::new((160, 100), [0.2, 0.2, 0.2, 1.0]);
    for color_mode in ColorMode::ALL {
        cache.set_appearance(Appearance {
            color_mode,
            accent_color: AccentColor::Orange,
        });
        let frame = cache
            .elements_with_opacity(
                &mut renderer,
                1,
                window,
                1.,
                true,
                false,
                FrameKind::ServerSide,
                false,
                true,
                1.,
            )
            .unwrap();
        let clipped = frame.clip.unwrap().wrap(
            solid(&body, (0, 0)),
            Some(Color32F::new(0.2, 0.2, 0.2, 1.0)),
        );
        // Same front-to-back order as the compositor: the client must not
        // occlude the edge just because its buffer is opaque.
        let image = capture(
            &mut renderer,
            (160, 100).into(),
            Transform::Normal,
            &[frame.rim.into(), clipped.into()],
        );
        for (x, y) in [
            (80, 0),
            (0, 50),
            (159, 50),
            (80, 99),
            (0, 0),
            (159, 0),
            (0, 99),
            (159, 99),
        ] {
            let rim = pixel(&image, 160, x, y);
            assert!(
                rim[0] > rim[1] + 10,
                "missing inset accent at {x},{y} in {color_mode:?}: {rim:?}"
            );
            assert_eq!(rim[3], 255, "square focused corners must stay filled");
        }
        for (x, y) in [(80, 1), (1, 50), (158, 50), (80, 98), (80, 50)] {
            assert_eq!(
                pixel(&image, 160, x, y),
                &[51, 51, 51, 255],
                "the one-pixel edge must not tint client content at {x},{y}"
            );
        }
    }
}
