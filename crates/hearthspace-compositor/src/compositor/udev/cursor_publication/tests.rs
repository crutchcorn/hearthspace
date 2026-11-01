//! Exercise production publication/completion with real App protocol resources.
//! Image identities stand in for successful snapshots; no renderer or KMS runs.
use super::*;
use crate::compositor::udev::identity::DeviceToken;
use std::num::NonZeroU32;

mod fixture;
use fixture::Fixture;

fn key(generation: u64) -> OutputKey {
    OutputKey {
        device: DeviceToken {
            id: 226,
            incarnation: 2,
        },
        crtc: NonZeroU32::new(7).unwrap().into(),
        generation,
    }
}

fn publish(
    publication: &mut CursorPublication,
    app: &App,
    output: OutputKey,
    image: &Option<(Id, u64)>,
) -> Option<u64> {
    publication.prepare(PublishedCursor {
        output: Some(output),
        image: image.clone(),
        location: image.as_ref().map(|_| (1, 1).into()),
        surface: app.cursor_surface.clone(),
    })
}

#[test]
fn old_image_completion_does_not_release_callbacks_from_a_new_commit() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    fixture.cursor(Some(&surface));
    fixture.commit(&surface, Some(0xffff0000), 1);
    let mut publication = CursorPublication::default();
    let old = Some((Id::new(), 1));
    let new = Some((old.as_ref().unwrap().0.clone(), 2));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &old),
        Some(1)
    );

    fixture.commit(&surface, Some(0xff00ff00), 2);
    // Dispatch has committed new pixels, but publication has not run yet (the
    // same boundary also applies when taking the new snapshot fails).
    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 16), 1);
    fixture.assert_done(&[(1, 16)]);
    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 17), 0);
    fixture.assert_done(&[]);

    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &new),
        Some(2)
    );
    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 24), 0);
    fixture.assert_done(&[]);
    assert_eq!(publication.callbacks(&fixture.app, key(1), &new, 32), 1);
    fixture.assert_done(&[(2, 32)]);
    assert_eq!(publication.callbacks(&fixture.app, key(1), &new, 48), 0);
    assert_eq!(publish(&mut publication, &fixture.app, key(1), &new), None);
}

#[test]
fn callback_only_commit_waits_for_its_publication_and_keeps_coalescing() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    fixture.cursor(Some(&surface));
    fixture.commit(&surface, Some(0xffff0000), 1);
    let mut publication = CursorPublication::default();
    let image = Some((Id::new(), 1));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &image),
        Some(1)
    );
    fixture.commit(&surface, None, 2);
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &image),
        None
    );

    assert_eq!(publication.callbacks(&fixture.app, key(1), &image, 16), 1);
    fixture.assert_done(&[(1, 16)]);
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &image),
        Some(2)
    );
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &image),
        None
    );
    // FrameOpportunity carries the same image even when no new pixels exist.
    assert_eq!(publication.callbacks(&fixture.app, key(1), &image, 32), 1);
    fixture.assert_done(&[(2, 32)]);
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &image),
        None
    );
}

#[test]
fn superseded_publication_preserves_requests_for_the_replacement_image() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    fixture.cursor(Some(&surface));
    fixture.commit(&surface, Some(0xffff0000), 1);
    let mut publication = CursorPublication::default();
    let old = Some((Id::new(), 1));
    let new = Some((old.as_ref().unwrap().0.clone(), 2));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &old),
        Some(1)
    );
    fixture.commit(&surface, Some(0xff00ff00), 2);
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &new),
        Some(2)
    );

    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 16), 0);
    fixture.assert_done(&[]);
    assert_eq!(publication.callbacks(&fixture.app, key(1), &new, 32), 2);
    fixture.assert_done(&[(1, 32), (2, 32)]);
    assert!(publication.callbacks.is_empty());
    assert_eq!(publish(&mut publication, &fixture.app, key(1), &new), None);
}

#[test]
fn reset_preserves_requests_without_accepting_old_output_or_image_generations() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    fixture.cursor(Some(&surface));
    fixture.commit(&surface, Some(0xffff0000), 1);
    let mut publication = CursorPublication::default();
    let old = Some((Id::new(), 1));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &old),
        Some(1)
    );
    publication.reset();
    assert!(publication.callbacks.is_empty());
    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 16), 0);
    fixture.assert_done(&[]);

    // Security reset creates a new snapshot identity; output rebuilds also
    // replace the generation. The original callback remains in committed state.
    let new = Some((Id::new(), 1));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(2), &new),
        Some(2)
    );
    assert_eq!(publication.callbacks(&fixture.app, key(1), &new, 24), 0);
    assert_eq!(publication.callbacks(&fixture.app, key(2), &old, 24), 0);
    fixture.assert_done(&[]);
    assert_eq!(publication.callbacks(&fixture.app, key(2), &new, 32), 1);
    fixture.assert_done(&[(1, 32)]);
    assert_eq!(publish(&mut publication, &fixture.app, key(2), &new), None);
}

