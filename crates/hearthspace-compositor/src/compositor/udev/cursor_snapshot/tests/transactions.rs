use smithay::wayland::compositor::{
    Barrier, BufferAssignment, SubsurfaceCachedState, SurfaceAttributes, add_blocker,
    add_post_commit_hook, with_states,
};
use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn parent_commit_does_not_consume_child_pending_attachment_but_applies_position() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let sub = fixture.subsurface(&child, &root);
    sub.set_position(-1, 0);
    let red = fixture.image((2, 1), &[RED; 2], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image((2, 1), &[BLUE; 2], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &blue);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&first), vec![BLUE, BLUE, RED]);
    let green = fixture.image((2, 1), &[GREEN; 2], (0, 0), wl_shm::Format::Argb8888);
    child.attach(Some(&green.buffer), 0, 0);
    child.damage_buffer(0, 0, 2, 1);
    // Dispatch the requests and parent commit, intentionally omitting child.commit.
    root.commit();
    fixture.sync();
    let still_blue = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&still_blue), words(&first));
    assert_eq!(still_blue.revision, first.revision);
    assert_eq!(still_blue.image.as_ptr(), first.image.as_ptr());
    sub.set_position(-2, 0);
    root.commit();
    fixture.sync();
    let positioned = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&positioned), vec![BLUE, BLUE, RED, RED]);
    // Position belongs to the parent; the green buffer still belongs to the
    // uncommitted child state. Only this explicit child commit can cache it.
    child.commit();
    fixture.sync();
    assert_eq!(
        snapshot(&mut builder, &mut fixture, &view).revision,
        positioned.revision
    );
    root.commit();
    fixture.sync();
    let green = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&green), vec![GREEN, GREEN, RED, RED]);
    assert_ne!(green.revision, positioned.revision);
}

#[test]
fn stacking_requests_remain_pending_until_parent_commit() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let sub = fixture.subsurface(&child, &root);
    sub.set_position(-1, 0);
    let red = fixture.image((2, 1), &[RED; 2], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image((2, 1), &[BLUE; 2], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &blue);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let above = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&above), vec![BLUE, BLUE, RED]);
    sub.place_below(&root);
    fixture.sync();
    let pending = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(pending.revision, above.revision);
    assert_eq!(words(&pending), words(&above));
    root.commit();
    fixture.sync();
    let below = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&below), vec![BLUE, RED, RED]);
    assert_ne!(below.revision, above.revision);
    sub.place_above(&root);
    fixture.sync();
    let pending = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(pending.revision, below.revision);
    assert_eq!(words(&pending), words(&below));
    root.commit();
    fixture.sync();
    let restored = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&restored), words(&above));
    assert_ne!(restored.revision, below.revision);
}

#[test]
fn blocked_parent_layout_stays_pending_across_desynchronized_child_commits() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let sub = fixture.subsurface(&child, &root);
    let red = fixture.image((2, 1), &[RED; 2], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image((2, 1), &[BLUE; 2], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &blue);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    let blocker = Barrier::new(false);
    add_blocker(&fixture.server_surface(&root), blocker.clone());
    sub.set_position(1, 0);
    sub.place_below(&root);
    root.commit();
    fixture.sync();
    assert_eq!(
        snapshot(&mut builder, &mut fixture, &view).revision,
        first.revision
    );
    sub.set_desync();
    child.commit();
    fixture.sync();
    let child_commit = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(child_commit.revision, first.revision);
    assert_eq!(words(&child_commit), vec![BLUE, BLUE]);
    fixture.clear_blocker(&blocker);
    let applied = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&applied), vec![RED, RED, BLUE]);
    child.commit();
    fixture.sync();
    let retained = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(retained.revision, applied.revision);
    assert_eq!(words(&retained), words(&applied));
}

#[test]
fn child_post_commit_hook_sees_parent_buffer_and_layout_from_same_transaction() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let sub = fixture.subsurface(&child, &root);
    let red = fixture.image((1, 1), &[RED], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image((1, 1), &[BLUE], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &blue);
    fixture.attach(&root, &red);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observations = observed.clone();
    let parent = fixture.server_surface(&root);
    add_post_commit_hook::<App, _>(&fixture.server_surface(&child), move |_, _, surface| {
        let parent_has_attachment = with_states(&parent, |states| {
            matches!(
                states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .buffer,
                Some(BufferAssignment::NewBuffer(_))
            )
        });
        let location = with_states(surface, |states| {
            states
                .cached_state
                .get::<SubsurfaceCachedState>()
                .current()
                .location
        });
        observations
            .lock()
            .unwrap()
            .push((parent_has_attachment, location));
    });
    let green = fixture.image((1, 1), &[GREEN], (0, 0), wl_shm::Format::Argb8888);
    child.commit();
    sub.set_position(2, 0);
    fixture.attach(&root, &green);
    assert_eq!(*observed.lock().unwrap(), vec![(true, (2, 0).into())]);
}

#[test]
fn ancestor_applies_committed_grandchild_without_consuming_its_pending_buffer() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let grandchild = fixture.surface();
    let child_role = fixture.subsurface(&child, &root);
    let grandchild_role = fixture.subsurface(&grandchild, &child);
    child_role.set_position(1, 0);
    grandchild_role.set_position(1, 0);
    let red = fixture.image((1, 1), &[RED], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image((1, 1), &[BLUE], (0, 0), wl_shm::Format::Argb8888);
    let green = fixture.image((1, 1), &[GREEN], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&grandchild, &green);
    fixture.attach(&child, &blue);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&first), vec![RED, BLUE, GREEN]);
    let white = fixture.image((1, 1), &[0xffff_ffff], (0, 0), wl_shm::Format::Argb8888);
    grandchild.attach(Some(&white.buffer), 0, 0);
    grandchild.damage_buffer(0, 0, 1, 1);
    root.commit();
    fixture.sync();
    let pending = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(pending.revision, first.revision);
    assert_eq!(words(&pending), words(&first));
    grandchild.commit();
    // The direct parent need not commit again: the effective sync ancestor
    // applies already cached descendants when its own state is applied.
    root.commit();
    fixture.sync();
    assert_eq!(
        words(&snapshot(&mut builder, &mut fixture, &view)),
        vec![RED, BLUE, 0xffff_ffff]
    );
}

#[test]
fn queued_layout_does_not_cross_destroyed_and_recreated_subsurface_role() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let old_role = fixture.subsurface(&child, &root);
    let red = fixture.image((2, 1), &[RED; 2], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image((1, 1), &[BLUE], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &blue);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    assert_eq!(
        words(&snapshot(&mut builder, &mut fixture, &view)),
        vec![BLUE, RED]
    );
    let blocker = Barrier::new(false);
    add_blocker(&fixture.server_surface(&root), blocker.clone());
    old_role.set_position(2, 0);
    old_role.place_below(&root);
    root.commit();
    fixture.sync();
    old_role.destroy();
    fixture.sync();
    let _new_role = fixture.subsurface(&child, &root);
    child.commit();
    root.commit();
    fixture.sync();
    fixture.clear_blocker(&blocker);
    // The same wl_surface now has a new role instance, with default position
    // and stacking. The old blocked transaction must not alter that instance.
    let recreated = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(recreated.image.size(), (2, 1).into());
    assert_eq!(words(&recreated), vec![BLUE, RED]);
}
