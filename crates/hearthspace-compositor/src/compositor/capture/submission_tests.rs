//! Capture request ownership across delayed scene readback and presentation.
//! Only PNG encoding is substituted; requests, staging and replies are real.
use super::{CAPTURE_TIMEOUT, CaptureState, ControlReply, OutputMetadata};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn output() -> OutputMetadata {
    OutputMetadata {
        name: "DP-1".into(),
        width: 2,
        height: 2,
        scale: 2,
        logical_x: 20,
        logical_y: 10,
        logical_width: 1,
        logical_height: 1,
        transform: "normal",
        frame_id: 0,
    }
}

fn request_at(
    state: &mut CaptureState,
    accepted_at: Instant,
    metadata: bool,
) -> mpsc::Receiver<ControlReply> {
    let ControlReply::Deferred(receiver) = state.request("DP-1".into(), metadata) else {
        panic!("capture request rejected");
    };
    // Explicit acceptance times make the cutoff boundary independent of sleeps
    // and scheduler resolution, while retaining the real request/reply channel.
    state.pending.last_mut().unwrap().deadline = accepted_at + CAPTURE_TIMEOUT;
    receiver
}

fn assert_pending(receiver: &mpsc::Receiver<ControlReply>) {
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
}

fn wait_for_payload(state: &mut CaptureState, receiver: &mpsc::Receiver<ControlReply>) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        state.finish_frames();
        match receiver.try_recv() {
            Ok(ControlReply::Payload(bytes)) => return bytes,
            Err(mpsc::TryRecvError::Empty) => {}
            _ => panic!("expected capture payload"),
        }
        assert!(
            Instant::now() < deadline,
            "capture encoder did not complete"
        );
        std::thread::yield_now();
    }
}

fn wait_for_encoding(state: &mut CaptureState) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        state.finish_frames();
        if state.staged["DP-1"].encoding_complete() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "capture encoder did not complete"
        );
        std::thread::yield_now();
    }
}

fn assert_presented_payload(bytes: &[u8], frame_id: u64, pixels: &[u8]) {
    let newline = bytes.iter().position(|byte| *byte == b'\n').unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&bytes[..newline]).unwrap();
    assert_eq!(metadata["output"], "DP-1");
    assert_eq!(metadata["frame_id"], frame_id);
    assert_eq!(metadata["presentation"], "page-flip");
    assert_eq!(metadata["cursor_source"], "submitted-planes");
    assert_eq!(metadata["scale"], 2);
    assert_eq!(metadata["logical_x"], 20);
    assert_eq!(&bytes[newline + 1..], pixels);
}

#[test]
fn delayed_scene_pixels_complete_only_requests_accepted_by_its_cutoff() {
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut first_encoding = true;
    let mut state = CaptureState::with_test_encoder(move |_, pixels| {
        if std::mem::take(&mut first_encoding) {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        Ok(pixels.to_vec())
    });
    let cutoff = Instant::now();
    let older = request_at(&mut state, cutoff - Duration::from_millis(1), true);
    let at_cutoff = request_at(&mut state, cutoff, false);
    let newer_at = cutoff + Duration::from_nanos(1);
    let newer = request_at(&mut state, newer_at, true);

    // All three requests have arrived before the worker returns the old pixels.
    state
        .stage_pixels_before(
            output(),
            (2, 2).into(),
            vec![17; 16],
            "submitted-planes",
            cutoff,
        )
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    state.presented("DP-1", "page-flip");
    assert_pending(&older);
    assert_pending(&at_cutoff);
    assert_pending(&newer);
    // A cursor-only flip while encoding must not replace these pixels' frame id.
    state.presented("DP-1", "page-flip");
    release.send(()).unwrap();
    assert_presented_payload(&wait_for_payload(&mut state, &older), 1, &[17; 16]);
    assert_eq!(wait_for_payload(&mut state, &at_cutoff), vec![17; 16]);
    assert_pending(&newer);
    assert!(state.wanted("DP-1"));

    state
        .stage_pixels_before(
            output(),
            (2, 2).into(),
            vec![29; 16],
            "submitted-planes",
            newer_at,
        )
        .unwrap();
    wait_for_encoding(&mut state);
    assert_pending(&newer);
    state.presented("HDMI-1", "page-flip");
    assert_pending(&newer);
    state.presented("DP-1", "page-flip");
    assert_presented_payload(&wait_for_payload(&mut state, &newer), 3, &[29; 16]);
    assert!(matches!(
        older.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    assert!(!state.wanted("DP-1"));
    assert!(state.timeout().is_none());
}

#[test]
fn older_scene_with_no_eligible_request_cannot_resolve_a_new_capture() {
    let mut state = CaptureState::with_test_encoder(|_, pixels| Ok(pixels.to_vec()));
    let cutoff = Instant::now();
    let newer_at = cutoff + Duration::from_nanos(1);
    let newer = request_at(&mut state, newer_at, true);

    state
        .stage_pixels_before(
            output(),
            (2, 2).into(),
            vec![41; 16],
            "submitted-planes",
            cutoff,
        )
        .unwrap();
    wait_for_encoding(&mut state);
    assert_pending(&newer);
    state.presented("DP-1", "page-flip");
    assert_pending(&newer);
    assert!(state.wanted("DP-1"));

    state
        .stage_pixels_before(
            output(),
            (2, 2).into(),
            vec![53; 16],
            "submitted-planes",
            newer_at,
        )
        .unwrap();
    wait_for_encoding(&mut state);
    assert_pending(&newer);
    state.presented("DP-1", "page-flip");
    assert_presented_payload(&wait_for_payload(&mut state, &newer), 2, &[53; 16]);
    assert!(matches!(
        newer.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    assert!(state.timeout().is_none());
}
