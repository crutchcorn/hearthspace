use super::*;
use crate::compositor::frame_callbacks::test_support::Fixture;
use std::time::Duration;

const A: DeviceToken = DeviceToken {
    id: 226,
    incarnation: 1,
};
const B: DeviceToken = DeviceToken {
    id: 227,
    incarnation: 2,
};

fn outputs() -> [(OutputPresentation, usize); 2] {
    let cutoff = SurfaceEventCutoff::now();
    // Deliberately identical CRTC numbers on different GPUs.
    [
        (OutputPresentation::new(A, 1, "A".into(), cutoff), 0),
        (OutputPresentation::new(B, 1, "B".into(), cutoff), 0),
    ]
}

fn current_event() -> DrmEventMetadata {
    DrmEventMetadata {
        time: DrmEventTime::Monotonic(Clock::<Monotonic>::new().now().into()),
        sequence: 7,
    }
}

fn complete(
    outputs: &mut [(OutputPresentation, usize)],
    device: DeviceToken,
    metadata: &DrmEventMetadata,
) -> Submission {
    complete_frame(
        outputs.iter_mut().map(|(state, buffer)| (state, buffer)),
        device,
        1,
        Some(metadata),
        |buffer| {
            *buffer += 1;
            Ok(true)
        },
    )
    .unwrap()
}

fn dispatch(fixture: &mut Fixture, result: Submission, time: u32) -> Option<bool> {
    let Submission::Verified { completion, .. } = result else {
        panic!("verified flip discarded");
    };
    completion
        .dispatch(|output| {
            assert_eq!(fixture.send_output(output, time), 1);
            Ok::<_, ()>(())
        })
        .unwrap()
}

#[test]
fn alternating_gpu_completions_service_clients_while_redraw_waits_for_all_outputs() {
    let mut fixture = Fixture::new();
    let a = fixture.window("A");
    let b = fixture.window("B");
    let mut outputs = outputs();
    fixture.request_frame(a);
    fixture.request_frame(b);
    outputs[0].0.queued(None);
    outputs[1].0.queued(None);

    let result = complete(&mut outputs, A, &current_event());
    assert_eq!(dispatch(&mut fixture, result, 16), None);
    assert!(!outputs[0].0.pending);
    assert!(outputs[1].0.pending);
    fixture.assert_done(&[(a, 16)]);

    // A's callback permits a new commit/queue before B completes. B's done
    // must arrive without consuming A's new callback or opening the redraw gate.
    fixture.request_frame(a);
    outputs[0].0.queued(None);
    let result = complete(&mut outputs, B, &current_event());
    assert_eq!(dispatch(&mut fixture, result, 32), None);
    assert!(outputs[0].0.pending);
    assert!(!outputs[1].0.pending);
    fixture.assert_done(&[(b, 32)]);

    fixture.request_frame(b);
    outputs[1].0.queued(None);
    let result = complete(&mut outputs, A, &current_event());
    assert_eq!(dispatch(&mut fixture, result, 48), None);
    fixture.assert_done(&[(a, 48)]);

    fixture.request_frame(a);
    outputs[0].0.queued(None);
    outputs[0].0.dirty = true; // A redraw request arrived while this flip waited.
    let result = complete(&mut outputs, B, &current_event());
    assert_eq!(dispatch(&mut fixture, result, 64), None);
    fixture.assert_done(&[(b, 64)]);

    let result = complete(&mut outputs, A, &current_event());
    assert_eq!(dispatch(&mut fixture, result, 80), Some(true));
    fixture.assert_done(&[(a, 80)]);
    assert_eq!((outputs[0].1, outputs[1].1), (3, 2));
    assert!(
        outputs
            .iter()
            .all(|(state, _)| state.presented_once && !state.pending)
    );
}

#[test]
fn idle_clean_completion_sends_callback_without_requesting_redraw() {
    let mut fixture = Fixture::new();
    let a = fixture.window("A");
    fixture.request_frame(a);
    let mut outputs = outputs();
    outputs[0].0.queued(None);
    outputs[1].0.dirty = false;
    let result = complete(&mut outputs, A, &current_event());
    assert_eq!(dispatch(&mut fixture, result, 16), Some(false));
    fixture.assert_done(&[(a, 16)]);
}

#[test]
fn recycled_device_id_and_old_surface_event_cannot_acknowledge_new_buffer() {
    let mut outputs = outputs();
    outputs[0].0.queued(Some(12));
    let old = DeviceToken {
        incarnation: 0,
        ..A
    };
    assert!(matches!(
        complete(&mut outputs, old, &current_event()),
        Submission::Ignored
    ));
    let stale = DrmEventMetadata {
        time: DrmEventTime::Monotonic(Duration::ZERO),
        sequence: 7,
    };
    assert!(matches!(
        complete(&mut outputs, A, &stale),
        Submission::Ignored
    ));
    assert!(outputs[0].0.pending);
    assert_eq!(outputs[0].0.lock_generation, Some(12));
    assert_eq!(
        outputs[0].1, 0,
        "stale events must not reach GBM acknowledgement"
    );
}

#[test]
fn untimed_modeset_releases_queue_without_lock_or_client_completion() {
    let mut outputs = outputs();
    outputs[0].0.queued(Some(12));
    let untimed = DrmEventMetadata {
        time: DrmEventTime::Monotonic(Duration::ZERO),
        sequence: 0,
    };
    assert!(
        matches!(complete(&mut outputs, A, &untimed), Submission::Untimed(name) if name == "A")
    );
    assert!(!outputs[0].0.pending);
    assert!(outputs[0].0.dirty);
    assert!(!outputs[0].0.presented_once);
    assert_eq!(outputs[0].0.lock_generation, None);
    outputs[0].0.queued(Some(12));
    assert!(matches!(
        complete(&mut outputs, A, &current_event()),
        Submission::Verified {
            lock_generation: Some(12),
            ..
        }
    ));
}

#[test]
fn missing_gbm_submission_keeps_pending_frame_and_lock_marker() {
    let mut outputs = outputs();
    outputs[0].0.queued(Some(12));
    let result = complete_frame(
        outputs.iter_mut().map(|(state, buffer)| (state, buffer)),
        A,
        1,
        Some(&current_event()),
        |_| Ok(false),
    )
    .unwrap();
    assert!(matches!(result, Submission::Ignored));
    assert!(outputs[0].0.pending);
    assert_eq!(outputs[0].0.lock_generation, Some(12));
}
