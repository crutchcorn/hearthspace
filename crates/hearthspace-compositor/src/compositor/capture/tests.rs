use super::*;

fn output(name: &str) -> OutputMetadata {
    OutputMetadata {
        name: name.into(),
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

fn request(state: &mut CaptureState, name: &str, metadata: bool) -> mpsc::Receiver<ControlReply> {
    let ControlReply::Deferred(receiver) = state.request(name.into(), metadata) else {
        panic!("request rejected")
    };
    receiver
}

fn payload(receiver: &mpsc::Receiver<ControlReply>) -> Vec<u8> {
    match receiver.try_recv().unwrap() {
        ControlReply::Payload(p) => p,
        _ => panic!("expected payload"),
    }
}

#[test]
fn native_reply_waits_for_its_output_page_flip() {
    let mut state = CaptureState::default();
    let receiver = request(&mut state, "DP-1", true);
    state.stage(output("DP-1"), vec![1, 2, 3], "hardware-overlay");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    state.presented("HDMI-1", "page-flip");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    state.presented("DP-1", "page-flip");
    let bytes = payload(&receiver);
    let newline = bytes.iter().position(|b| *b == b'\n').unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&bytes[..newline]).unwrap();
    assert_eq!(metadata["frame_id"], 1);
    assert_eq!(metadata["presentation"], "page-flip");
    assert_eq!(metadata["logical_x"], 20);
    assert_eq!(metadata["scale"], 2);
    assert_eq!(metadata["cursor_source"], "hardware-overlay");
    assert_eq!(&bytes[newline + 1..], &[1, 2, 3]);
}

#[test]
fn requests_arriving_after_submission_wait_for_next_frame() {
    let mut state = CaptureState::default();
    let first = request(&mut state, "DP-1", false);
    state.stage(output("DP-1"), vec![1], "framebuffer");
    let second = request(&mut state, "DP-1", false);
    state.presented("DP-1", "page-flip");
    assert_eq!(payload(&first), vec![1]);
    assert!(matches!(second.try_recv(), Err(mpsc::TryRecvError::Empty)));
    state.stage(output("DP-1"), vec![2], "framebuffer");
    state.presented("DP-1", "page-flip");
    assert_eq!(payload(&second), vec![2]);
    assert_eq!(state.frame_id("DP-1"), 2);
}

#[test]
fn deadlines_and_disappearing_outputs_fail_pending_and_staged_requests() {
    let mut state = CaptureState::default();
    let expired = request(&mut state, "DP-1", false);
    state.pending[0].deadline = Instant::now();
    state.stage(output("DP-1"), vec![1], "framebuffer");
    state.maintain(&[output("DP-1")], true);
    assert!(matches!(expired.try_recv().unwrap(), ControlReply::Err(_)));
    let removed = request(&mut state, "DP-1", false);
    state.maintain(&[], true);
    assert!(matches!(removed.try_recv().unwrap(), ControlReply::Err(_)));
    let inactive = request(&mut state, "DP-1", false);
    state.maintain(&[output("DP-1")], false);
    assert!(matches!(inactive.try_recv().unwrap(), ControlReply::Err(_)));
    assert!(state.timeout().is_none());
}

#[test]
fn bounds_requests_and_tolerates_disconnected_clients() {
    let mut state = CaptureState::default();
    for _ in 0..MAX_REQUESTS {
        drop(request(&mut state, "DP-1", true));
    }
    assert!(matches!(
        state.request("DP-1".into(), true),
        ControlReply::Err(_)
    ));
    state.stage(output("DP-1"), vec![1], "framebuffer");
    state.presented("DP-1", "page-flip");
    assert!(state.timeout().is_none());
    drop(request(&mut state, "DP-1", true));
}
