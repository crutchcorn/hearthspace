use crate::fixture::{Api, DP8, DP9, FORMAT, Fixture};
use smithay::{
    backend::renderer::{
        Bind, Color32F, Frame, Renderer,
        multigpu::{GpuManager, TransferCache},
    },
    utils::{Rectangle, Transform},
};

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn replacing_source_context_on_same_drm_node_reallocates() {
    let mut fixture = Fixture::new();
    let mut cache = TransferCache::default();
    let first = fixture.draw(Some(&mut cache), DP8, FORMAT);
    fixture.edit_device(fixture.source, |device| device.replace_context());
    let replacement = fixture.draw(Some(&mut cache), DP8, FORMAT);
    assert_ne!(first, replacement);
    assert!(first.upgrade().is_none());
    assert_eq!(fixture.draw(Some(&mut cache), DP8, FORMAT), replacement);
    assert_eq!(fixture.allocations(), 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn replacing_target_context_on_same_drm_node_renegotiates() {
    let mut fixture = Fixture::new();
    let mut cache = TransferCache::default();
    let first = fixture.draw(Some(&mut cache), (32, 24), FORMAT);
    fixture.edit_device(fixture.target, |device| {
        device.replace_context();
        device.renderer.reject_dma = true;
    });
    let replacement = fixture.draw(Some(&mut cache), (32, 24), FORMAT);
    assert_ne!(first, replacement);
    assert!(first.upgrade().is_none());
    assert_eq!(
        fixture.draw(Some(&mut cache), (32, 24), FORMAT),
        replacement
    );
    assert_eq!(fixture.allocations(), 3); // direct, rejected direct, CPU fallback
    let stats = fixture.stats.lock().unwrap();
    assert_eq!(stats.direct_samples.len(), 1);
    assert_eq!(stats.memory_samples, 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn changing_node_pair_with_unchanged_contexts_rebinds_the_cache() {
    let mut fixture = Fixture::new();
    let mut cache = TransferCache::default();
    let original = fixture.draw(Some(&mut cache), DP8, FORMAT);
    for device in fixture.manager.devices_mut().unwrap() {
        device.node = if device.node == fixture.source {
            fixture.target
        } else {
            fixture.source
        };
    }
    std::mem::swap(&mut fixture.source, &mut fixture.target);
    let rebound = fixture.draw(Some(&mut cache), DP8, FORMAT);
    assert_ne!(original, rebound);
    assert!(original.upgrade().is_none());
    assert_eq!(fixture.draw(Some(&mut cache), DP8, FORMAT), rebound);
    assert_eq!(fixture.allocations(), 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn owner_drop_releases_its_intermediate_without_retirement_growth() {
    let mut fixture = Fixture::new();
    let mut survivor = TransferCache::default();
    let stable = fixture.draw(Some(&mut survivor), DP8, FORMAT);
    for _ in 0..64 {
        let retired = {
            let mut output = TransferCache::default();
            let buffer = fixture.draw(Some(&mut output), DP9, FORMAT);
            assert!(buffer.upgrade().is_some());
            buffer
        };
        assert!(retired.upgrade().is_none());
        assert_eq!(fixture.draw(Some(&mut survivor), DP8, FORMAT), stable);
        assert_eq!(
            fixture
                .stats
                .lock()
                .unwrap()
                .allocations
                .iter()
                .filter(|buffer| buffer.upgrade().is_some())
                .count(),
            1,
        );
    }
    drop(survivor);
    assert!(stable.upgrade().is_none());
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn manager_invalidation_requires_explicit_caller_cache_clear() {
    let mut fixture = Fixture::new();
    let pair = fixture.draw(None, DP8, FORMAT);
    let mut cache = TransferCache::default();
    let external = fixture.draw(Some(&mut cache), DP8, FORMAT);
    fixture.manager.invalidate_caches().unwrap();
    assert!(pair.upgrade().is_none());
    assert!(external.upgrade().is_some());
    cache.clear();
    assert!(external.upgrade().is_none());
    assert_ne!(fixture.draw(Some(&mut cache), DP8, FORMAT), external);
    assert_eq!(fixture.allocations(), 3);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn renderer_invalidation_clears_the_selected_owner_only() {
    let mut fixture = Fixture::new();
    let mut a = TransferCache::default();
    let mut b = TransferCache::default();
    let first = fixture.draw(Some(&mut a), DP8, FORMAT);
    let second = fixture.draw(Some(&mut b), DP9, FORMAT);
    fixture
        .manager
        .renderer(&fixture.source, &fixture.target, FORMAT)
        .unwrap()
        .with_transfer_cache(&mut a)
        .invalidate_caches()
        .unwrap();
    assert!(first.upgrade().is_none());
    assert_eq!(fixture.draw(Some(&mut b), DP9, FORMAT), second);
    assert_ne!(fixture.draw(Some(&mut a), DP8, FORMAT), first);
    assert_eq!(fixture.allocations(), 3);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn separate_managers_share_the_builder_without_sharing_context_identity() {
    let mut source = Fixture::new();
    let mut target = Fixture::new();
    let mut cache = TransferCache::default();
    let old = source.draw(Some(&mut cache), (32, 24), FORMAT);
    // A real cross_renderer borrows independently-lived managers. Its target
    // node is unchanged, but the replacement target renderer has another context.
    let mut scanout = old.upgrade().unwrap();
    let mut renderer = GpuManager::<Api>::cross_renderer(
        &mut source.manager,
        &mut target.manager,
        &source.source,
        &target.target,
        FORMAT,
    )
    .unwrap()
    .with_transfer_cache(&mut cache);
    let mut framebuffer = renderer.bind(&mut scanout).unwrap();
    let mut frame = renderer
        .render(&mut framebuffer, (32, 24).into(), Transform::Normal)
        .unwrap();
    frame
        .clear(
            Color32F::TRANSPARENT,
            &[Rectangle::from_size((32, 24).into())],
        )
        .unwrap();
    frame.finish().unwrap().wait().unwrap();
    drop(framebuffer);
    drop(renderer);
    let new = source.last_intermediate();
    assert_ne!(old, new);
    assert_eq!(source.allocations(), 2);
    assert_eq!(
        target.stats.lock().unwrap().direct_samples.last(),
        Some(&new)
    );
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn single_renderer_does_not_allocate_or_consume_the_transfer_cache() {
    let mut fixture = Fixture::new();
    let mut cache = TransferCache::default();
    let intermediate = fixture.draw(Some(&mut cache), (32, 24), FORMAT);
    let mut scanout = intermediate.upgrade().unwrap();
    let mut renderer = fixture
        .manager
        .single_renderer(&fixture.source)
        .unwrap()
        .with_transfer_cache(&mut cache);
    let mut framebuffer = renderer.bind(&mut scanout).unwrap();
    let mut frame = renderer
        .render(&mut framebuffer, (32, 24).into(), Transform::Normal)
        .unwrap();
    frame
        .clear(
            Color32F::TRANSPARENT,
            &[Rectangle::from_size((32, 24).into())],
        )
        .unwrap();
    frame.finish().unwrap().wait().unwrap();
    drop(framebuffer);
    drop(renderer);
    drop(scanout);
    assert_eq!(
        fixture.draw(Some(&mut cache), (32, 24), FORMAT),
        intermediate
    );
    assert_eq!(fixture.allocations(), 1);
}
