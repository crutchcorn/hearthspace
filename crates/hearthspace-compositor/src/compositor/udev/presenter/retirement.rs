use super::{OutputKey, PresenterEvent, SceneFrame};
use smithay::backend::renderer::{gles::GlesTexture, sync::SyncPoint};

pub(super) struct Scene<F = SceneFrame> {
    pub frame: F,
    pub texture: Option<GlesTexture>,
    pub read_sync: Option<SyncPoint>,
    pub capture_taken: bool,
    pub submitted: bool,
    pub uncertain: bool,
}

impl<F> Scene<F> {
    pub fn capture_result<T>(
        &mut self,
        result: Result<T, String>,
        finish: impl FnOnce() -> bool,
    ) -> Result<T, String> {
        if result.is_err() && !finish() {
            self.uncertain = true;
        }
        result
    }
}

impl From<SceneFrame> for Scene {
    fn from(frame: SceneFrame) -> Self {
        Self {
            frame,
            texture: None,
            read_sync: None,
            capture_taken: false,
            submitted: false,
            uncertain: false,
        }
    }
}

// The owner stays inside the same record as every dependency required to
// release it. Tests replace only this opaque resource with a drop-counted lease.
pub(super) struct Retained<T> {
    pub owner: T,
    pub key: OutputKey,
    pub sequence: u64,
    pub submitted: bool,
    pub producer: SyncPoint,
    pub reader: Option<SyncPoint>,
    pub uncertain: bool,
}

pub(super) struct Retirement<T = Scene> {
    scenes: Vec<Retained<T>>,
}

impl<T> Default for Retirement<T> {
    fn default() -> Self {
        Self { scenes: Vec::new() }
    }
}

impl Retirement {
    pub fn push(&mut self, scene: Scene, emit: impl FnOnce(PresenterEvent)) {
        self.retain(
            Retained {
                key: scene.frame.output,
                sequence: scene.frame.sequence,
                submitted: scene.submitted,
                producer: scene.frame.ready.clone(),
                reader: scene.read_sync.clone(),
                uncertain: scene.uncertain,
                owner: scene,
            },
            emit,
        );
    }
}

impl<T> Retirement<T> {
    pub fn retain(&mut self, scene: Retained<T>, emit: impl FnOnce(PresenterEvent)) {
        if !scene.submitted {
            emit(PresenterEvent::SceneDiscarded {
                key: scene.key,
                scene_sequence: scene.sequence,
            });
        }
        self.scenes.push(scene);
    }

