use super::*;
use crate::compositor::capture::{CaptureState, OutputMetadata};

fn output() -> OutputMetadata {
    OutputMetadata {
        name: "DP-1".into(),
        width: 2,
        height: 2,
        scale: 1,
        logical_x: 0,
        logical_y: 0,
        logical_width: 2,
        logical_height: 2,
        transform: "normal",
        frame_id: 0,
    }
}

#[test]
fn another_control_client_responds_while_capture_encoder_is_blocked() {
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut captures = CaptureState::with_test_encoder(move |_, _| {
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
        Ok(b"png".to_vec())
    });
    let mut event_loop = EventLoop::<CaptureState>::try_new().unwrap();
    let limits = Rc::new(Limits::default());
    let (capture_server, mut capture_client) = pair();
    event_loop
        .handle()
        .insert_source(
            CommandSource::new(capture_server, limits.clone()).unwrap(),
            |_, _, captures| {
                let reply = captures.request("DP-1".into(), false);
                captures
                    .stage_pixels(output(), (2, 2).into(), vec![0; 16], "framebuffer")
                    .unwrap();
                captures.presented("DP-1", "page-flip");
                reply
            },
        )
        .unwrap();
    capture_client.write_all(b"capture\n").unwrap();
    event_loop
        .dispatch(Duration::from_millis(50), &mut captures)
        .unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let (status_server, mut status_client) = pair();
    event_loop
        .handle()
        .insert_source(
            CommandSource::new(status_server, limits).unwrap(),
            |_, _, _| ControlReply::Payload(b"live".to_vec()),
        )
        .unwrap();
    status_client.write_all(b"status\n").unwrap();
    let mut status = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(1);
    while status.len() < b"ok 4\nlive".len() && Instant::now() < deadline {
        event_loop
            .dispatch(Duration::from_millis(10), &mut captures)
            .unwrap();
        captures.maintain(&[output()], true);
        status.extend(read_available(&status_client));
    }
    assert_eq!(
        status, b"ok 4\nlive",
        "control must respond before releasing encoder"
    );
    assert!(read_available(&capture_client).is_empty());
    release.send(()).unwrap();
    let mut capture = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while capture.len() < b"ok 3\npng".len() && Instant::now() < deadline {
        event_loop
            .dispatch(Duration::from_millis(10), &mut captures)
            .unwrap();
        captures.maintain(&[output()], true);
        capture.extend(read_available(&capture_client));
    }
    assert_eq!(capture, b"ok 3\npng");
}
