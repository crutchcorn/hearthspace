//! Capture uses the native helper, real scene rendering and two EGL buffers.
//! Only GBM acquisition/presentation is substituted with explicit buffer ages.
use super::{Fixture, Output, point, same_pixels};
use hearthspace_ipc::BackgroundMode;
use smithay::input::pointer::CursorIcon;

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn cursorless_capture_preserves_scene_damage_for_the_next_rotating_buffer(#[case] scale: i32) {
    let mut f = Fixture::new();
    f.app.background_mode = BackgroundMode::RepeatWithCanvas;
    let mut output = Output::new(&mut f, 40, scale);
    let mut spare = Output::new(&mut f, 40, scale);
    let mut other_output = Output::new(&mut f, 232, 1);
    point(&mut f, &other_output, 55., 40.);
    f.named(CursorIcon::Default);
    assert!(other_output.render(&mut f, 0).software_cursor);

    // Initialize A, then B, without changing the scene. Both slots contain the
    // old background; the output's tracker now has enough history for age two.
    let baseline = output.render(&mut f, 0);
    assert!(!baseline.software_cursor);
    output.rotate_buffer(&mut spare);
    let initialized_b = output.render(&mut f, 0);
    same_pixels(
        &initialized_b.pixels,
        &baseline.pixels,
        "initial slots differ",
    );
    output.rotate_buffer(&mut spare);

    // A pan changes scene content during this capture frame. The pointer stays
    // on the other output, so hiding/restoring it cannot add incidental damage
    // that would conceal a missing background update in scanout history.
    f.app.viewport_offset.x += 7;
    let clean = output.cursorless_capture(&mut f);
    assert!(f.app.software_cursor_visible);
    assert!(!clean.software_cursor);
    assert!(
        clean.pixels != baseline.pixels,
        "pan must change scene pixels"
    );
    let reference = output.reference(&mut f);
    same_pixels(
        &clean.pixels,
        &reference.pixels,
        "capture lost the scene change",
    );
    let restored = output.render(&mut f, 0);
    assert!(!restored.software_cursor);
    same_pixels(
        &restored.pixels,
        &reference.pixels,
        "restoration lost scene pixels",
    );

    // One buffer was presented, despite two render passes. B has age two. If
    // capture used the scanout tracker, restoration would push empty newdamage
    // and B would retain the old dots. Check pixels before damage to expose it.
    output.rotate_buffer(&mut spare);
    let reused_b = output.render(&mut f, 2);
    same_pixels(
        &reused_b.pixels,
        &reference.pixels,
        "age-two slot missed captured scene change",
    );
    assert!(!reused_b.damage.is_empty());
    assert!(!reused_b.software_cursor);

    // A already contains the restored scene. It needs no unrelated repaint on
    // its next turn, and no cursor is rendered on this output at either age.
    output.rotate_buffer(&mut spare);
    let reused_a = output.render(&mut f, 2);
    same_pixels(
        &reused_a.pixels,
        &reference.pixels,
        "restored slot changed on reuse",
    );
    assert!(reused_a.damage.is_empty());
    assert!(!reused_a.software_cursor);
}
