//! Production mailbox and output scheduling with external device operations
//! substituted. Native acceptance still owns actual DRM/GL behavior.
use super::super::cursor_snapshot::CursorSnapshot;
use super::{
    CursorUpdate,
    mailbox::Mailbox,
    output::{Completion, RenderOutcome, Schedule, SubmissionOps},
};
use smithay::{
    backend::drm::{DrmEventMetadata, DrmEventTime},
    backend::{
        allocator::Fourcc,
        renderer::element::{Id, memory::MemoryBuffer},
    },
    reexports::calloop::{EventLoop, channel, ping::make_ping},
    utils::{Clock, Monotonic},
};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

fn cursor(sequence: u64, epoch: u64, image_id: &Id) -> CursorUpdate {
    CursorUpdate {
        sequence,
        security_epoch: epoch,
        output: None,
        snapshot: Some(CursorSnapshot {
            image_id: image_id.clone(),
            revision: 7,
            image: MemoryBuffer::from_slice(&[0, 0, 255, 255], Fourcc::Argb8888, (1, 1)),
            location: (sequence as i32, 12).into(),
        }),
    }
}

#[test]
fn motion_burst_coalesces_without_changing_image_revision() {
    let (ping, _source) = make_ping().unwrap();
    let mailbox = Mailbox::new(ping);
    let id = Id::new();
    for sequence in 1..=1000 {
        mailbox.update_cursor(cursor(sequence, 0, &id));
    }
    let latest = mailbox.take().cursor.unwrap();
    assert_eq!(latest.sequence, 1000);
    let snapshot = latest.snapshot.unwrap();
    assert_eq!(snapshot.image_id, id);
    assert_eq!(snapshot.revision, 7);
    assert_eq!(snapshot.location.x, 1000);
    assert!(mailbox.take().cursor.is_none());
}

#[test]
fn pause_and_security_revoke_a_frame_prepared_before_the_transition() {
    let (ping, _source) = make_ping().unwrap();
    let mailbox = Mailbox::new(ping);
    let prepared = mailbox.current();
    assert!(mailbox.permits(&prepared));
    let paused = mailbox.set_active(false);
    assert!(!mailbox.permits(&prepared));
    let resumed = mailbox.set_active(true);
    assert!(resumed > paused);
    assert!(!mailbox.permits(&prepared));
    let prepared = mailbox.current();
    mailbox.set_security(1, Some(9));
    assert!(!mailbox.permits(&prepared));
    mailbox.update_cursor(cursor(2, 0, &Id::new()));
    assert!(mailbox.take().cursor.is_none());
}

#[test]
fn stopped_presenter_cannot_be_reactivated_or_reconfigured() {
    let (ping, _source) = make_ping().unwrap();
    let mailbox = Mailbox::new(ping);
    let generation = mailbox.configure(Vec::new());
    mailbox.stop();
    assert_eq!(mailbox.set_active(true), generation);
    assert_eq!(mailbox.configure(Vec::new()), generation);
    mailbox.update_cursor(cursor(1, 0, &Id::new()));
    let stopped = mailbox.take();
    assert!(stopped.control.stopped);
    assert!(!stopped.control.active);
    assert!(stopped.cursor.is_none());
    assert!(!mailbox.permits(&stopped.control));
}

#[test]
fn latest_configuration_and_lifecycle_generation_survive_coalescing() {
    let (ping, _source) = make_ping().unwrap();
    let mailbox = Mailbox::new(ping);
    let first = mailbox.configure(Vec::new());
    let pause = mailbox.set_active(false);
    let latest = mailbox.configure(Vec::new());
    assert!(first < pause && pause < latest);
    let control = mailbox.take().control;
    assert_eq!(control.generation, latest);
    assert_eq!(control.configuration, latest);
    assert!(!control.active);
    // Configuration never silently reactivates a revoked seat.
    assert!(!mailbox.permits(&control));
}

struct DeviceOps {
    resets: usize,
    unchanged: bool,
    latest: CursorUpdate,
    queued: Option<CursorUpdate>,
}

impl SubmissionOps for DeviceOps {
    fn reset_state(&mut self) -> Result<(), String> {
        self.resets += 1;
        self.unchanged = false;
        Ok(())
    }
    fn render_and_queue(&mut self) -> Result<RenderOutcome, String> {
        if self.unchanged {
            return Ok(RenderOutcome::Empty);
        }
        assert!(
            self.queued.is_none(),
            "only one device submission may be pending"
        );
        self.queued = Some(self.latest.clone());
        self.unchanged = true;
        Ok(RenderOutcome::Queued)
    }
}

