//! Real App cursor elements and pixels on persistent and rotating framebuffers.
//! Requires surfaceless EGL/GLES (software Mesa is sufficient), not DRM/KMS.
//! The native backend's force decision is outside this test boundary; these
//! regressions establish that its normal buffer age preserves cursor correctness.
use smithay::{
    input::pointer::CursorIcon,
    utils::{Logical, Point, Rectangle},
};

#[cfg(feature = "udev")]
#[path = "cursor_damage_tests/capture.rs"]
mod capture;
#[path = "cursor_damage_tests/client.rs"]
mod client;
#[path = "cursor_damage_tests/fixture.rs"]
mod fixture;
use fixture::{Fixture, Frame, Output};

fn point(f: &mut Fixture, output: &Output, x: f64, y: f64) {
    f.app.pointer_location = output.rect.loc.to_f64() + Point::<f64, Logical>::from((x, y));
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((x, y).into(), (w, h).into())
}

fn same_pixels(actual: &[u8], expected: &[u8], message: &str) {
    assert_eq!(actual.len(), expected.len());
    let difference = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(
        difference.is_none(),
        "{message}: first differing byte {difference:?}"
    );
}

fn incremental(f: &mut Fixture, output: &mut Output, region: Rectangle<i32, Logical>) -> Frame {
    let frame = updated(f, output);
    assert!(frame.software_cursor);
    let allowed = region.to_physical(output.scale);
    assert!(
        frame
            .damage
            .iter()
            .all(|damage| allowed.contains_rect(*damage)),
        "cursor update invalidated unrelated pixels: {:?}, allowed {allowed:?}",
        frame.damage,
    );
    frame
}

fn updated(f: &mut Fixture, output: &mut Output) -> Frame {
    let frame = output.render(f, 1);
    let reference = output.reference(f);
    same_pixels(
        &frame.pixels,
        &reference.pixels,
        "incremental cursor left stale pixels",
    );
    assert!(
        !frame.damage.is_empty(),
        "cursor update must damage the output"
    );
    frame
}

fn pixel<'a>(frame: &'a Frame, output: &Output, x: i32, y: i32) -> &'a [u8] {
    let width = output.rect.size.w * output.scale;
    let offset = ((y * output.scale * width + x * output.scale) * 4) as usize;
    &frame.pixels[offset..offset + 4]
}

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn client_cursor_damage_tracks_motion_image_hotspot_and_hide(#[case] scale: i32) {
    let mut f = Fixture::new();
    let mut output = Output::new(&mut f, 40, scale);
    let clean = output.render(&mut f, 0);
    assert!(!clean.software_cursor);
    let unchanged = output.render(&mut f, 1);
    assert!(unchanged.damage.is_empty());
    assert!(!unchanged.software_cursor);

    point(&mut f, &output, 40., 40.);
    f.image([255, 0, 0, 255]);
    f.hotspot(3, 2);
    // Smithay compares absolute element z-indices. Adding/removing the cursor
    // also changes the background's index, so visibility transitions can damage
    // this whole output. Steady motion/image/hotspot updates must stay local.
    let shown = updated(&mut f, &mut output);
    assert!(shown.software_cursor);
    assert_eq!(pixel(&shown, &output, 40, 40), &[255, 0, 0, 255]);
    let unchanged = output.render(&mut f, 1);
    assert!(unchanged.damage.is_empty());
    assert!(unchanged.software_cursor);

    point(&mut f, &output, 100., 40.);
    let moved = incremental(&mut f, &mut output, rect(37, 38, 76, 12));
    assert_eq!(
        pixel(&moved, &output, 40, 40),
        pixel(&clean, &output, 40, 40)
    );
    assert_eq!(pixel(&moved, &output, 100, 40), &[255, 0, 0, 255]);

    // Recommit the same wl_surface at the same position: damage must follow
    // the surface's commit, not just its ID or screen geometry.
    f.image([0, 0, 255, 255]);
    let changed = incremental(&mut f, &mut output, rect(97, 38, 16, 12));
    assert_eq!(pixel(&changed, &output, 100, 40), &[0, 0, 255, 255]);

    // A hotspot-only wl_pointer request moves the existing committed image.
    f.hotspot(11, 8);
    let hotspot = incremental(&mut f, &mut output, rect(89, 32, 24, 18));
    assert_eq!(pixel(&hotspot, &output, 90, 33), &[0, 0, 255, 255]);
    assert_eq!(
        pixel(&hotspot, &output, 110, 45),
        pixel(&clean, &output, 110, 45)
    );

    f.hide();
    let hidden = updated(&mut f, &mut output);
    assert!(!hidden.software_cursor);
    same_pixels(
        &hidden.pixels,
        &clean.pixels,
        "hidden client cursor was not erased",
    );
    let unchanged = output.render(&mut f, 1);
    assert!(unchanged.damage.is_empty());
    assert!(!unchanged.software_cursor);
}

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn destroyed_client_cursor_falls_back_to_named_image_with_local_damage(#[case] scale: i32) {
    let mut f = Fixture::new();
    let mut output = Output::new(&mut f, 40, scale);
    let clean = output.render(&mut f, 0);
    point(&mut f, &output, 40., 40.);
    f.image([255, 0, 0, 255]);
    f.hotspot(3, 2);
    updated(&mut f, &mut output);

    f.cursor.destroy();
    f.sync();
    let fallback = incremental(&mut f, &mut output, rect(24, 24, 48, 48));
    assert!(
        f.app.cursor_surface.is_none(),
        "rendering must retire the destroyed cursor"
    );
    assert!(
        fallback.pixels != clean.pixels,
        "named fallback must be visible"
    );
    let unchanged = output.render(&mut f, 1);
    assert!(unchanged.damage.is_empty());
    assert!(unchanged.software_cursor);

    point(&mut f, &output, 100., 40.);
    incremental(&mut f, &mut output, rect(24, 24, 108, 48));
    // Named image replacement changes both artwork and the packaged hotspot.
    f.named(CursorIcon::Grabbing);
    incremental(&mut f, &mut output, rect(76, 16, 48, 48));
    f.hide();
    let hidden = updated(&mut f, &mut output);
    assert!(!hidden.software_cursor);
    same_pixels(&hidden.pixels, &clean.pixels, "named cursor was not erased");
}

