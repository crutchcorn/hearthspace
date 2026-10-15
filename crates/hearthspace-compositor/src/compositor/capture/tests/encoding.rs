use super::*;

fn blocked_encoder() -> (CaptureState, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let state = CaptureState::with_test_encoder(move |_, pixels| {
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
        Ok(pixels.to_vec())
    });
    (state, started, release)
}

fn wait_for_reply(
    state: &mut CaptureState,
    receiver: &mpsc::Receiver<ControlReply>,
) -> ControlReply {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        state.maintain(&[output("DP-1"), output("HDMI-1")], true);
        if let Ok(reply) = receiver.try_recv() {
            return reply;
        }
        assert!(Instant::now() < deadline, "encoder did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn slow_encoding_retains_original_page_flip_and_defers_later_capture() {
    let (mut state, started, release) = blocked_encoder();
    let first = request(&mut state, "DP-1", true);
    state
        .stage_pixels(output("DP-1"), (2, 2).into(), vec![1; 16], "framebuffer")
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(state.timeout().unwrap() <= Duration::from_millis(10));
    state.presented("DP-1", "page-flip");
    state.presented("DP-1", "page-flip");
    let second = request(&mut state, "DP-1", false);
    assert!(
        !state.wanted("DP-1"),
        "must not overwrite pixels awaiting encoding"
    );
    assert!(matches!(first.try_recv(), Err(mpsc::TryRecvError::Empty)));
    release.send(()).unwrap();
    let ControlReply::Payload(bytes) = wait_for_reply(&mut state, &first) else {
        panic!("expected encoded pixels");
    };
    let newline = bytes.iter().position(|byte| *byte == b'\n').unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&bytes[..newline]).unwrap();
    assert_eq!(metadata["frame_id"], 1);
    assert_eq!(metadata["presentation"], "page-flip");
    assert_eq!(&bytes[newline + 1..], &[1; 16]);
    assert!(state.wanted("DP-1"));
    assert!(matches!(second.try_recv(), Err(mpsc::TryRecvError::Empty)));
}

#[test]
fn completed_encoding_still_waits_for_its_output_presentation() {
    let mut state = CaptureState::with_test_encoder(|_, pixels| Ok(pixels.to_vec()));
    let receiver = request(&mut state, "DP-1", false);
    state
        .stage_pixels(output("DP-1"), (2, 2).into(), vec![9; 16], "framebuffer")
        .unwrap();
    // Poll until the worker's reservation is held by the completed frame.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !state.staged["DP-1"].encoding_complete() {
        state.maintain(&[output("DP-1")], true);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    state.presented("HDMI-1", "page-flip");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    state.presented("DP-1", "page-flip");
    assert_eq!(payload(&receiver), vec![9; 16]);
}

#[test]
fn lock_discards_inflight_encoding_without_reusing_its_pixels() {
    let (mut state, started, release) = blocked_encoder();
    let receiver = request(&mut state, "DP-1", false);
    state
        .stage_pixels(output("DP-1"), (2, 2).into(), vec![1; 16], "framebuffer")
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    state.invalidate("session is locked");
    assert!(
        matches!(receiver.try_recv().unwrap(), ControlReply::Err(error) if error == "session is locked")
    );
    state.presented("DP-1", "page-flip");
    release.send(()).unwrap();
    let next = request(&mut state, "DP-1", false);
    state
        .stage_pixels(output("DP-1"), (2, 2).into(), vec![2; 16], "framebuffer")
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    state.presented("DP-1", "page-flip");
    assert!(
        matches!(wait_for_reply(&mut state, &next), ControlReply::Payload(bytes) if bytes == vec![2;16])
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
}

#[test]
fn deadline_expires_while_worker_is_blocked() {
    let (mut state, started, release) = blocked_encoder();
    let receiver = request(&mut state, "DP-1", false);
    state
        .stage_pixels(output("DP-1"), (2, 2).into(), vec![1; 16], "framebuffer")
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    state.staged.get_mut("DP-1").unwrap().requests[0].deadline = Instant::now();
    state.maintain(&[output("DP-1")], true);
    assert!(
        matches!(receiver.try_recv().unwrap(), ControlReply::Err(error) if error.contains("timed out"))
    );
    assert!(state.timeout().is_none());
    release.send(()).unwrap();
    state.presented("DP-1", "page-flip");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
}

#[test]
fn completed_pngs_keep_worker_capacity_reserved_until_consumed() {
    let encoder =
        super::super::encoding::Encoder::with_encode(|_, pixels| Ok(pixels.to_vec())).unwrap();
    let first = encoder.submit((2, 2).into(), vec![0; 16]).unwrap();
    let second = encoder.submit((2, 2).into(), vec![0; 16]).unwrap();
    assert!(!encoder.available());
    assert!(encoder.submit((2, 2).into(), vec![0; 16]).is_err());
    let first_png = first.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
    let second_png = second
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert!(!encoder.available(), "finished buffers must remain bounded");
    drop(first_png);
    assert!(encoder.available());
    assert!(encoder.submit((4097, 4096).into(), vec![]).is_err());
    assert!(encoder.submit((2, 2).into(), vec![0; 12]).is_err());
    drop(second_png);
}

#[test]
fn shutdown_does_not_join_a_blocked_encoder_or_deliver_its_pixels() {
    struct Finished(mpsc::Sender<()>);
    impl Drop for Finished {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let (finished, exited) = mpsc::channel();
    let finished = Finished(finished);
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut state = CaptureState::with_test_encoder(move |_, pixels| {
        let _keep_until_worker_exit = &finished;
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
        Ok(pixels.to_vec())
    });
    let reply = request(&mut state, "DP-1", false);
    state
        .stage_pixels(output("DP-1"), (2, 2).into(), vec![1; 16], "framebuffer")
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let before = Instant::now();
    drop(state);
    assert!(before.elapsed() < Duration::from_millis(100));
    assert!(matches!(
        reply.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    release.send(()).unwrap();
    exited.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(
        reply.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
}

#[test]
fn encoding_error_releases_reservation_and_returns_request_error() {
    let mut state = CaptureState::with_test_encoder(|_, _| Err("encoder failed".into()));
    let reply = request(&mut state, "DP-1", false);
    state
        .stage_pixels(output("DP-1"), (2, 2).into(), vec![1; 16], "framebuffer")
        .unwrap();
    assert!(
        matches!(wait_for_reply(&mut state, &reply), ControlReply::Err(error) if error == "encoder failed")
    );
    assert!(state.encoder.as_ref().unwrap().available());
    assert!(state.staged.is_empty());
    assert!(state.timeout().is_none());
}
