//! Real client requests exercise App, Smithay and the production feedback adapter.
//! Only the GPU import itself is replaced by a recorded, nonfatal rejection.
use smithay::backend::allocator::{Format, Fourcc, Modifier};
use wayland_client::Proxy;

mod client;
mod fixture;
mod server;
use fixture::{Fixture, format};

const GPU_A: u64 = 0x101;
const GPU_B: u64 = 0x102;

#[test]
fn failover_updates_existing_default_roleless_and_surface_tree_feedback() {
    let a = format(Fourcc::Argb8888);
    let b = format(Fourcc::Xrgb8888);
    let mut f = Fixture::new(Some((GPU_A, vec![a])), 4);
    let bound = f.dmabuf();
    let global = f.server.app._dmabuf_global;
    f.default_feedback(0);
    let roleless = f.surface();
    let (toplevel, xdg) = f.toplevel();
    let child = f.subsurface(&toplevel);
    let popup = f.popup(&xdg);
    let popup_child = f.subsurface(&popup);
    for (index, surface) in [&roleless, &toplevel, &child, &popup, &popup_child]
        .into_iter()
        .enumerate()
    {
        f.surface_feedback(surface, index + 1);
        f.assert_feedback(index + 1, 1, GPU_A, &[a]);
    }
    // A second feedback object for one surface shares its independent state.
    f.surface_feedback(&roleless, 6);
    assert_eq!(f.server.app.dmabuf_feedback_surfaces.len(), 5);

    // The GPUs deliberately have no format intersection. Full B feedback must
    // remain valid; an empty table/tranche or a retained A device is incorrect.
    f.replace(GPU_B, vec![b]);
    assert_eq!(f.dmabuf(), bound);
    assert_eq!(f.server.app._dmabuf_global, global);
    assert!(f.client.removed.is_empty());
    for id in 0..=6 {
        f.assert_feedback(id, 2, GPU_B, &[b]);
    }

    let late = f.surface();
    f.surface_feedback(&late, 7);
    f.assert_feedback(7, 1, GPU_B, &[b]);
    f.surface_feedback(&roleless, 8);
    f.assert_feedback(8, 1, GPU_B, &[b]);
    f.replace(GPU_B, vec![b]);
    for id in 0..=6 {
        f.assert_feedback(id, 2, GPU_B, &[b]);
    }
    f.assert_feedback(7, 1, GPU_B, &[b]);
}

#[test]
fn existing_bindings_and_inflight_params_accept_new_fourcc_and_nonfatally_reject_stale_allocations()
{
    let a = format(Fourcc::Argb8888);
    let b = format(Fourcc::Xrgb8888);
    let mut f = Fixture::new(Some((GPU_A, vec![a])), 4);
    f.default_feedback(0);
    let new_format_params = f.params(1);
    let old_allocation_params = f.params(2);
    f.replace(GPU_B, vec![b]);
    f.assert_feedback(0, 2, GPU_B, &[b]);

    // This formerly hit Smithay's fatal InvalidFormat before the import handler.
    Fixture::submit(&new_format_params, b);
    Fixture::submit(&old_allocation_params, a);
    f.sync();
    assert_eq!(f.server.imported, [b, a]);
    assert_eq!(f.client.failed, [1, 2]);
    assert!(f.connection.protocol_error().is_none());

    let later_params = f.params(3);
    Fixture::submit(&later_params, b);
    f.sync();
    assert_eq!(f.server.imported, [b, a, b]);
    assert_eq!(f.client.failed, [1, 2, 3]);
    assert!(f.connection.protocol_error().is_none());
}

