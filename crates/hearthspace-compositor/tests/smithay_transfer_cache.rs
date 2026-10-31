//! Deterministic integration tests of Smithay's real multi-GPU transfer path.
//! Device/allocator substitutes do not establish GPU import or native performance.
#![cfg(feature = "udev")]

#[path = "support/transfer_cache/completion.rs"]
mod completion;
#[path = "support/transfer_cache/mod.rs"]
mod fixture;
#[path = "support/transfer_cache/lifecycle.rs"]
mod lifecycle;
#[path = "../src/compositor/udev/transfer/cache.rs"]
mod output_transfer;

use fixture::{DP8, DP9, FORMAT, Fixture};
use smithay::backend::{allocator::Fourcc, renderer::multigpu::TransferCache};

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn alternating_output_sizes_allocate_exactly_two_intermediates() {
    let mut fixture = Fixture::new();
    let mut landscape = TransferCache::default();
    let mut portrait = TransferCache::default();
    let first = fixture.draw(Some(&mut landscape), DP8, FORMAT);
    let second = fixture.draw(Some(&mut portrait), DP9, FORMAT);
    assert_ne!(first, second);
    for _ in 0..128 {
        assert_eq!(fixture.draw(Some(&mut landscape), DP8, FORMAT), first);
        assert_eq!(fixture.draw(Some(&mut portrait), DP9, FORMAT), second);
    }
    assert_eq!(fixture.allocations(), 2);
    let stats = fixture.stats.lock().unwrap();
    assert_eq!(stats.direct_samples.len(), 258);
    for samples in stats.direct_samples.chunks_exact(2) {
        assert_eq!(samples, &[first.clone(), second.clone()]);
    }
    assert_eq!(stats.downloads, 0);
    assert_eq!(stats.uploads, 0);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn default_pair_cache_still_recreates_on_every_output_size_switch() {
    let mut fixture = Fixture::new();
    for _ in 0..32 {
        fixture.draw(None, DP8, FORMAT);
        fixture.draw(None, DP9, FORMAT);
    }
    assert_eq!(fixture.allocations(), 64);
    let last = fixture.draw(None, DP9, FORMAT);
    assert_eq!(fixture.allocations(), 64);
    assert_eq!(fixture.draw(None, DP9, FORMAT), last);
    let reformatted = fixture.draw(None, DP9, Fourcc::Xrgb8888);
    assert_ne!(last, reformatted);
    assert_eq!(fixture.allocations(), 65);
    assert_eq!(fixture.draw(None, DP9, Fourcc::Xrgb8888), reformatted);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn size_and_fourcc_changes_replace_only_that_owners_intermediate() {
    let mut fixture = Fixture::new();
    let mut first = TransferCache::default();
    let mut other = TransferCache::default();
    let old = fixture.draw(Some(&mut first), DP8, FORMAT);
    let unaffected = fixture.draw(Some(&mut other), DP9, FORMAT);
    let resized = fixture.draw(Some(&mut first), (1920, 1080), FORMAT);
    assert_ne!(old, resized);
    assert!(old.upgrade().is_none());
    assert_eq!(fixture.draw(Some(&mut other), DP9, FORMAT), unaffected);
    let reformatted = fixture.draw(Some(&mut first), (1920, 1080), Fourcc::Xrgb8888);
    assert_ne!(resized, reformatted);
    assert!(resized.upgrade().is_none());
    assert_eq!(
        fixture.draw(Some(&mut first), (1920, 1080), Fourcc::Xrgb8888),
        reformatted
    );
    assert_eq!(fixture.draw(Some(&mut other), DP9, FORMAT), unaffected);
    assert_eq!(fixture.allocations(), 4);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn same_size_owners_keep_distinct_storage_and_leave_default_cache_alone() {
    let mut fixture = Fixture::new();
    let pair = fixture.draw(None, DP8, FORMAT);
    let mut a = TransferCache::default();
    let mut b = TransferCache::default();
    let first = fixture.draw(Some(&mut a), DP8, FORMAT);
    let second = fixture.draw(Some(&mut b), DP8, FORMAT);
    assert_ne!(first, second);
    assert_ne!(pair, first);
    assert_ne!(pair, second);
    for _ in 0..32 {
        assert_eq!(fixture.draw(Some(&mut a), DP8, FORMAT), first);
        assert_eq!(fixture.draw(Some(&mut b), DP8, FORMAT), second);
        assert_eq!(fixture.draw(None, DP8, FORMAT), pair);
    }
    assert_eq!(fixture.allocations(), 3);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn real_cpu_fallback_reuses_its_own_intermediate_and_copy_branch() {
    let mut fixture = Fixture::new();
    fixture.edit_device(fixture.target, |device| device.renderer.reject_dma = true);
    let mut a = TransferCache::default();
    let mut b = TransferCache::default();
    let first = fixture.draw(Some(&mut a), (32, 24), FORMAT);
    let second = fixture.draw(Some(&mut b), (24, 32), FORMAT);
    for _ in 0..32 {
        assert_eq!(fixture.draw(Some(&mut a), (32, 24), FORMAT), first);
        assert_eq!(fixture.draw(Some(&mut b), (24, 32), FORMAT), second);
    }
    // Each owner attempts one shared allocation, whose target import fails,
    // then allocates one CPU-copy intermediate. That branch stays cached.
    assert_eq!(fixture.allocations(), 4);
    let stats = fixture.stats.lock().unwrap();
    assert_eq!(stats.dma_imports.len(), 2);
    assert_eq!(stats.downloads, 66);
    assert_eq!(stats.uploads, 66);
    assert_eq!(stats.memory_samples, 66);
    assert!(stats.direct_samples.is_empty());
    assert!(stats.allocations[0].upgrade().is_none());
    assert!(stats.allocations[2].upgrade().is_none());
}
