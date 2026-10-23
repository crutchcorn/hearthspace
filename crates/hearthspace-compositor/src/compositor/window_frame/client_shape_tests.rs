use super::*;

fn window() -> Rectangle<i32, Logical> {
    Rectangle::new((8, 12).into(), (160, 100).into())
}

fn classify(has_alpha: Option<bool>, opaque: Option<&RegionAttributes>) -> FrameKind {
    for_coverage((180, 130).into(), window(), has_alpha, opaque, None)
}

#[test]
fn an_opaque_buffer_format_establishes_square_client_coverage() {
    assert_eq!(classify(Some(false), None), FrameKind::RectangularClient);
}

#[test]
fn alpha_capability_or_missing_format_information_does_not_establish_shape() {
    assert_eq!(classify(Some(true), None), FrameKind::UncertainClient);
    assert_eq!(classify(None, None), FrameKind::UncertainClient);
}

#[test]
fn opaque_region_union_can_establish_the_entire_client_rectangle() {
    let region = RegionAttributes {
        rects: vec![
            (
                RectangleKind::Add,
                Rectangle::new((8, 12).into(), (80, 100).into()),
            ),
            (
                RectangleKind::Add,
                Rectangle::new((88, 12).into(), (80, 100).into()),
            ),
        ],
    };
    assert_eq!(
        classify(Some(true), Some(&region)),
        FrameKind::RectangularClient
    );
}

#[test]
fn gtk_corner_cutouts_never_become_a_square_or_inferred_rounded_frame() {
    let mut region = RegionAttributes {
        rects: vec![(RectangleKind::Add, window())],
    };
    assert_eq!(
        classify(Some(true), Some(&region)),
        FrameKind::RectangularClient
    );
    // The same buffer and window geometry can receive a region-only update.
    region.rects.push((
        RectangleKind::Subtract,
        Rectangle::new((8, 12).into(), (12, 12).into()),
    ));
    assert_eq!(
        classify(Some(true), Some(&region)),
        FrameKind::UncertainClient
    );
}

#[test]
fn later_region_operations_override_earlier_corner_cutouts() {
    let corner = Rectangle::new((8, 12).into(), (12, 12).into());
    let region = RegionAttributes {
        rects: vec![
            (RectangleKind::Add, window()),
            (RectangleKind::Subtract, corner),
            (RectangleKind::Add, corner),
        ],
    };
    assert_eq!(
        classify(Some(true), Some(&region)),
        FrameKind::RectangularClient
    );
}

#[test]
fn conservative_inner_rectangle_is_not_a_visible_outline() {
    let region = RegionAttributes {
        rects: vec![(
            RectangleKind::Add,
            Rectangle::new((16, 20).into(), (144, 84).into()),
        )],
    };
    assert_eq!(
        classify(Some(true), Some(&region)),
        FrameKind::UncertainClient
    );
}

#[test]
fn shadow_margin_operations_outside_window_geometry_do_not_remove_coverage() {
    let region = RegionAttributes {
        rects: vec![
            (RectangleKind::Add, window()),
            (
                RectangleKind::Subtract,
                Rectangle::new((0, 0).into(), (8, 130).into()),
            ),
        ],
    };
    assert_eq!(
        classify(Some(true), Some(&region)),
        FrameKind::RectangularClient
    );
}

#[test]
fn client_alpha_modifier_requires_omission_even_with_opaque_format_and_hints() {
    let region = RegionAttributes {
        rects: vec![(RectangleKind::Add, window())],
    };
    assert_eq!(
        for_coverage(
            (180, 130).into(),
            window(),
            Some(false),
            Some(&region),
            Some(u32::MAX - 1),
        ),
        FrameKind::UncertainClient,
    );
    assert_eq!(
        for_coverage(
            (180, 130).into(),
            window(),
            Some(false),
            None,
            Some(u32::MAX)
        ),
        FrameKind::RectangularClient,
    );
}

#[test]
fn committed_surface_view_must_cover_the_geometry_after_viewport_or_transform() {
    assert_eq!(
        for_coverage((160, 100).into(), window(), Some(false), None, None),
        FrameKind::UncertainClient,
    );
    assert_eq!(
        for_coverage(
            (180, 130).into(),
            Rectangle::zero(),
            Some(false),
            None,
            None
        ),
        FrameKind::UncertainClient,
    );
}

#[test]
fn complex_regions_fall_back_without_unbounded_per_frame_work() {
    let region = RegionAttributes {
        rects: vec![(RectangleKind::Add, window()); 65],
    };
    assert_eq!(
        classify(Some(true), Some(&region)),
        FrameKind::UncertainClient
    );
}