#[test]
fn feedback_and_new_legacy_bindings_use_current_formats_not_validation_history() {
    let a = format(Fourcc::Argb8888);
    let b = Format {
        code: Fourcc::Xrgb8888,
        modifier: Modifier::Invalid,
    };
    let mut f = Fixture::new(Some((GPU_A, vec![a])), 6);
    let old_legacy = f.legacy_binding(40);
    assert_eq!(f.client.legacy[&40], [a]);
    f.default_feedback(0);
    let roleless = f.surface();
    f.surface_feedback(&roleless, 1);
    f.replace(GPU_B, vec![b]);
    f.assert_feedback(0, 2, GPU_B, &[b]);
    f.assert_feedback(1, 2, GPU_B, &[b]);
    let new_legacy = f.legacy_binding(41);
    assert_eq!(old_legacy.version(), 3);
    assert_eq!(new_legacy.version(), 3);
    assert_eq!(
        f.client.legacy[&40],
        [a],
        "already sent legacy events are not revoked"
    );
    assert_eq!(
        f.client.legacy[&41],
        [b],
        "retired formats are validation history, not current advice"
    );

    f.replace(GPU_A, vec![a]);
    f.assert_feedback(0, 3, GPU_A, &[a]);
    let stale_b = f.params(2);
    Fixture::submit(&stale_b, b);
    f.sync();
    assert_eq!(f.server.imported, [b]);
    assert_eq!(f.client.failed, [2]);
}

#[test]
fn no_gpu_startup_upgrades_v3_once_and_preserves_existing_bindings() {
    let mut f = Fixture::new(None, 4);
    assert_eq!(f.client.globals.len(), 1);
    let (old_name, _, old_binding) = f.client.globals[0].clone();
    assert_eq!(old_binding.version(), 3);
    let a = format(Fourcc::Argb8888);
    f.replace(GPU_A, vec![a]);
    f.sync();
    assert_eq!(f.client.globals.len(), 2);
    assert_eq!(f.client.removed, [old_name]);
    assert!(old_binding.is_alive());
    assert_eq!(f.dmabuf().version(), 4);
    f.default_feedback(0);
    f.assert_feedback(0, 1, GPU_A, &[a]);
    f.replace(GPU_B, vec![format(Fourcc::Xrgb8888)]);
    assert_eq!(f.client.globals.len(), 2);
    assert_eq!(f.client.removed, [old_name]);
    assert!(f.connection.protocol_error().is_none());
}

#[test]
fn destroyed_surfaces_are_pruned_and_feedback_recreated_after_failover_is_current() {
    let a = format(Fourcc::Argb8888);
    let b = format(Fourcc::Xrgb8888);
    let mut f = Fixture::new(Some((GPU_A, vec![a])), 4);
    let retired = f.surface();
    f.surface_feedback(&retired, 0);
    let survivor = f.surface();
    let old_feedback = f.surface_feedback(&survivor, 1);
    old_feedback.destroy();
    retired.destroy();
    f.sync();
    assert_eq!(f.server.app.dmabuf_feedback_surfaces.len(), 1);
    f.replace(GPU_B, vec![b]);
    assert_eq!(f.server.app.dmabuf_feedback_surfaces.len(), 1);
    f.surface_feedback(&survivor, 2);
    f.assert_feedback(2, 1, GPU_B, &[b]);
    assert!(f.connection.protocol_error().is_none());
}

#[test]
fn never_advertised_fourcc_still_gets_invalid_format() {
    let mut f = Fixture::new(Some((GPU_A, vec![format(Fourcc::Argb8888)])), 4);
    f.replace(GPU_B, vec![format(Fourcc::Xrgb8888)]);
    let params = f.params(0);
    Fixture::submit(&params, format(Fourcc::Abgr8888));
    assert!(f.sync_result().is_err());
    let error = f
        .connection
        .protocol_error()
        .expect("unadvertised Fourcc must be rejected");
    assert_eq!(error.object_interface, "zwp_linux_buffer_params_v1");
    assert_eq!(error.code, wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Error::InvalidFormat as u32);
    assert!(f.server.imported.is_empty());
}