fn metadata() -> DrmEventMetadata {
    DrmEventMetadata {
        time: DrmEventTime::Monotonic(Clock::<Monotonic>::new().now().into()),
        sequence: 9,
    }
}

#[test]
fn warmed_unchanged_black_requires_a_real_submission_for_each_security_generation() {
    let mut schedule = Schedule::default();
    let mut device = DeviceOps {
        resets: 0,
        unchanged: false,
        latest: cursor(1, 0, &Id::new()),
        queued: None,
    };
    let refresh = Duration::from_millis(16);
    schedule
        .submit(Instant::now(), refresh, &mut device)
        .unwrap();
    assert!(matches!(
        schedule
            .complete(Some(&metadata()), || Ok(device.queued.take()))
            .unwrap(),
        Completion::Presented(_)
    ));
    schedule.dirty = true;
    schedule
        .submit(Instant::now(), refresh, &mut device)
        .unwrap();
    assert!(!schedule.pending);
    assert!(schedule.opportunity_at.is_some());
    // The same lifecycle operation used by Worker::apply_security must override
    // the warmed no-damage result. Each generation needs its own actual queue.
    for epoch in 1..=3 {
        device.latest.security_epoch = epoch;
        schedule.invalidate_security();
        schedule
            .submit(Instant::now(), refresh, &mut device)
            .unwrap();
        assert!(schedule.pending);
        assert!(schedule.opportunity_at.is_none());
        let Completion::Presented(submitted) = schedule
            .complete(Some(&metadata()), || Ok(device.queued.take()))
            .unwrap()
        else {
            panic!("missing physical completion")
        };
        assert_eq!(submitted.security_epoch, epoch);
    }
    assert_eq!(device.resets, 3);
}

#[test]
fn stale_completion_does_not_retire_pending_work_and_untimed_requires_retry() {
    let mut schedule = Schedule::default();
    let mut device = DeviceOps {
        resets: 0,
        unchanged: false,
        latest: cursor(1, 0, &Id::new()),
        queued: None,
    };
    let refresh = Duration::from_millis(16);
    schedule
        .submit(Instant::now(), refresh, &mut device)
        .unwrap();
    let stale = DrmEventMetadata {
        time: DrmEventTime::Monotonic(Duration::from_micros(1)),
        sequence: 2,
    };
    assert!(matches!(
        schedule
            .complete::<()>(Some(&stale), || panic!("stale event reached device"))
            .unwrap(),
        Completion::Ignored
    ));
    assert!(schedule.pending);
    let untimed = DrmEventMetadata {
        time: DrmEventTime::Monotonic(Duration::ZERO),
        sequence: 0,
    };
    assert!(matches!(
        schedule
            .complete(Some(&untimed), || Ok(device.queued.take()))
            .unwrap(),
        Completion::Untimed
    ));
    schedule
        .submit(Instant::now(), refresh, &mut device)
        .unwrap();
    assert!(schedule.pending);
    assert_eq!(device.resets, 1);
}

#[test]
fn security_barrier_survives_an_old_pending_flip_and_revoked_preparation() {
    struct Revoked {
        resets: usize,
    }
    impl SubmissionOps for Revoked {
        fn reset_state(&mut self) -> Result<(), String> {
            self.resets += 1;
            Ok(())
        }
        fn render_and_queue(&mut self) -> Result<RenderOutcome, String> {
            Ok(RenderOutcome::Revoked)
        }
    }
    let mut schedule = Schedule::default();
    let mut device = DeviceOps {
        resets: 0,
        unchanged: false,
        latest: cursor(1, 0, &Id::new()),
        queued: None,
    };
    let refresh = Duration::from_millis(16);
    schedule
        .submit(Instant::now(), refresh, &mut device)
        .unwrap();
    schedule.invalidate_security();
    schedule
        .submit(Instant::now(), refresh, &mut device)
        .unwrap();
    assert_eq!(device.resets, 0, "pending flip must retire before reset");
    schedule
        .complete(Some(&metadata()), || Ok(device.queued.take()))
        .unwrap();
    let mut revoked = Revoked { resets: 0 };
    schedule
        .submit(Instant::now(), refresh, &mut revoked)
        .unwrap();
    assert_eq!(revoked.resets, 1);
    assert!(!schedule.pending);
    assert!(schedule.dirty);
    schedule
        .submit(Instant::now(), refresh, &mut device)
        .unwrap();
    assert_eq!(
        device.resets, 1,
        "revocation must not consume forced-commit latch"
    );
    assert!(schedule.pending);
}

