use std::{os::fd::OwnedFd, sync::Arc};

use wayland_server::backend::{ClientId, Handle, ObjectData, ObjectId, protocol::Message};

use super::*;

/// A live wl_buffer resource whose driver exposes no recognized storage. This
/// substitutes only the buffer-driver boundary, not surface commits or rendering.
pub(super) struct UnrecognizedBuffer;

impl ObjectData<App> for UnrecognizedBuffer {
    fn request(
        self: Arc<Self>,
        _: &Handle,
        _: &mut App,
        _: ClientId,
        message: Message<ObjectId, OwnedFd>,
    ) -> Option<Arc<dyn ObjectData<App>>> {
        assert_eq!(message.opcode, 0, "wl_buffer only has a destroy request");
        None
    }

    fn destroyed(self: Arc<Self>, _: &Handle, _: &mut App, _: ClientId, _: ObjectId) {}
}

#[test]
fn unrecognized_nonnull_root_is_error_until_valid_attachment_or_explicit_null() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let unknown = fixture.unrecognized_image();
    fixture.attach(&surface, &unknown);
    fixture.cursor(Some(&surface), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .is_err()
    );
    // A callback-only commit must not turn the failed non-null buffer into None.
    surface.frame(&fixture.queue(), false);
    surface.commit();
    fixture.sync();
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .is_err()
    );
    let blue = fixture.image((1, 1), &[BLUE], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&surface, &blue);
    let recovered = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&recovered), vec![BLUE]);
    fixture.attach(&surface, &unknown);
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .is_err()
    );
    surface.attach(None, 0, 0);
    surface.commit();
    fixture.sync();
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .unwrap()
            .is_none()
    );
    assert_eq!(words(&recovered), vec![BLUE]);
}

#[test]
fn unrecognized_child_fails_whole_cursor_instead_of_publishing_partial_artwork() {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let child = fixture.surface();
    let sub = fixture.subsurface(&child, &root);
    sub.set_position(1, 0);
    let red = fixture.image((1, 1), &[RED], (0, 0), wl_shm::Format::Argb8888);
    let unknown = fixture.unrecognized_image();
    fixture.attach(&child, &unknown);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .is_err()
    );
    let blue = fixture.image((1, 1), &[BLUE], (0, 0), wl_shm::Format::Argb8888);
    fixture.attach(&child, &blue);
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .is_err(),
        "recovery is still synchronized"
    );
    root.commit();
    fixture.sync();
    let recovered = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(recovered.image.size(), (2, 1).into());
    assert_eq!(words(&recovered), vec![RED, BLUE]);
    child.attach(None, 0, 0);
    child.commit();
    root.commit();
    fixture.sync();
    assert_eq!(
        words(&snapshot(&mut builder, &mut fixture, &view)),
        vec![RED]
    );
}