    pub fn poll(&mut self, mut emit: impl FnMut(PresenterEvent)) {
        let mut index = 0;
        while index < self.scenes.len() {
            let scene = &self.scenes[index];
            if !scene.uncertain
                && scene.producer.is_reached()
                && scene.reader.as_ref().is_none_or(SyncPoint::is_reached)
            {
                let scene = self.scenes.swap_remove(index);
                let key = scene.key;
                drop(scene.owner);
                emit(PresenterEvent::SceneCapacityAvailable { key });
            } else {
                index += 1;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    pub fn has_uncertain(&self) -> bool {
        self.scenes.iter().any(|scene| scene.uncertain)
    }

    pub fn resolve_uncertain(&mut self) {
        for scene in &mut self.scenes {
            scene.uncertain = false;
        }
    }

    /// One recovery attempt per active turn. Callers enter recovery on false;
    /// no uncertain lease is dropped even when the external operation fails.
    pub fn recover_uncertain(&mut self, finish: impl FnOnce() -> bool) -> bool {
        if !self.has_uncertain() {
            return true;
        }
        if !finish() {
            return false;
        }
        self.resolve_uncertain();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        output::{Completion, RenderOutcome, Schedule, SubmissionOps},
        worker::prepare_active_output,
    };
    use super::*;
    use crate::compositor::udev::identity::DeviceToken;
    use smithay::{
        backend::{
            drm::{DrmEventMetadata, DrmEventTime},
            renderer::sync::{Fence, Interrupted},
        },
        utils::{Clock, Monotonic},
    };
    use std::{
        num::NonZeroU32,
        os::fd::OwnedFd,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    #[derive(Debug)]
    struct ProbeFence(Arc<AtomicBool>);
    impl Fence for ProbeFence {
        fn is_signaled(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
        fn wait(&self) -> Result<(), Interrupted> {
            panic!("retirement blocked on a fence")
        }
        fn is_exportable(&self) -> bool {
            false
        }
        fn export(&self) -> Option<OwnedFd> {
            None
        }
    }

    struct Lease(Arc<AtomicUsize>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    fn key() -> OutputKey {
        OutputKey {
            device: DeviceToken {
                id: 226,
                incarnation: 2,
            },
            crtc: NonZeroU32::new(7).unwrap().into(),
            generation: 3,
        }
    }

    #[test]
    fn canceled_generation_owns_its_lease_until_both_fences_complete() {
        let drops = Arc::new(AtomicUsize::new(0));
        let owner = Arc::new(Lease(drops.clone()));
        let weak = Arc::downgrade(&owner);
        let producer = Arc::new(AtomicBool::new(false));
        let reader = Arc::new(AtomicBool::new(false));
        let mut retirement = Retirement::default();
        retirement.retain(
            Retained {
                owner,
                key: key(),
                sequence: 42,
                submitted: false,
                producer: SyncPoint::from(ProbeFence(producer.clone())),
                reader: Some(SyncPoint::from(ProbeFence(reader.clone()))),
                uncertain: false,
            },
            |event| {
                assert!(matches!(
                    event,
                    PresenterEvent::SceneDiscarded {
                        scene_sequence: 42,
                        ..
                    }
                ))
            },
        );
        let mut released = 0;
        for dependency in [None, Some(&producer), Some(&reader)] {
            if let Some(dependency) = dependency {
                dependency.store(true, Ordering::Release);
            }
            retirement.poll(|event| {
                assert!(
                    matches!(event, PresenterEvent::SceneCapacityAvailable { key: k } if k == key())
                );
                assert!(
                    weak.upgrade().is_none(),
                    "capacity published before ownership release"
                );
                released += 1;
            });
            if !reader.load(Ordering::Acquire) {
                assert!(weak.upgrade().is_some());
                assert!(
                    !retirement.is_empty(),
                    "new pool must not be configured yet"
                );
                assert_eq!(drops.load(Ordering::Acquire), 0);
            }
        }
        assert_eq!(released, 1);
        assert!(retirement.is_empty());
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[test]
    fn failed_capture_recovery_retains_owner_then_success_releases_once() {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut retirement = Retirement::default();
        retirement.retain(
            Retained {
                owner: Lease(drops.clone()),
                key: key(),
                sequence: 7,
                submitted: true,
                producer: SyncPoint::signaled(),
                reader: None,
                uncertain: true,
            },
            |_| panic!("a submitted scene was canceled"),
        );
        assert!(!retirement.recover_uncertain(|| false));
        retirement.poll(|_| panic!("uncertain capture lease released"));
        assert_eq!(drops.load(Ordering::Acquire), 0);
        assert!(!retirement.is_empty());
        assert!(retirement.recover_uncertain(|| true));
        let mut released = 0;
        retirement.poll(|_| released += 1);
        retirement.poll(|_| panic!("lease released twice"));
        assert_eq!(released, 1);
        assert_eq!(drops.load(Ordering::Acquire), 1);
        assert!(retirement.is_empty());
    }

    #[test]
    fn active_capture_recovery_waits_for_completion_then_succeeds_or_fails_with_owner() {
        struct Queue;
        impl SubmissionOps for Queue {
            fn reset_state(&mut self) -> Result<(), String> {
                Ok(())
            }
            fn render_and_queue(&mut self) -> Result<RenderOutcome, String> {
                Ok(RenderOutcome::Queued)
            }
        }
        for recovered in [true, false] {
            let drops = Arc::new(AtomicUsize::new(0));
            let mut scene = Scene {
                frame: Lease(drops.clone()),
                texture: None,
                read_sync: None,
                capture_taken: false,
                submitted: false,
                uncertain: false,
            };
            let mut immediate_finishes = 0;
            let capture = scene.capture_result(Err::<(), _>("readback failed".to_owned()), || {
                immediate_finishes += 1;
                false
            });
            assert_eq!(capture.as_ref().unwrap_err(), "readback failed");
            assert_eq!(immediate_finishes, 1);
            assert!(scene.uncertain);
            let mut schedule = Schedule::default();
            schedule
                .submit(Instant::now(), Duration::from_millis(16), &mut Queue)
                .unwrap();
            assert!(schedule.pending);
            let mut events = Vec::new();
            assert!(!prepare_active_output(
                key(),
                &mut schedule,
                Some(&mut scene),
                || panic!("recovered before physical completion"),
                |event| events.push(event)
            ));
            assert!(events.is_empty());
            assert_eq!(drops.load(Ordering::Acquire), 0);

            let metadata = DrmEventMetadata {
                time: DrmEventTime::Monotonic(Clock::<Monotonic>::new().now().into()),
                sequence: 4,
            };
            // Even with failed capture readback, the successful queue has its
            // own verified completion before active recovery can affect output.
            assert!(matches!(
                schedule
                    .complete(Some(&metadata), || Ok(Some(capture)))
                    .unwrap(),
                Completion::Presented(Err(_))
            ));
            let mut recovery_calls = 0;
            let may_advance = prepare_active_output(
                key(),
                &mut schedule,
                Some(&mut scene),
                || {
                    recovery_calls += 1;
                    recovered
                },
                |event| events.push(event),
            );
            assert_eq!(recovery_calls, 1);
            assert_eq!(may_advance, recovered);
            assert_eq!(scene.uncertain, !recovered);
            assert_eq!(schedule.failed, !recovered);
            assert_eq!(
                drops.load(Ordering::Acquire),
                0,
                "active lease was released"
            );
            if recovered {
                assert!(events.is_empty());
                schedule.dirty = true;
                schedule
                    .submit(Instant::now(), Duration::from_millis(16), &mut Queue)
                    .unwrap();
                assert!(schedule.pending, "recovered output did not progress");
            } else {
                assert!(
                    matches!(events.as_slice(), [PresenterEvent::Failed { output: Some(k), .. }] if *k == key())
                );
                assert!(!prepare_active_output(
                    key(),
                    &mut schedule,
                    Some(&mut scene),
                    || panic!("failed output retried without recovery"),
                    |_| panic!("duplicate failure")
                ));
                schedule.dirty = true;
                schedule
                    .submit(Instant::now(), Duration::from_millis(16), &mut Queue)
                    .unwrap();
                assert!(!schedule.pending, "failed output continued submitting");
            }
            assert_eq!(drops.load(Ordering::Acquire), 0);
            drop(scene);
            assert_eq!(drops.load(Ordering::Acquire), 1);
        }
    }
}
