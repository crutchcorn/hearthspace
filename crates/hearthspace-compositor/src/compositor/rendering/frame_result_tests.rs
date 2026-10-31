//! Preserve the GPU completion dependency when owning Smithay's render result.
use std::{
    os::unix::io::OwnedFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use smithay::{
    backend::renderer::{
        damage::OutputDamageTracker,
        element::solid::SolidColorRenderElement,
        sync::{Fence, Interrupted},
    },
    utils::{Size, Transform},
};

use super::{Physical, Rectangle, RenderOutputResult, RenderedFrame, SyncPoint};

#[derive(Debug)]
struct PendingFence(Arc<AtomicBool>);

impl Fence for PendingFence {
    fn is_signaled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn wait(&self) -> Result<(), Interrupted> {
        panic!("owning a render result must not wait on the compositor thread");
    }

    fn is_exportable(&self) -> bool {
        false
    }

    fn export(&self) -> Option<OwnedFd> {
        None
    }
}

#[test]
fn final_composition_preserves_its_pending_fence_and_owned_damage() {
    let reached = Arc::new(AtomicBool::new(false));
    let region = Rectangle::<i32, Physical>::new((4, 8).into(), (16, 32).into());
    let frame = {
        let damage = vec![region];
        RenderedFrame::from(RenderOutputResult {
            sync: SyncPoint::from(PendingFence(Arc::clone(&reached))),
            damage: Some(&damage),
            states: Default::default(),
        })
    };

    assert_eq!(frame.damage, Some(vec![region]));
    assert!(frame.sync.contains_fence());
    assert!(
        !frame.sync.is_reached(),
        "submission must retain the GPU wait"
    );
    reached.store(true, Ordering::SeqCst);
    assert!(
        frame.sync.is_reached(),
        "retain the original completion fence"
    );
}

#[test]
fn unchanged_composition_keeps_no_damage_and_signaled_completion() {
    let size = Size::<i32, Physical>::from((64, 64));
    let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
    let elements: [SolidColorRenderElement; 0] = [];
    let (damage, states) = tracker.damage_output(0, &elements).unwrap();
    let first = RenderedFrame::from(RenderOutputResult {
        sync: SyncPoint::signaled(),
        damage,
        states,
    });
    assert_eq!(first.damage, Some(vec![Rectangle::from_size(size)]));
    let (damage, states) = tracker.damage_output(1, &elements).unwrap();
    // Smithay returns an already-signaled sync point when it skips rendering.
    let unchanged = RenderedFrame::from(RenderOutputResult {
        sync: SyncPoint::signaled(),
        damage,
        states,
    });

    assert!(unchanged.damage.is_none());
    assert!(!unchanged.sync.contains_fence());
    assert!(unchanged.sync.is_reached());
}
