use super::*;
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::Event;

use super::test_client as client;
mod server;

use client::Client;
use server::Server;

const RED_BLUE: &[u8] = &[
    255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255,
];

fn stage(server: &Server) {
    server.run(|state| {
        state.remote.stage(
            "test-output",
            (2, 2).into(),
            RED_BLUE.to_vec(),
            Some(vec![255; 16]),
        )
    });
}

fn presented(server: &Server) {
    server.run(|state| {
        state
            .remote
            .presented("test-output", Duration::new(123, 456))
    });
}

fn ready(client: &Client) -> usize {
    client
        .events
        .frames
        .iter()
        .filter(|event| matches!(event, Event::Ready))
        .count()
}

#[test]
fn advertises_only_supported_shm_formats_and_bounded_dimensions() {
    let formats = constraints((2, 3).into()).unwrap();
    assert_eq!(
        formats.shm,
        [wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888]
    );
    for size in [(0, 1), (4097, 4096), (i32::MAX, i32::MAX)] {
        assert!(constraints(size.into()).is_none());
    }
}

#[test]
fn globals_are_hidden_without_explicit_opt_in() {
    let (_server, socket) = Server::new(false);
    let client = Client::new(socket);
    assert!(client.events.manager.is_none());
}

#[test]
fn shm_waits_for_matching_page_flip_and_preserves_stride_padding_and_orientation() {
    let (server, socket) = Server::new(true);
    let mut client = Client::new(socket);
    let session = client.session(false);
    let (_frame, buffer) = client.capture(&session, 2);
    stage(&server);
    server.run(|state| state.remote.presented("other-output", Duration::ZERO));
    client.sync();
    assert_eq!(ready(&client), 0);
    assert_eq!(buffer.bytes(), [0x7e; 40]);
    presented(&server);
    client.sync();
    assert_eq!(ready(&client), 1);
    let bytes = buffer.bytes();
    assert_eq!(&bytes[4..8], &0xffff0000_u32.to_ne_bytes());
    assert_eq!(&bytes[16..20], &0xff0000ff_u32.to_ne_bytes());
    assert_eq!(&bytes[0..4], &[0x7e; 4]);
    assert_eq!(&bytes[12..16], &[0x7e; 4]);
    assert_eq!(&bytes[24..], &[0x7e; 16]);
    assert!(client.events.frames.iter().any(|event| matches!(
        event,
        Event::Damage {
            x: 0,
            y: 0,
            width: 2,
            height: 2
        }
    )));
    assert!(client.events.frames.iter().any(|event| matches!(
        event,
        Event::PresentationTime {
            tv_sec_hi: 0,
            tv_sec_lo: 123,
            tv_nsec: 456
        }
    )));
}

#[test]
fn paint_cursors_selects_only_the_cursor_embedded_pixels() {
    let (server, socket) = Server::new(true);
    let mut client = Client::new(socket);
    let session = client.session(true);
    let (_frame, buffer) = client.capture(&session, 2);
    stage(&server);
    presented(&server);
    client.sync();
    assert_eq!(&buffer.bytes()[4..12], &[255; 8]);
    assert_eq!(ready(&client), 1);
}

#[test]
fn lock_or_inactive_stop_discards_staged_pixels_before_late_page_flip() {
    let (server, socket) = Server::new(true);
    let mut client = Client::new(socket);
    let session = client.session(true);
    let (_frame, buffer) = client.capture(&session, 2);
    stage(&server);
    server.run(|state| state.remote.stop());
    presented(&server);
    client.sync();
    assert_eq!(ready(&client), 0);
    assert_eq!(client.events.stopped, 1);
    assert_eq!(buffer.bytes(), [0x7e; 40]);
    assert!(
        client
            .events
            .frames
            .iter()
            .any(|event| matches!(event, Event::Failed { .. }))
    );
}

#[test]
fn requests_after_submission_wait_for_a_later_frame() {
    let (server, socket) = Server::new(true);
    let mut client = Client::new(socket);
    let first = client.session(false);
    let (_first_frame, first_buffer) = client.capture(&first, 2);
    stage(&server);
    let second = client.session(false);
    let (_second_frame, second_buffer) = client.capture(&second, 2);
    presented(&server);
    client.sync();
    assert_eq!(ready(&client), 1);
    assert_ne!(first_buffer.bytes(), [0x7e; 40]);
    assert_eq!(second_buffer.bytes(), [0x7e; 40]);
    stage(&server);
    presented(&server);
    client.sync();
    assert_eq!(ready(&client), 2);
}

#[test]
fn wrong_size_and_destroyed_buffers_never_receive_pixels() {
    let (server, socket) = Server::new(true);
    let mut client = Client::new(socket);
    let session = client.session(false);
    let (frame, buffer) = client.capture(&session, 1);
    assert!(
        client
            .events
            .frames
            .iter()
            .any(|event| matches!(event, Event::Failed { .. }))
    );
    assert_eq!(buffer.bytes(), [0x7e; 40]);
    frame.destroy();
    let (_frame, buffer) = client.capture(&session, 2);
    stage(&server);
    buffer.object.destroy();
    client.sync();
    presented(&server);
    client.sync();
    assert_eq!(ready(&client), 0);
    assert_eq!(buffer.bytes(), [0x7e; 40]);
}

#[test]
fn expired_frame_cannot_write_client_memory() {
    let (server, socket) = Server::new(true);
    let mut client = Client::new(socket);
    let session = client.session(false);
    let (_frame, buffer) = client.capture(&session, 2);
    stage(&server);
    server.run(|state| state.remote.requests[0].deadline = Instant::now());
    presented(&server);
    client.sync();
    assert_eq!(ready(&client), 0);
    assert_eq!(buffer.bytes(), [0x7e; 40]);
}