#[test]
fn hidden_and_replaced_surfaces_keep_their_own_pending_callbacks() {
    let mut fixture = Fixture::new();
    let first = fixture.surface();
    let second = fixture.surface();
    fixture.cursor(Some(&first));
    fixture.commit(&first, Some(0xffff0000), 1);
    let mut publication = CursorPublication::default();
    let old = Some((Id::new(), 1));
    let new = Some((old.as_ref().unwrap().0.clone(), 2));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &old),
        Some(1)
    );
    fixture.cursor(None);
    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 16), 0);
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &None),
        Some(2)
    );
    assert_eq!(publication.callbacks(&fixture.app, key(1), &None, 24), 0);
    fixture.assert_done(&[]);

    fixture.cursor(Some(&second));
    fixture.commit(&second, Some(0xff00ff00), 2);
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &new),
        Some(3)
    );
    assert_eq!(publication.callbacks(&fixture.app, key(1), &new, 32), 1);
    fixture.assert_done(&[(2, 32)]);
    fixture.cursor(Some(&first));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &old),
        Some(4)
    );
    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 48), 1);
    fixture.assert_done(&[(1, 48)]);
}

#[test]
fn destroyed_surface_releases_the_publication_batch_without_sending_done() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    fixture.cursor(Some(&surface));
    fixture.commit(&surface, Some(0xffff0000), 1);
    let mut publication = CursorPublication::default();
    let image = Some((Id::new(), 1));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &image),
        Some(1)
    );
    surface.destroy();
    fixture.sync();
    assert_eq!(publication.callbacks(&fixture.app, key(1), &image, 16), 0);
    fixture.assert_done(&[]);
    assert!(publication.callbacks.is_empty());
    publication.reset();
    assert!(publication.callbacks.is_empty());
}

#[test]
fn cursor_subsurface_callbacks_are_also_frozen_at_publication() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.child(&root);
    fixture.cursor(Some(&root));
    fixture.commit(&root, Some(0xffff0000), 1);
    fixture.commit(&child, Some(0xff00ff00), 2);
    let mut publication = CursorPublication::default();
    let old = Some((Id::new(), 1));
    let new = Some((old.as_ref().unwrap().0.clone(), 2));
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &old),
        Some(1)
    );
    fixture.commit(&child, Some(0xff0000ff), 3);
    assert_eq!(publication.callbacks(&fixture.app, key(1), &old, 16), 2);
    fixture.assert_done(&[(1, 16), (2, 16)]);
    assert_eq!(
        publish(&mut publication, &fixture.app, key(1), &new),
        Some(2)
    );
    assert_eq!(publication.callbacks(&fixture.app, key(1), &new, 32), 1);
    fixture.assert_done(&[(3, 32)]);
}

#[test]
fn native_cursor_artwork_and_callbacks_do_not_request_application_scene_rendering() {
    let mut fixture = Fixture::new();
    fixture.app.independent_cursor = true;
    let root = fixture.surface();
    let child = fixture.child(&root);
    fixture.app.needs_redraw = false;
    fixture.cursor(Some(&root));
    assert!(!fixture.app.needs_redraw);
    fixture.commit(&root, Some(0xffff0000), 1);
    fixture.commit(&child, Some(0xff00ff00), 2);
    fixture.commit(&root, None, 3);
    assert!(!fixture.app.needs_redraw);
    fixture.cursor(None);
    assert!(!fixture.app.needs_redraw);
    // Existing scene work survives cursor-only dispatch.
    fixture.app.request_redraw();
    fixture.cursor(Some(&root));
    fixture.commit(&root, Some(0xff0000ff), 4);
    assert!(fixture.app.needs_redraw);
    fixture.app.needs_redraw = false;
    let ordinary = fixture.surface();
    fixture.commit(&ordinary, Some(0xffffffff), 5);
    assert!(fixture.app.needs_redraw);
}

#[test]
fn composited_backend_cursor_commits_still_request_a_scene() {
    let mut fixture = Fixture::new();
    assert!(!fixture.app.independent_cursor);
    let surface = fixture.surface();
    fixture.app.needs_redraw = false;
    fixture.cursor(Some(&surface));
    assert!(fixture.app.needs_redraw);
    fixture.app.needs_redraw = false;
    fixture.commit(&surface, Some(0xffff0000), 1);
    assert!(fixture.app.needs_redraw);
}
