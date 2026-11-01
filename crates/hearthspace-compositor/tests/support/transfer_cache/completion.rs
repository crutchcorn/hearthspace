use crate::{
    fixture::{DP8, DP9, FORMAT, Fixture, ManualFence},
    output_transfer::OutputTransfer,
};
use smithay::backend::renderer::sync::SyncPoint;
use std::{io, sync::atomic::Ordering};

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn alternating_outputs_wait_only_their_own_previous_target_completion() {
    let mut fixture = Fixture::new();
    let mut a = OutputTransfer::default();
    let mut b = OutputTransfer::default();
    fixture.draw_owned(&mut a, DP8, false).unwrap();
    let first = fixture.last_intermediate();
    let a_fence = fixture.last_fence();
    fixture.draw_owned(&mut b, DP9, false).unwrap();
    let second = fixture.last_intermediate();
    let b_fence = fixture.last_fence();
    assert_eq!(a_fence.waits.load(Ordering::SeqCst), 0);
    assert_eq!(b_fence.waits.load(Ordering::SeqCst), 0);
    fixture.draw_owned(&mut a, DP8, false).unwrap();
    assert_eq!(fixture.last_intermediate(), first);
    assert_eq!(a_fence.waits.load(Ordering::SeqCst), 1);
    assert_eq!(b_fence.waits.load(Ordering::SeqCst), 0);
    fixture.draw_owned(&mut b, DP9, false).unwrap();
    assert_eq!(fixture.last_intermediate(), second);
    assert_eq!(b_fence.waits.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.allocations(), 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn failed_wait_blocks_storage_rewrite_and_retains_the_dependency_for_retry() {
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    let buffer = fixture.last_intermediate();
    let fence = fixture.last_fence();
    fence.interrupted.store(true, Ordering::SeqCst);
    for _ in 0..2 {
        assert!(fixture.draw_owned(&mut owner, DP8, false).is_err());
    }
    assert_eq!(fixture.stats.lock().unwrap().source_writes.len(), 1);
    assert_eq!(fixture.allocations(), 1);
    assert!(buffer.upgrade().is_some());
    fence.interrupted.store(false, Ordering::SeqCst);
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert_eq!(fixture.last_intermediate(), buffer);
    assert_eq!(fence.waits.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.allocations(), 1);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn source_draw_error_still_finishes_transfer_and_retains_target_fence() {
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    fixture.edit_device(fixture.source, |device| device.renderer.fail_draw = true);
    assert!(fixture.draw_owned(&mut owner, DP8, false).is_err());
    let buffer = fixture.last_intermediate();
    let fence = fixture.last_fence();
    fixture.edit_device(fixture.source, |device| device.renderer.fail_draw = false);
    fence.interrupted.store(true, Ordering::SeqCst);
    assert!(fixture.draw_owned(&mut owner, DP8, false).is_err());
    assert_eq!(fixture.stats.lock().unwrap().source_writes.len(), 1);
    fence.interrupted.store(false, Ordering::SeqCst);
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert_eq!(fixture.last_intermediate(), buffer);
    assert_eq!(fixture.allocations(), 1);
    assert_eq!(fence.waits.load(Ordering::SeqCst), 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn readback_failure_does_not_drop_target_completion() {
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    fixture.edit_device(fixture.target, |device| {
        device.renderer.fail_readback = true
    });
    let submitted = fixture.draw_owned(&mut owner, DP8, true).unwrap();
    assert!(submitted.readback.is_err());
    let buffer = fixture.last_intermediate();
    let fence = submitted.sync.get::<ManualFence>().unwrap().clone();
    drop(submitted);
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert_eq!(fixture.last_intermediate(), buffer);
    assert_eq!(fence.waits.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.allocations(), 1);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn rejected_kms_submission_does_not_drop_target_completion() {
    fn reject_queue(sync: SyncPoint) -> Result<(), io::Error> {
        // Substitute only the consuming KMS submission boundary. The owner
        // already retained the real MultiFrame completion before this call.
        drop(sync);
        Err(io::Error::other("injected KMS queue failure"))
    }
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    let submitted = fixture.draw_owned(&mut owner, DP8, false).unwrap();
    let buffer = fixture.last_intermediate();
    let fence = fixture.last_fence();
    assert!(reject_queue(submitted.sync).is_err());
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert_eq!(fixture.last_intermediate(), buffer);
    assert_eq!(fence.waits.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.allocations(), 1);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn failed_target_finish_discards_storage_with_no_trustworthy_completion() {
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    let buffer = fixture.last_intermediate();
    fixture.edit_device(fixture.target, |device| device.renderer.fail_finish = true);
    assert!(fixture.draw_owned(&mut owner, DP8, false).is_err());
    assert!(buffer.upgrade().is_none());
    fixture.edit_device(fixture.target, |device| device.renderer.fail_finish = false);
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert_ne!(fixture.last_intermediate(), buffer);
    assert_eq!(fixture.allocations(), 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn failed_render_discards_prepared_storage_before_retry() {
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    fixture.edit_device(fixture.source, |device| device.renderer.fail_render = true);
    assert!(fixture.draw_owned(&mut owner, DP8, false).is_err());
    let buffer = fixture.stats.lock().unwrap().allocations[0].clone();
    assert!(buffer.upgrade().is_none());
    fixture.edit_device(fixture.source, |device| device.renderer.fail_render = false);
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert_ne!(fixture.last_intermediate(), buffer);
    assert_eq!(fixture.allocations(), 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn missing_completion_cannot_reuse_even_a_populated_cache() {
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    let buffer = fixture.draw(Some(&mut owner.cache), DP8, FORMAT);
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert!(buffer.upgrade().is_none());
    assert_ne!(fixture.last_intermediate(), buffer);
    assert_eq!(fixture.allocations(), 2);
}

#[test]
#[ignore = "requires two DRM sysfs node identities"]
fn explicit_reset_releases_storage_and_fence_as_one_owner() {
    let mut fixture = Fixture::new();
    let mut owner = OutputTransfer::default();
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    let buffer = fixture.last_intermediate();
    let old_fence = fixture.last_fence();
    old_fence.interrupted.store(true, Ordering::SeqCst);
    owner.reset();
    assert!(buffer.upgrade().is_none());
    fixture.draw_owned(&mut owner, DP8, false).unwrap();
    assert_ne!(fixture.last_intermediate(), buffer);
    assert_eq!(old_fence.waits.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.allocations(), 2);
}