#[rstest::rstest]
#[case::client_surface(true)]
#[case::named_fallback(false)]
fn cursor_crossing_outputs_erases_old_pixels_and_damages_only_the_new_output(
    #[case] client_surface: bool,
) {
    let mut f = Fixture::new();
    let mut left = Output::new(&mut f, 40, 1);
    let mut right = Output::new(&mut f, 232, 2);
    let left_clean = left.render(&mut f, 0);
    let right_clean = right.render(&mut f, 0);
    point(&mut f, &left, 190., 40.);
    if client_surface {
        f.image([0, 0, 255, 255]);
        f.hotspot(3, 2);
    } else {
        f.named(CursorIcon::Default);
    }
    // The owner output clips an image crossing its right edge. An unaffected
    // output with a different density must retain both pixels and empty damage.
    assert!(updated(&mut f, &mut left).software_cursor);
    let unaffected = right.render(&mut f, 1);
    assert!(unaffected.damage.is_empty());
    assert!(!unaffected.software_cursor);
    same_pixels(
        &unaffected.pixels,
        &right_clean.pixels,
        "other output changed",
    );

    point(&mut f, &right, 1., 40.);
    let erased = updated(&mut f, &mut left);
    assert!(!erased.software_cursor);
    same_pixels(
        &erased.pixels,
        &left_clean.pixels,
        "old output retained cursor pixels",
    );
    let entered = updated(&mut f, &mut right);
    assert!(entered.software_cursor);
    assert!(entered.pixels != right_clean.pixels);

    point(&mut f, &right, 55., 40.);
    incremental(&mut f, &mut right, rect(0, 16, 88, 56));
    let unaffected = left.render(&mut f, 1);
    assert!(unaffected.damage.is_empty());
    assert!(!unaffected.software_cursor);
    f.hide();
    let hidden = updated(&mut f, &mut right);
    assert!(!hidden.software_cursor);
    same_pixels(
        &hidden.pixels,
        &right_clean.pixels,
        "new output retained cursor pixels",
    );
}

#[cfg(feature = "udev")]
#[test]
fn explicit_invalidation_and_cursorless_pass_restore_the_full_visible_scene() {
    let mut f = Fixture::new();
    let mut output = Output::new(&mut f, 40, 2);
    let clean = output.render(&mut f, 0);
    point(&mut f, &output, 40., 40.);
    f.image([255, 0, 0, 255]);
    f.hotspot(3, 2);
    let visible = updated(&mut f, &mut output);

    // Native cursorless capture temporarily renders without the software
    // cursor, then explicitly restores the complete visible scene at age zero.
    let without = output.cursorless_capture(&mut f);
    let full = Rectangle::from_size(output.rect.size.to_physical(output.scale));
    assert_eq!(without.damage, vec![full]);
    assert!(!without.software_cursor);
    assert!(f.app.software_cursor_visible);
    same_pixels(
        &without.pixels,
        &clean.pixels,
        "cursorless pixels contain a cursor",
    );
    let restored = output.render(&mut f, 0);
    assert_eq!(restored.damage, vec![full]);
    assert!(restored.software_cursor);
    same_pixels(
        &restored.pixels,
        &visible.pixels,
        "capture restoration lost the cursor",
    );
    let unchanged = output.render(&mut f, 1);
    assert!(unchanged.damage.is_empty());
    assert!(unchanged.software_cursor);
    point(&mut f, &output, 100., 40.);
    incremental(&mut f, &mut output, rect(37, 38, 76, 12));
}
