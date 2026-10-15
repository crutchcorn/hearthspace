use std::{
    cell::RefCell,
    io::{self, ErrorKind, Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

use smithay::reexports::calloop::EventLoop;

use super::{
    ControlReply,
    connection::{COMMAND_TIMEOUT, Connection},
    reply::{Limits, MAX_CONNECTIONS, MAX_REPLY_BYTES, Output},
    run_command_line,
    source::CommandSource,
};

fn pair() -> (UnixStream, UnixStream) {
    let (server, client) = UnixStream::pair().unwrap();
    server.set_nonblocking(true).unwrap();
    client.set_nonblocking(true).unwrap();
    (server, client)
}

fn read_available(mut stream: &UnixStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    bytes
}

#[test]
fn partial_header_and_payload_writes_resume_at_the_exact_offset() {
    struct ShortWriter {
        bytes: Vec<u8>,
        remaining: usize,
    }
    impl Write for ShortWriter {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(ErrorKind::WouldBlock.into());
            }
            let count = data.len().min(self.remaining).min(3);
            self.bytes.extend_from_slice(&data[..count]);
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let limits = Rc::new(Limits::default());
    let payload: Vec<_> = (0..4096).map(|n| (n % 251) as u8).collect();
    let mut expected = format!("ok {}\n", payload.len()).into_bytes();
    expected.extend_from_slice(&payload);
    let mut output = Output::new(ControlReply::Payload(payload), limits.clone());
    let mut writer = ShortWriter {
        bytes: Vec::new(),
        remaining: 1,
    };
    assert!(!output.flush(&mut writer).unwrap());
    assert_eq!(writer.bytes, b"o");
    while writer.bytes.len() < expected.len() {
        writer.remaining = 13;
        output.flush(&mut writer).unwrap();
    }
    assert_eq!(writer.bytes, expected);
    drop(output);
    assert_eq!(limits.output_bytes.get(), 0);
}

#[test]
fn oversized_replies_and_error_newlines_keep_the_wire_protocol_valid() {
    let limits = Rc::new(Limits::default());
    let mut output = Output::new(
        ControlReply::Payload(vec![0; MAX_REPLY_BYTES + 1]),
        limits.clone(),
    );
    let mut bytes = Vec::new();
    assert!(output.flush(&mut bytes).unwrap());
    assert_eq!(bytes, b"err reply exceeds output budget\n");
    let mut output = Output::new(ControlReply::Err("readback\nfailed\r\n".into()), limits);
    bytes.clear();
    assert!(output.flush(&mut bytes).unwrap());
    assert_eq!(bytes, b"err readback failed  \n");
}

#[test]
fn incomplete_commands_are_buffered_and_half_close_runs_trailing_command() {
    let (server, mut client) = pair();
    let now = Instant::now();
    let mut connection = Connection::new(Rc::default(), now);
    let mut commands = Vec::new();
    client.write_all(b"first\nsec").unwrap();
    connection
        .advance(&server, true, now, |line| {
            commands.push(line);
            ControlReply::Ok
        })
        .unwrap();
    assert_eq!(commands, vec![b"first\n".to_vec()]);
    client.write_all(b"ond").unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    assert!(
        !connection
            .advance(&server, true, now, |line| {
                commands.push(line);
                ControlReply::Ok
            })
            .unwrap()
    );
    assert_eq!(commands, vec![b"first\n".to_vec(), b"second".to_vec()]);
    assert_eq!(read_available(&client), b"ok\nok\n");
}

#[test]
fn deferred_reply_blocks_later_actions_and_preserves_reply_order() {
    let (server, mut client) = pair();
    let now = Instant::now();
    let mut connection = Connection::new(Rc::default(), now);
    let (sender, receiver) = mpsc::channel();
    client.write_all(b"capture\nclick\n").unwrap();
    let mut receiver = Some(receiver);
    let mut commands = Vec::new();
    connection
        .advance(&server, true, now, |line| {
            commands.push(line);
            ControlReply::Deferred(receiver.take().unwrap())
        })
        .unwrap();
    assert_eq!(commands.len(), 1);
    assert!(!connection.interest().writable);
    assert!(read_available(&client).is_empty());
    sender.send(ControlReply::Payload(b"png".to_vec())).unwrap();
    connection
        .advance(&server, false, now, |line| {
            commands.push(line);
            ControlReply::Ok
        })
        .unwrap();
    assert_eq!(commands, vec![b"capture\n".to_vec(), b"click\n".to_vec()]);
    assert_eq!(read_available(&client), b"ok 3\npngok\n");
}

#[test]
fn deferred_timeout_closes_without_running_later_actions() {
    let (server, mut client) = pair();
    let now = Instant::now();
    let mut connection = Connection::new(Rc::default(), now);
    let (_sender, receiver) = mpsc::channel();
    let mut receiver = Some(receiver);
    client.write_all(b"capture\nclick\n").unwrap();
    connection
        .advance(&server, true, now, |_| {
            ControlReply::Deferred(receiver.take().unwrap())
        })
        .unwrap();
    assert!(
        !connection
            .advance(&server, false, now + COMMAND_TIMEOUT, |_| panic!(
                "out-of-order action"
            ))
            .unwrap()
    );
    assert_eq!(read_available(&client), b"err command timed out\n");
}

#[test]
fn disconnected_capture_producer_returns_an_error() {
    let (server, mut client) = pair();
    let now = Instant::now();
    let mut connection = Connection::new(Rc::default(), now);
    let (sender, receiver) = mpsc::channel();
    drop(sender);
    let mut receiver = Some(receiver);
    client.write_all(b"capture\n").unwrap();
    connection
        .advance(&server, true, now, |_| {
            ControlReply::Deferred(receiver.take().unwrap())
        })
        .unwrap();
    connection
        .advance(&server, false, now, |_| panic!("no queued command"))
        .unwrap();
    assert_eq!(read_available(&client), b"err command cancelled\n");
}

#[test]
fn invalid_utf8_and_empty_lines_return_errors_instead_of_silence() {
    for bytes in [&b"\xff\n"[..], &b" \n"[..]] {
        assert!(matches!(
            run_command_line(bytes, |_| panic!("invalid command ran")),
            ControlReply::Err(_)
        ));
    }
    run_command_line(b"  valid \n", |text| {
        assert_eq!(text, "valid");
        ControlReply::Ok
    });
}

#[test]
fn unterminated_oversized_command_gets_a_bounded_error() {
    let (server, mut client) = pair();
    let now = Instant::now();
    let mut connection = Connection::new(Rc::default(), now);
    client.write_all(&vec![b'x'; 4097]).unwrap();
    assert!(
        !connection
            .advance(&server, true, now, |_| panic!("oversized command ran"))
            .unwrap()
    );
    assert_eq!(
        read_available(&client),
        b"err command exceeds input limit\n"
    );
}

#[test]
fn stalled_input_and_disconnected_readers_release_the_connection() {
    let (server, client) = pair();
    let now = Instant::now();
    let mut connection = Connection::new(Rc::default(), now);
    assert!(
        !connection
            .advance(&server, false, now + COMMAND_TIMEOUT, |_| unreachable!())
            .unwrap()
    );
    let mut output = Output::new(ControlReply::Payload(vec![1; 1024]), Rc::default());
    drop(client);
    assert!(output.flush(&server).is_err());
}

#[test]
fn queued_input_and_stalled_writes_have_absolute_bounds() {
    let (server, mut client) = pair();
    let now = Instant::now();
    let limits = Rc::new(Limits::default());
    let mut connection = Connection::new(limits.clone(), now);
    client.write_all(b"capture\n").unwrap();
    connection
        .advance(&server, true, now, |_| {
            ControlReply::Payload(vec![0; 2 * 1024 * 1024])
        })
        .unwrap();
    assert!(limits.output_bytes.get() > 0);
    assert!(
        !connection
            .advance(&server, false, now + COMMAND_TIMEOUT, |_| {
                panic!("write deadline must expire")
            })
            .unwrap()
    );
    drop(connection);
    assert_eq!(limits.output_bytes.get(), 0);

    let (server, mut client) = pair();
    let mut connection = Connection::new(Rc::default(), now);
    let (_sender, receiver) = mpsc::channel();
    let mut receiver = Some(receiver);
    client.write_all(b"capture\n").unwrap();
    connection
        .advance(&server, true, now, |_| {
            ControlReply::Deferred(receiver.take().unwrap())
        })
        .unwrap();
    client.write_all(&vec![b'\n'; 64 * 1024]).unwrap();
    connection
        .advance(&server, true, now, |_| {
            panic!("deferred action must block queue")
        })
        .unwrap();
    client.write_all(b"\n").unwrap();
    assert_eq!(
        connection
            .advance(&server, true, now, |_| unreachable!())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn aggregate_output_budget_rejects_another_payload() {
    let limits = Rc::new(Limits::default());
    limits.output_bytes.set(128 * 1024 * 1024 - 32);
    let mut output = Output::new(ControlReply::Payload(vec![0; 1024]), limits.clone());
    let mut bytes = Vec::new();
    assert!(output.flush(&mut bytes).unwrap());
    assert_eq!(bytes, b"err reply exceeds output budget\n");
    drop(output);
    assert_eq!(limits.output_bytes.get(), 128 * 1024 * 1024 - 32);
}

#[test]
fn connection_limit_is_reclaimed_when_sources_drop() {
    let limits = Rc::new(Limits::default());
    let mut sources = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        sources.push(CommandSource::new(pair().0, limits.clone()).unwrap());
    }
    assert!(CommandSource::new(pair().0, limits.clone()).is_none());
    sources.pop();
    assert!(CommandSource::new(pair().0, limits.clone()).is_some());
    drop(sources);
    assert_eq!(limits.connections.get(), 0);
}

#[test]
fn calloop_resumes_large_reply_after_reader_backpressure() {
    let (server, mut client) = pair();
    let mut event_loop = EventLoop::<()>::try_new().unwrap();
    let limits = Rc::new(Limits::default());
    let source = CommandSource::new(server, limits.clone()).unwrap();
    let payload = vec![0x5a; 2 * 1024 * 1024];
    let mut expected = format!("ok {}\n", payload.len()).into_bytes();
    expected.extend_from_slice(&payload);
    let mut payload = Some(payload);
    event_loop
        .handle()
        .insert_source(source, move |_, _, _| {
            ControlReply::Payload(payload.take().unwrap())
        })
        .unwrap();
    client.write_all(b"screenshot\n").unwrap();
    event_loop
        .dispatch(Duration::from_millis(50), &mut ())
        .unwrap();
    assert!(
        limits.output_bytes.get() > 0,
        "kernel buffer must exert backpressure"
    );
    let mut received = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while received.len() < expected.len() && Instant::now() < deadline {
        received.extend(read_available(&client));
        event_loop
            .dispatch(Duration::from_millis(2), &mut ())
            .unwrap();
    }
    received.extend(read_available(&client));
    assert_eq!(received, expected);
    assert_eq!(limits.output_bytes.get(), 0);
    drop(client);
    event_loop
        .dispatch(Duration::from_millis(50), &mut ())
        .unwrap();
    assert_eq!(limits.connections.get(), 0);
}

#[test]
fn calloop_timer_completes_a_deferred_reply_without_new_socket_input() {
    let (server, mut client) = pair();
    let mut event_loop = EventLoop::<()>::try_new().unwrap();
    let source = CommandSource::new(server, Rc::default()).unwrap();
    let (sender, receiver) = mpsc::channel();
    let receiver = RefCell::new(Some(receiver));
    event_loop
        .handle()
        .insert_source(source, move |_, _, _| {
            ControlReply::Deferred(receiver.borrow_mut().take().unwrap())
        })
        .unwrap();
    client.write_all(b"capture\n").unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    event_loop
        .dispatch(Duration::from_millis(50), &mut ())
        .unwrap();
    sender
        .send(ControlReply::Payload(b"presented".to_vec()))
        .unwrap();
    event_loop
        .dispatch(Duration::from_millis(50), &mut ())
        .unwrap();
    assert_eq!(read_available(&client), b"ok 9\npresented");
}
