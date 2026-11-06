use super::super::{
    PresenterEvent,
    output::{Completion, Schedule},
};
use super::test_support::*;
use smithay::backend::drm::compositor::FrameFlags;
use std::{
    io::ErrorKind,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

fn submit(schedule: &mut Schedule, device: &mut DeviceOps) -> Result<(), String> {
    schedule.submit(Instant::now(), Duration::from_millis(16), device)
}

fn assert_retained_until_reads_finish(device: DeviceOps, uncertain: bool) {
    let drops = device.drops.clone();
    let reads = device.reads.clone();
    let mut retirement = device.retire();
    assert_eq!(drops.load(Ordering::Acquire), 0);
    let mut released = 0;
    let mut poll = |retirement: &mut super::super::retirement::Retirement<_>| {
        retirement.poll(|event| {
            assert!(
                matches!(event, PresenterEvent::SceneCapacityAvailable { key: k } if k == key())
            );
            assert_eq!(
                drops.load(Ordering::Acquire),
                1,
                "capacity preceded ownership release"
            );
            released += 1;
        });
    };
    poll(&mut retirement);
    assert!(!retirement.is_empty());
    for (index, read) in reads.iter().enumerate() {
        assert_eq!(drops.load(Ordering::Acquire), 0);
        read.store(true, Ordering::Release);
        poll(&mut retirement);
        if index + 1 != reads.len() || uncertain {
            assert!(!retirement.is_empty());
        }
    }
    if uncertain {
        assert_eq!(drops.load(Ordering::Acquire), 0);
        assert!(!retirement.recover_uncertain(|| false));
        poll(&mut retirement);
        assert_eq!(drops.load(Ordering::Acquire), 0);
        assert!(retirement.recover_uncertain(|| true));
        poll(&mut retirement);
    }
    assert!(retirement.is_empty());
    assert_eq!(released, 1);
    assert_eq!(drops.load(Ordering::Acquire), 1);
}

#[test]
fn selected_queue_rejection_gets_one_fresh_capture_and_exact_accepted_completion() {
    for rejection in [Reject::Access(ErrorKind::InvalidInput), Reject::TestFailed] {
        let mut device = DeviceOps::new([rejected(rejection), accepted()]);
        let mut schedule = Schedule::default();
        schedule.invalidate_security();
        submit(&mut schedule, &mut device).unwrap();
        assert_eq!(
            device.attempts,
            [super::scanout_flags(), FrameFlags::empty()]
        );
        assert_eq!(device.resets, 1);
        assert_eq!(device.state_resets, 1);
        assert_eq!(device.captures, 2);
        assert_eq!(device.queued_sequences, [1, 2]);
        assert_eq!(device.rejected_sequences, [1]);
        assert!(schedule.pending);
        assert!(device.scene.as_ref().unwrap().capture_taken);
        submit(&mut schedule, &mut device).unwrap();
        assert_eq!(
            device.attempts.len(),
            2,
            "pending submission admitted another attempt"
        );
        let Completion::Presented(frame) = schedule
            .complete(Some(&metadata()), || Ok(device.accepted.take()))
            .unwrap()
        else {
            panic!("accepted retry did not complete")
        };
        assert_eq!(
            (frame.sequence, frame.scene_sequence, frame.cursor_sequence),
            (2, 41, 72)
        );
        assert_eq!(frame.cursor_image, Some((device.image_id.clone(), 9)));
        assert!(!frame.diagnostics.cursor_plane);
        assert_eq!(frame.capture.unwrap().unwrap().pixels, [2; 4]);
        assert!(matches!(
            schedule
                .complete::<()>(Some(&metadata()), || panic!("duplicate acknowledgment"))
                .unwrap(),
            Completion::Ignored
        ));
        assert_retained_until_reads_finish(device, false);
    }
}

#[test]
fn nonselected_nonqueue_inactive_and_other_access_errors_never_retry() {
    let steps = [
        Step::Queue {
            selected: false,
            rejection: Some(Reject::TestFailed),
        },
        Step::Queue {
            selected: false,
            rejection: Some(Reject::Access(ErrorKind::InvalidInput)),
        },
        rejected(Reject::Inactive),
        rejected(Reject::Access(ErrorKind::PermissionDenied)),
        rejected(Reject::Access(ErrorKind::ResourceBusy)),
        rejected(Reject::Access(ErrorKind::WouldBlock)),
        rejected(Reject::Access(ErrorKind::Interrupted)),
        rejected(Reject::Access(ErrorKind::Other)),
        Step::RenderError,
    ];
    for step in steps {
        let mut device = DeviceOps::new([step, accepted()]);
        let mut schedule = Schedule::default();
        assert!(submit(&mut schedule, &mut device).is_err());
        assert_eq!(device.attempts, [super::scanout_flags()]);
        assert_eq!(device.resets, 0);
        assert_eq!(device.steps.len(), 1);
        assert!(device.accepted.is_none());
        assert!(!schedule.pending);
        assert_eq!(device.drops.load(Ordering::Acquire), 0);
        assert!(!device.scene.as_ref().unwrap().capture_taken);
        if !device.reads.is_empty() {
            assert_retained_until_reads_finish(device, false);
        }
    }
}

#[test]
fn retry_failure_is_final_and_keeps_the_consumer_lease() {
    let mut device = DeviceOps::new([
        rejected(Reject::TestFailed),
        Step::Queue {
            selected: false,
            rejection: Some(Reject::Access(ErrorKind::InvalidInput)),
        },
        accepted(),
    ]);
    let mut schedule = Schedule::default();
    let error = submit(&mut schedule, &mut device).unwrap_err();
    assert!(error.contains("composition retry failed"));
    assert_eq!(device.attempts.len(), 2);
    assert_eq!(device.resets, 1);
    assert_eq!(
        device.steps.len(),
        1,
        "third attempt consumed the spare device response"
    );
    assert_eq!(device.rejected_sequences, [1, 2]);
    assert!(!schedule.pending);
    assert!(device.accepted.is_none());
    assert!(!device.scene.as_ref().unwrap().capture_taken);
    assert!(matches!(
        schedule
            .complete::<()>(Some(&metadata()), || panic!("rejected queue acknowledged"))
            .unwrap(),
        Completion::Ignored
    ));
    assert_retained_until_reads_finish(device, false);
}

#[test]
fn control_revocation_between_attempts_prevents_retry_and_preserves_barrier() {
    for revoke in [
        Revoke::Pause,
        Revoke::Security,
        Revoke::Configure,
        Revoke::Stop,
    ] {
        let mut device = DeviceOps::new([rejected(Reject::TestFailed), accepted()]);
        device.revoke_after_rejection = Some(revoke);
        let mut schedule = Schedule::default();
        schedule.invalidate_security();
        submit(&mut schedule, &mut device).unwrap();
        assert_eq!(device.attempts.len(), 1);
        assert_eq!(device.resets, 0);
        assert!(!schedule.pending);
        assert!(schedule.dirty);
        assert!(device.accepted.is_none());
        // Even another revoked pass must still try its security reset: the
        // rejected queue did not consume the forced-submission latch.
        submit(&mut schedule, &mut device).unwrap();
        assert_eq!(device.state_resets, 2);
        assert_eq!(device.attempts.len(), 1);
        assert_retained_until_reads_finish(device, false);
    }
}

#[test]
fn revocation_during_reset_or_before_retry_queue_never_accepts_stale_capture() {
    for before_queue in [false, true] {
        let mut device = DeviceOps::new([rejected(Reject::TestFailed), accepted()]);
        if before_queue {
            device.revoke_before_retry_queue = Some(Revoke::Security);
        } else {
            device.revoke_on_reset = Some(Revoke::Pause);
        }
        let mut schedule = Schedule::default();
        submit(&mut schedule, &mut device).unwrap();
        assert_eq!(device.resets, 1);
        assert_eq!(device.attempts.len(), if before_queue { 2 } else { 1 });
        assert_eq!(device.queued_sequences, [1]);
        assert!(device.accepted.is_none());
        assert!(!schedule.pending);
        assert!(!device.scene.as_ref().unwrap().capture_taken);
        assert_retained_until_reads_finish(device, false);
    }
}

#[test]
fn failed_retry_capture_does_not_replace_successful_scanout_or_drop_quarantine() {
    for failed_capture in [1, 2] {
        let mut device = DeviceOps::new([rejected(Reject::TestFailed), accepted()]);
        device.capture_error = Some(failed_capture);
        let mut schedule = Schedule::default();
        submit(&mut schedule, &mut device).unwrap();
        assert!(schedule.pending);
        assert_eq!(device.captures, 2);
        assert!(device.scene.as_ref().unwrap().uncertain);
        let Completion::Presented(frame) = schedule
            .complete(Some(&metadata()), || Ok(device.accepted.take()))
            .unwrap()
        else {
            panic!("capture failure suppressed successful scanout")
        };
        assert_eq!(frame.sequence, 2);
        if failed_capture == 2 {
            assert!(matches!(frame.capture, Some(Err(error)) if error == "retry readback failed"));
        } else {
            assert_eq!(frame.capture.unwrap().unwrap().pixels, [2; 4]);
        }
        assert_retained_until_reads_finish(device, true);
    }
}

#[test]
fn empty_retry_cannot_acknowledge_or_consume_the_security_submission() {
    let mut device = DeviceOps::new([rejected(Reject::TestFailed), Step::Empty]);
    let mut schedule = Schedule::default();
    schedule.invalidate_security();
    let error = submit(&mut schedule, &mut device).unwrap_err();
    assert!(error.contains("composition retry produced no submission"));
    assert_eq!(device.attempts.len(), 2);
    assert_eq!(device.resets, 1);
    assert_eq!(device.queued_sequences, [1]);
    assert!(device.accepted.is_none());
    assert!(!schedule.pending);
    assert!(schedule.dirty);
    assert!(schedule.opportunity_at.is_none());
    assert!(!device.scene.as_ref().unwrap().capture_taken);
    assert_retained_until_reads_finish(device, false);
}

#[test]
fn rejected_primary_without_a_cursor_retries_and_preserves_the_source_through_reads() {
    let mut device = DeviceOps::new([rejected(Reject::TestFailed), accepted()]);
    device.primary_scanout = true;
    let mut schedule = Schedule::default();
    submit(&mut schedule, &mut device).unwrap();
    assert_eq!(
        device.attempts,
        [super::scanout_flags(), FrameFlags::empty()]
    );
    let Completion::Presented(frame) = schedule
        .complete(Some(&metadata()), || Ok(device.accepted.take()))
        .unwrap()
    else {
        panic!("fallback was not presented")
    };
    assert!(!frame.diagnostics.cursor_plane);
    assert!(!frame.diagnostics.primary_scanout);
    assert_eq!(frame.capture.unwrap().unwrap().pixels, [2; 4]);
    assert_retained_until_reads_finish(device, false);
}
