use super::*;

#[test]
fn callback_only_commit_reuses_image_but_replacement_changes_pixels() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let red = fixture.image((2, 2), &[RED; 4], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&surface, &red);
    fixture.cursor(Some(&surface), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    surface.frame(&fixture.queue(), false);
    surface.commit();
    fixture.sync();
    let callback = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(callback.revision, first.revision);
    assert_eq!(callback.image.as_ptr(), first.image.as_ptr());
    let blue = fixture.image((2, 2), &[BLUE; 4], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&surface, &blue);
    let replaced = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&replaced), vec![BLUE; 4]);
    assert_eq!(words(&first), vec![RED; 4]);
    assert_ne!(replaced.revision, first.revision);
}

#[test]
fn synchronized_tree_uses_applied_content_positions_and_stacking() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let sub = fixture.subsurface(&child, &root);
    sub.set_position(-1, 0);
    let red = fixture.image((2, 2), &[RED; 4], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image((2, 2), &[BLUE; 4], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &blue);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (1, 0));
    fixture.app.pointer_location = (10., 10.).into();
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(first.image.size(), (3, 2).into());
    assert_eq!(first.location, (8, 10).into());
    assert_eq!(words(&first), vec![BLUE, BLUE, RED, BLUE, BLUE, RED]);
    let green = fixture.image((2, 2), &[GREEN; 4], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &green);
    let pending = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(pending.revision, first.revision);
    assert_eq!(pending.image.as_ptr(), first.image.as_ptr());
    root.commit();
    fixture.sync();
    let applied = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&applied), vec![GREEN, GREEN, RED, GREEN, GREEN, RED]);
    assert_ne!(applied.revision, first.revision);
    sub.place_below(&root);
    root.commit();
    fixture.sync();
    let below = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&below), vec![GREEN, RED, RED, GREEN, RED, RED]);
    assert_ne!(below.revision, applied.revision);
    sub.set_position(2, 0);
    root.commit();
    fixture.sync();
    let moved = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(moved.image.size(), (4, 2).into());
    assert_eq!(moved.location, (9, 10).into());
    assert_eq!(
        words(&moved),
        vec![RED, RED, GREEN, GREEN, RED, RED, GREEN, GREEN]
    );
}

#[test]
fn named_fallback_output_ownership_and_reset_preserve_snapshot_independence() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let image = fixture.image((1, 1), &[RED], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&root, &image);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let client = snapshot(&mut builder, &mut fixture, &view);
    root.destroy();
    fixture.sync();
    let named = snapshot(&mut builder, &mut fixture, &view);
    assert!(named.image.size().w > 1);
    assert!(fixture.app.cursor_surface.is_none());
    assert_eq!(words(&client), vec![RED]);
    let wrong_output = fixture::view((800, 0), 1);
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &wrong_output)
            .unwrap()
            .is_none()
    );
    let again = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(again.revision, named.revision);
    assert_eq!(again.image.as_ptr(), named.image.as_ptr());
    let dense_view = fixture::view((0, 0), 2);
    let dense = snapshot(&mut builder, &mut fixture, &dense_view);
    assert_eq!(dense.image.size().w, named.image.size().w * 2);
    assert_eq!(dense.image.size().h, named.image.size().h * 2);
    builder.reset();
    let reset = snapshot(&mut builder, &mut fixture, &dense_view);
    assert_ne!(reset.image_id, dense.image_id);
    assert_eq!(words(&reset), words(&dense));
}
