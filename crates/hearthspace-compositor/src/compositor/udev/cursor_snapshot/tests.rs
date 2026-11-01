mod applied_state;
mod client;
mod commits;
mod fixture;
mod recognition;
mod reference_reorder;
mod transactions;

use wayland_client::protocol::{wl_output, wl_shm};

use super::*;
use fixture::{Fixture, view};

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;

fn words(snapshot: &CursorSnapshot) -> Vec<u32> {
    snapshot
        .image
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| u32::from_ne_bytes(*pixel))
        .collect()
}

fn snapshot(
    builder: &mut SnapshotBuilder,
    fixture: &mut Fixture,
    view: &OutputRenderView,
) -> CursorSnapshot {
    builder
        .update(&mut fixture.app, &mut fixture.renderer, view)
        .unwrap()
        .unwrap()
}

#[test]
fn committed_shm_respects_stride_alpha_hotspot_and_image_lifetime() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let pixels = [RED, 0x8040_2010, BLUE, 0];
    let image = fixture.image((2, 2), &pixels, (12, 8), wl_shm::Format::Argb8888);
    fixture.attach(&surface, &image);
    fixture.cursor(Some(&surface), (1, 2));
    fixture.app.pointer_location = (110.4, 70.6).into();
    let view = view((100, 50), 1);
    let mut builder = SnapshotBuilder::default();
    // wl_buffer and pool destruction must not invalidate the committed content.
    image.buffer.destroy();
    fixture.sync();
    let first = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(words(&first), pixels);
    assert_eq!(first.location, (9, 19).into());
    assert_eq!(first.image.stride(), 8);
    assert_eq!(
        first.image.format(),
        smithay::backend::allocator::Fourcc::Argb8888
    );
    fixture.app.pointer_location = (113.4, 72.6).into();
    fixture.app.viewport_scale = 0.25;
    let moved = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(moved.location, (12, 21).into());
    assert_eq!(
        (moved.image_id.clone(), moved.revision),
        (first.image_id.clone(), first.revision)
    );
    assert_eq!(moved.image.as_ptr(), first.image.as_ptr());
    fixture.cursor(Some(&surface), (0, 0));
    let hotspot = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(hotspot.location, (13, 23).into());
    assert_eq!(hotspot.image.as_ptr(), first.image.as_ptr());
    assert_eq!(hotspot.revision, first.revision);
    surface.attach(None, 0, 0);
    surface.commit();
    fixture.sync();
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        words(&first),
        pixels,
        "a submitted snapshot outlives its client buffer"
    );
}

#[test]
fn viewport_transform_and_density_are_rendered_without_scene_policy_changes() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let image = fixture.image(
        (4, 2),
        &[RED, RED, BLUE, BLUE, RED, RED, BLUE, BLUE],
        (0, 0),
        wl_shm::Format::Argb8888,
    );
    surface.set_buffer_scale(2);
    let viewport = fixture.viewport(&surface);
    // Fractional crop stays within the blue pixel centers when magnified, so
    // linear filtering cannot legitimately sample the adjacent red region.
    viewport.set_source(1.25, 0., 0.5, 1.);
    viewport.set_destination(2, 2);
    fixture.attach(&surface, &image);
    fixture.cursor(Some(&surface), (1, 1));
    fixture.app.pointer_location = (110.25, 70.75).into();
    fixture.app.software_cursor_visible = false;
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view((100, 50), 2));
    assert_eq!(first.location, (19, 40).into());
    assert_eq!(first.image.size(), (4, 4).into());
    assert_eq!(words(&first), vec![BLUE; 16]);
    assert!(!fixture.app.software_cursor_visible);
    fixture.app.pointer_location = (111.25, 71.75).into();
    surface.frame(&fixture.queue(), false);
    surface.commit();
    fixture.sync();
    let moved = snapshot(&mut builder, &mut fixture, &view((100, 50), 2));
    assert_eq!(moved.location, (21, 42).into());
    assert_eq!(moved.revision, first.revision);
    assert_eq!(moved.image.as_ptr(), first.image.as_ptr());
    // Transform changes the crop's source pixels, even with the same wl_buffer.
    surface.set_buffer_transform(wl_output::Transform::Flipped);
    fixture.attach(&surface, &image);
    let flipped = snapshot(&mut builder, &mut fixture, &view((100, 50), 2));
    assert_eq!(words(&flipped), vec![RED; 16]);
    assert_ne!(flipped.revision, first.revision);
    let low_density = snapshot(&mut builder, &mut fixture, &view((100, 50), 1));
    assert_eq!(low_density.image.size(), (2, 2).into());
    assert_eq!(words(&low_density), vec![RED; 4]);
    assert_ne!(low_density.revision, flipped.revision);
}

#[test]
fn full_shm_buffer_at_matching_output_density_preserves_native_pixels() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let pixels = [RED, GREEN, BLUE, 0, BLUE, GREEN, RED, 0];
    let image = fixture.image((4, 2), &pixels, (8, 4), wl_shm::Format::Argb8888);
    surface.set_buffer_scale(2);
    fixture.attach(&surface, &image);
    fixture.cursor(Some(&surface), (1, 1));
    fixture.app.pointer_location = (10.25, 20.75).into();
    let first = snapshot(
        &mut SnapshotBuilder::default(),
        &mut fixture,
        &view((0, 0), 2),
    );
    assert_eq!(first.image.size(), (4, 2).into());
    assert_eq!(first.location, (19, 40).into());
    assert_eq!(words(&first), pixels);
}

#[test]
fn oversized_software_cursor_and_retriable_bounds_error_are_distinct_from_hidden() {
    let mut fixture = Fixture::new();
    let surface = fixture.surface();
    let image = fixture.image((1, 1), &[0x0012_3456], (4, 4), wl_shm::Format::Xrgb8888);
    let viewport = fixture.viewport(&surface);
    viewport.set_destination(256, 256);
    fixture.attach(&surface, &image);
    fixture.cursor(Some(&surface), (0, 0));
    let view = view((0, 0), 1);
    let mut builder = SnapshotBuilder::default();
    let first = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(first.image.size(), (256, 256).into());
    assert_eq!(words(&first), vec![0xff12_3456; 256 * 256]);
    viewport.set_destination(4097, 4096);
    surface.commit();
    fixture.sync();
    fixture.app.software_cursor_visible = true;
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .is_err()
    );
    assert!(fixture.app.software_cursor_visible);
    viewport.set_destination(32, 32);
    surface.commit();
    fixture.sync();
    let recovered = snapshot(&mut builder, &mut fixture, &view);
    assert_eq!(recovered.image.size(), (32, 32).into());
    assert_ne!(recovered.revision, first.revision);
    fixture.cursor(None, (0, 0));
    assert!(
        builder
            .update(&mut fixture.app, &mut fixture.renderer, &view)
            .unwrap()
            .is_none()
    );
}