#[test]
fn device_event_owner_submits_latest_motion_without_main_dispatch() {
    let (ping, source) = make_ping().unwrap();
    let mailbox = Arc::new(Mailbox::new(ping));
    let worker_mailbox = mailbox.clone();
    let (device_tx, device_rx) = channel::channel::<()>();
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (verified_tx, verified_rx) = mpsc::channel();
    let id = Id::new();
    let initial = cursor(0, 0, &id);
    let worker = std::thread::spawn(move || {
        // Use the exact scheduling/completion boundary that Worker::turn and
        // Worker::vblank call. Only the external device operations are fake.
        let mut event_loop = EventLoop::<(Schedule, DeviceOps)>::try_new().unwrap();
        let signal = event_loop.get_signal();
        let timeout_signal = signal.clone();
        event_loop
            .handle()
            .insert_source(source, move |(), &mut (), (schedule, device)| {
                if let Some(cursor) = worker_mailbox.take().cursor {
                    device.latest = cursor;
                    device.unchanged = false;
                    schedule.dirty = true;
                    schedule
                        .submit(Instant::now(), Duration::from_millis(16), device)
                        .unwrap();
                    accepted_tx.send(device.latest.sequence).unwrap();
                }
            })
            .unwrap();
        event_loop
            .handle()
            .insert_source(device_rx, move |event, _, (schedule, device)| match event {
                channel::Event::Msg(()) => {
                    if let Completion::Presented(submitted) = schedule
                        .complete(Some(&metadata()), || Ok(device.queued.take()))
                        .unwrap()
                    {
                        verified_tx.send(submitted).unwrap();
                    }
                    schedule
                        .submit(Instant::now(), Duration::from_millis(16), device)
                        .unwrap();
                }
                channel::Event::Closed => signal.stop(),
            })
            .unwrap();
        let mut state = (
            Schedule::default(),
            DeviceOps {
                resets: 0,
                unchanged: false,
                latest: initial,
                queued: None,
            },
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        event_loop
            .run(Some(Duration::from_millis(50)), &mut state, |_| {
                if Instant::now() >= deadline {
                    timeout_signal.stop();
                }
            })
            .unwrap();
    });
    mailbox.update_cursor(cursor(1, 0, &id));
    assert_eq!(accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    for sequence in 2..=1000 {
        mailbox.update_cursor(cursor(sequence, 0, &id));
    }
    while accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap() != 1000 {}
    device_tx.send(()).unwrap();
    let first = verified_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(first.sequence, 1); // Latest motion did not rewrite submitted identity.
    device_tx.send(()).unwrap();
    let latest = verified_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(latest.sequence, 1000);
    let snapshot = latest.snapshot.unwrap();
    assert_eq!((snapshot.image_id, snapshot.revision), (id, 7));
    drop(device_tx);
    worker.join().unwrap();
}

#[test]
fn synchronous_modeset_requires_a_fresh_flip_without_new_input() {
    struct Modeset {
        primed: bool,
        resets: usize,
    }
    impl SubmissionOps for Modeset {
        fn reset_state(&mut self) -> Result<(), String> {
            self.resets += 1;
            Ok(())
        }
        fn render_and_queue(&mut self) -> Result<RenderOutcome, String> {
            Ok(if std::mem::replace(&mut self.primed, true) {
                RenderOutcome::Queued
            } else {
                RenderOutcome::Primed
            })
        }
    }
    let now = Instant::now();
    let refresh = Duration::from_millis(16);
    let mut schedule = Schedule::default();
    let mut device = Modeset {
        primed: false,
        resets: 0,
    };
    schedule.submit(now, refresh, &mut device).unwrap();
    assert!(!schedule.pending, "modeset must not await a vblank event");
    assert!(schedule.dirty);
    assert_eq!(schedule.opportunity_at, Some(now));
    assert!(matches!(
        schedule
            .complete::<()>(None, || panic!("no event was queued"))
            .unwrap(),
        Completion::Ignored
    ));
    schedule.submit(now, refresh, &mut device).unwrap();
    assert!(
        schedule.pending,
        "fresh flip must await verified completion"
    );
    assert!(!schedule.dirty);
    assert_eq!(
        device.resets, 1,
        "unchanged pixels must still produce a flip"
    );
}