#[test]
fn empty_formats_preserve_existing_feedback_until_a_valid_retry() {
    let a = format(Fourcc::Argb8888);
    let b = format(Fourcc::Xrgb8888);
    let mut f = Fixture::new(Some((GPU_A, vec![a])), 4);
    f.default_feedback(0);
    let surface = f.surface();
    f.surface_feedback(&surface, 1);
    let global = f.server.app._dmabuf_global;
    let binding = f.dmabuf();

    let error = f.try_replace(GPU_B, Vec::new()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(f.server.app._dmabuf_global, global);
    assert_eq!(f.dmabuf(), binding);
    assert!(f.client.removed.is_empty());
    f.assert_feedback(0, 1, GPU_A, &[a]);
    f.assert_feedback(1, 1, GPU_A, &[a]);
    // Fresh requests must still see the previously published complete state.
    f.default_feedback(2);
    f.surface_feedback(&surface, 3);
    f.assert_feedback(2, 1, GPU_A, &[a]);
    f.assert_feedback(3, 1, GPU_A, &[a]);

    f.replace(GPU_B, vec![b]);
    for id in 0..=3 {
        f.assert_feedback(id, 2, GPU_B, &[b]);
    }
    assert!(f.connection.protocol_error().is_none());
}

#[test]
fn empty_startup_formats_do_not_replace_the_v3_global() {
    let mut f = Fixture::new(None, 4);
    let global = f.server.app._dmabuf_global;
    let binding = f.dmabuf();
    let error = f.try_replace(GPU_A, Vec::new()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(f.server.app._dmabuf_global, global);
    assert_eq!(f.dmabuf(), binding);
    assert_eq!(binding.version(), 3);
    assert_eq!(f.client.globals.len(), 1);
    assert!(f.client.removed.is_empty());
    assert!(f.client.feedback.is_empty());

    let a = format(Fourcc::Argb8888);
    f.replace(GPU_A, vec![a]);
    assert_eq!(f.client.globals.len(), 2);
    assert_eq!(f.dmabuf().version(), 4);
    f.default_feedback(0);
    f.assert_feedback(0, 1, GPU_A, &[a]);
}

#[test]
fn app_surface_feedback_registry_tracks_lifetime_without_gpu_changes() {
    let a = format(Fourcc::Argb8888);
    let mut f = Fixture::new(Some((GPU_A, vec![a])), 4);
    let survivor = f.surface();
    f.surface_feedback(&survivor, 0);
    assert_eq!(f.server.app.dmabuf_feedback_surfaces.len(), 1);

    // This runs real App hooks without any native backend or feedback refresh.
    // Multiple feedback objects share one registration; surface destruction,
    // rather than a later GPU change, must retire it on every iteration.
    for id in 1..=32 {
        let transient = f.surface();
        f.surface_feedback(&transient, id);
        f.surface_feedback(&transient, id + 32);
        assert_eq!(f.server.app.dmabuf_feedback_surfaces.len(), 2);
        transient.destroy();
        f.sync();
        assert_eq!(f.server.app.dmabuf_feedback_surfaces.len(), 1);
        assert!(f.server.app.dmabuf_feedback_surfaces[0].is_alive());
    }
    f.assert_feedback(0, 1, GPU_A, &[a]);
    survivor.destroy();
    f.sync();
    assert!(f.server.app.dmabuf_feedback_surfaces.is_empty());
    assert!(f.connection.protocol_error().is_none());
}

#[test]
fn app_surface_feedback_registry_retires_disconnected_clients() {
    let mut f = Fixture::new(Some((GPU_A, vec![format(Fourcc::Argb8888)])), 4);
    let surface = f.surface();
    f.surface_feedback(&surface, 0);
    assert_eq!(f.server.app.dmabuf_feedback_surfaces.len(), 1);
    // Drop the client proxy without sending wl_surface.destroy, then close the
    // connection. The production destruction callback must cover this path too.
    drop(surface);
    let (_display, server) = f.disconnect();
    assert!(server.app.dmabuf_feedback_surfaces.is_empty());
}
