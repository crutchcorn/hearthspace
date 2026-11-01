use super::*;
use crate::compositor::{
    CalloopData, capture::protocol::RemoteCaptureState, client::ClientState,
    shell_integration::ControlReply,
};
use smithay::{
    output::{PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
};
use std::{
    os::unix::net::UnixStream,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::Event;
use wayland_server::Display;

use crate::compositor::{
    capture::protocol::test_client as client, frame_callbacks::test_support::app::new_app,
};

fn descriptor(name: &str) -> OutputDescriptor {
    OutputDescriptor {
        name: name.into(),
        properties: PhysicalProperties {
            size: (1, 1).into(),
            subpixel: Subpixel::Unknown,
            make: "test".into(),
            model: name.into(),
            serial_number: name.into(),
        },
        size: (2, 2).into(),
        scale: 1,
        refresh: 60_000,
    }
}

type Action =
    Box<dyn FnOnce(&mut App, &mut CaptureState, &mut PublishedOutputs, &DisplayHandle) + Send>;

struct Server {
    sender: Option<mpsc::Sender<Action>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn new() -> (Self, UnixStream) {
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let (sender, receiver) = mpsc::channel::<Action>();
        let thread = thread::spawn(move || {
            let mut display = Display::<App>::new().unwrap();
            let mut dh = display.handle();
            let event_loop = EventLoop::<CalloopData>::try_new().unwrap();
            let mut app = new_app(&dh, event_loop.handle());
            app.remote_capture = RemoteCaptureState::new(&dh, true);
            app.set_remote_input_active(true);
            let mut captures = CaptureState::default();
            let mut published = PublishedOutputs::default();
            published
                .update(vec![descriptor("A")])
                .apply(&mut app, &mut captures, &dh);
            dh.insert_client(server_socket, Arc::new(ClientState::default()))
                .unwrap();
            loop {
                display.dispatch_clients(&mut app).unwrap();
                match receiver.recv_timeout(Duration::from_millis(1)) {
                    Ok(action) => action(&mut app, &mut captures, &mut published, &dh),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                display.flush_clients().unwrap();
            }
        });
        (
            Self {
                sender: Some(sender),
                thread: Some(thread),
            },
            client_socket,
        )
    }

    fn run(
        &self,
        action: impl FnOnce(&mut App, &mut CaptureState, &mut PublishedOutputs, &DisplayHandle)
        + Send
        + 'static,
    ) {
        let (sender, receiver) = mpsc::sync_channel(0);
        self.sender
            .as_ref()
            .unwrap()
            .send(Box::new(move |app, captures, published, dh| {
                action(app, captures, published, dh);
                sender.send(()).unwrap();
            }))
            .unwrap();
        receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.sender.take();
        self.thread.take().unwrap().join().unwrap();
    }
}

fn stage(app: &mut App, captures: &mut CaptureState, name: &str) -> mpsc::Receiver<ControlReply> {
    let ControlReply::Deferred(receiver) = captures.request(name.into(), false) else {
        panic!("capture rejected")
    };
    let output = app
        .capture_outputs()
        .into_iter()
        .find(|output| output.name == name)
        .unwrap();
    captures.stage(output, vec![1, 2, 3], "framebuffer");
    app.remote_capture
        .stage(name, (2, 2).into(), vec![255; 16], None);
    receiver
}

fn assert_presented(
    app: &mut App,
    captures: &mut CaptureState,
    receiver: mpsc::Receiver<ControlReply>,
) {
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    captures.presented("A", "page-flip");
    app.remote_capture.presented("A", Duration::from_secs(1));
    assert!(
        matches!(receiver.try_recv().unwrap(), ControlReply::Payload(bytes) if bytes == [1, 2, 3])
    );
}

fn assert_remote_ready(client: &mut client::Client) {
    client.sync();
    assert_eq!(
        client.events.stopped, 0,
        "healthy output session was stopped"
    );
    assert!(
        client
            .events
            .frames
            .iter()
            .any(|event| matches!(event, Event::Ready))
    );
    assert!(
        !client
            .events
            .frames
            .iter()
            .any(|event| matches!(event, Event::Failed { .. }))
    );
}

#[test]
fn repeated_failed_secondary_recovery_preserves_healthy_pending_captures() {
    let (server, socket) = Server::new();
    let mut client = client::Client::new(socket);
    let session = client.session(false);
    let (_frame, buffer) = client.capture(&session, 2);
    server.run(|app, captures, published, dh| {
        let receiver = stage(app, captures, "A");
        app.needs_redraw = false;
        for _ in 0..3 {
            app.set_native_lock_outputs_available(false);
            assert!(
                !published
                    .update(vec![descriptor("A")])
                    .apply(app, captures, dh)
            );
        }
        assert!(
            !app.needs_redraw,
            "unchanged inventory caused reconfiguration"
        );
        assert_presented(app, captures, receiver);
    });
    assert_remote_ready(&mut client);
    assert_eq!(&buffer.bytes()[4..12], &[255; 8]);
}

#[test]
fn same_gpu_incomplete_discovery_retries_preserve_healthy_pending_captures() {
    let (server, socket) = Server::new();
    let mut client = client::Client::new(socket);
    let session = client.session(false);
    let (_frame, buffer) = client.capture(&session, 2);
    server.run(|app, captures, published, dh| {
        let receiver = stage(app, captures, "A");
        app.needs_redraw = false;
        // A is this GPU's healthy eDP. Its newly connected DP has no usable
        // mode/CRTC, so every scan returns the same usable target inventory.
        let usable = ["A"];
        let mut scanout_repair = false;
        let mut rebuilds = 0;
        for _ in 0..3 {
            app.set_native_lock_outputs_available(false);
            let refreshed = super::super::connector_policy::refresh_targets(
                &usable,
                &usable,
                true,
                scanout_repair,
                || {
                    // Substitute only the external KMS rebuild. Use the same
                    // source invalidation and real capture dispatch as hotplug.
                    rebuilds += 1;
                    published.invalidate(usable);
                    true
                },
            );
            scanout_repair = refreshed.recovery_needed;
            assert!(
                !published
                    .update(vec![descriptor("A")])
                    .apply(app, captures, dh),
                "discovery retry replaced healthy scanout"
            );
        }
        assert_eq!(rebuilds, 0);
        assert!(!scanout_repair);
        assert!(!app.needs_redraw);
        assert_presented(app, captures, receiver);
    });
    assert_remote_ready(&mut client);
    assert_eq!(&buffer.bytes()[4..12], &[255; 8]);
}

#[test]
fn secondary_add_rebuild_and_removal_only_invalidate_affected_captures() {
    let (server, socket) = Server::new();
    let mut client = client::Client::new(socket);
    let session = client.session(false);
    let (_frame, _buffer) = client.capture(&session, 2);
    server.run(|app, captures, published, dh| {
        let healthy = stage(app, captures, "A");
        assert!(
            published
                .update(vec![descriptor("A"), descriptor("B")])
                .apply(app, captures, dh)
        );
        let replaced = stage(app, captures, "B");
        published.invalidate(["B"]);
        let sync = published.update(vec![descriptor("A"), descriptor("B")]);
        assert!(
            sync.descriptors.is_none(),
            "source rebuild did not change topology"
        );
        assert!(sync.apply(app, captures, dh));
        assert!(matches!(replaced.try_recv().unwrap(), ControlReply::Err(_)));
        assert!(
            published
                .update(vec![descriptor("A")])
                .apply(app, captures, dh)
        );
        assert!(
            !published
                .update(vec![descriptor("A")])
                .apply(app, captures, dh)
        );
        assert_presented(app, captures, healthy);
    });
    assert_remote_ready(&mut client);
}

#[test]
fn same_name_source_rebuild_discards_staged_pixels_before_late_completion() {
    let (server, socket) = Server::new();
    let mut client = client::Client::new(socket);
    let session = client.session(false);
    let (_frame, buffer) = client.capture(&session, 2);
    server.run(|app, captures, published, dh| {
        let receiver = stage(app, captures, "A");
        published.invalidate(["A"]);
        assert!(
            published
                .update(vec![descriptor("A")])
                .apply(app, captures, dh)
        );
        captures.presented("A", "late-page-flip");
        app.remote_capture.presented("A", Duration::from_secs(1));
        assert!(matches!(receiver.try_recv().unwrap(), ControlReply::Err(_)));
    });
    client.sync();
    assert_eq!(
        client.events.stopped, 0,
        "same connector may capture subsequent frames"
    );
    assert!(
        client
            .events
            .frames
            .iter()
            .any(|event| matches!(event, Event::Failed { .. }))
    );
    assert!(
        !client
            .events
            .frames
            .iter()
            .any(|event| matches!(event, Event::Ready))
    );
    assert_eq!(buffer.bytes(), [0x7e; 40]);
}
