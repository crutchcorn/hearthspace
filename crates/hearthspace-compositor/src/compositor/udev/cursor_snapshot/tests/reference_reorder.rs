use smithay::wayland::compositor::{Barrier, add_blocker};

use super::*;

#[test]
fn reference_role_destruction_preserves_above_and_below_moves() {
    check_reference_destruction(false, true);
    check_reference_destruction(false, false);
}

#[test]
fn blocked_parent_preserves_above_and_below_moves_after_reference_role_destruction() {
    check_reference_destruction(true, true);
    check_reference_destruction(true, false);
}

fn check_reference_destruction(blocked: bool, above: bool) {
    let mut fixture = Fixture::new();
    let root = fixture.surface();
    let a = fixture.surface();
    let c = fixture.surface();
    let b = fixture.surface();
    let role_a = fixture.subsurface(&a, &root);
    let _role_c = fixture.subsurface(&c, &root);
    let role_b = fixture.subsurface(&b, &root);
    let white = 0xffff_ffff;
    let red = fixture.image((4, 1), &[RED; 4], (0, 0), wl_shm::Format::Argb8888);
    let blue = fixture.image(
        (4, 1),
        &[BLUE, BLUE, 0, 0],
        (0, 0),
        wl_shm::Format::Argb8888,
    );
    let green = fixture.image(
        (4, 1),
        &[GREEN, GREEN, GREEN, 0],
        (0, 0),
        wl_shm::Format::Argb8888,
    );
    let white_image = fixture.image(
        (4, 1),
        &[0, white, white, white],
        (0, 0),
        wl_shm::Format::Argb8888,
    );
    fixture.attach(&a, &blue);
    fixture.attach(&c, &green);
    fixture.attach(&b, &white_image);
    fixture.attach(&root, &red);
    fixture.cursor(Some(&root), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    // Initial bottom-to-top order is [parent, A, C, B]. All three
    // children overlap, with transparent pixels exposing lower layers.
    let initial = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&initial), vec![GREEN, white, white, white]);
    if above {
        role_a.place_above(&b); // Pending [parent, C, B, A].
    } else {
        role_b.place_below(&a); // Pending [parent, B, A, C].
    }
    fixture.sync();
    let pending = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(pending.revision, initial.revision);
    assert_eq!(words(&pending), words(&initial));
    let blocker = Barrier::new(false);
    if blocked {
        add_blocker(&fixture.server_surface(&root), blocker.clone());
        root.commit();
        fixture.sync();
        let waiting = snapshot(&mut builder, &mut fixture, &view);
        assert_eq!(waiting.revision, initial.revision);
        assert_eq!(words(&waiting), words(&initial));
    }
    // Only the reference role is destroyed. Its wl_surface and buffer stay
    // alive, so weak surface liveness alone cannot represent membership.
    if above {
        role_b.destroy();
    } else {
        role_a.destroy();
    }
    fixture.sync();
    let removed = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(
        words(&removed),
        if above {
            vec![GREEN, GREEN, GREEN, RED]
        } else {
            vec![GREEN, white, white, white]
        }
    );
    if blocked {
        fixture.clear_blocker(&blocker);
    } else {
        root.commit();
        fixture.sync();
    }
    let applied = snapshot(&mut builder, &mut fixture, &view);
    // Removing the reference preserves [parent, C, A] / [parent, B, C].
    assert_eq!(
        words(&applied),
        if above {
            vec![BLUE, BLUE, GREEN, RED]
        } else {
            vec![GREEN, GREEN, GREEN, white]
        }
    );
    assert_ne!(applied.revision, removed.revision);
    root.commit();
    fixture.sync();
    let retained = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(retained.revision, applied.revision);
    assert_eq!(words(&retained), words(&applied));
}
