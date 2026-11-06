use smithay::{
    backend::renderer::{Renderer, utils::with_renderer_surface_state},
    utils::Rectangle,
};

use super::*;

#[test]
fn scale_commit_without_attachment_changes_actual_cursor_pixels_and_extent() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let white = 0xffff_ffff;
    let pixels = [
        RED, RED, GREEN, GREEN, RED, RED, GREEN, GREEN, BLUE, BLUE, white, white, BLUE, BLUE,
        white, white,
    ];
    let image = fixture.image((4, 4), &pixels, (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&surface, &image);
    fixture.cursor(Some(&surface), (1, 1));
    fixture.app.pointer_location = (10., 10.).into();
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&first), pixels);
    surface.set_buffer_scale(2);
    fixture.sync();
    let pending = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(pending.revision, first.revision);
    // No attach, damage, or client buffer writes: only surface scale is committed.
    surface.commit();
    fixture.sync();
    let scaled = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(scaled.image.size(), (2, 2).into());
    assert_eq!(words(&scaled), vec![RED, GREEN, BLUE, white]);
    assert_eq!(scaled.location, first.location);
    assert_ne!(scaled.revision, first.revision);
    assert_eq!(words(&first), pixels);
}

#[test]
fn transform_commit_without_attachment_changes_asymmetric_pixels_at_same_extent() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let pixels = [RED, GREEN, BLUE, 0xffff_ffff, 0xffff_ff00, 0xff00_0000];
    let image = fixture.image((2, 3), &pixels, (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&surface, &image);
    fixture.cursor(Some(&surface), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    surface.set_buffer_transform(wl_output::Transform::Flipped);
    fixture.sync();
    assert_eq!(
        snapshot(&mut builder, &mut fixture, &view).revision,
        first.revision
    );
    surface.commit();
    fixture.sync();
    let flipped = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(flipped.image.size(), first.image.size());
    assert_eq!(
        words(&flipped),
        vec![GREEN, RED, 0xffff_ffff, BLUE, 0xff00_0000, 0xffff_ff00]
    );
    assert_ne!(flipped.revision, first.revision);
    surface.set_buffer_transform(wl_output::Transform::Normal);
    surface.commit();
    fixture.sync();
    assert_eq!(words(&snapshot(&mut builder, &mut fixture, &view)), pixels);
}

#[test]
fn damage_without_attachment_reaches_renderer_and_invalidates_imported_texture() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let image = fixture.image(
        (2, 2),
        &[RED, GREEN, BLUE, RED],
        (0, 0),
        wl_shm::Format::Argb8888,
    );
    let viewport = fixture.viewport(&surface);
    viewport.set_destination(4, 4);
    fixture.attach(&surface, &image);
    fixture.cursor(Some(&surface), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    let server = fixture.server_surface(&surface);
    let before = with_renderer_surface_state(&server, |state| {
        assert!(state.texture(fixture.renderer.context_id()).is_some());
        state.current_commit()
    })
    .unwrap();
    // This tests damage delivery, not illegal modification of retained SHM.
    // The same pixels are deliberately damaged without reattaching the buffer.
    surface.damage_buffer(1, 0, 1, 2);
    surface.commit();
    fixture.sync();
    with_renderer_surface_state(&server, |state| {
        assert_ne!(state.current_commit(), before);
        assert_eq!(
            state
                .damage_since(Some(before))
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![Rectangle::new((1, 0).into(), (1, 2).into())]
        );
        assert!(state.texture(fixture.renderer.context_id()).is_none());
    })
    .unwrap();
    let damaged = snapshot(&mut builder, &mut fixture, &view);
    assert_ne!(damaged.revision, first.revision);
    assert_eq!(words(&damaged), words(&first));
    with_renderer_surface_state(&server, |state| {
        assert!(state.texture(fixture.renderer.context_id()).is_some());
    })
    .unwrap();
}
