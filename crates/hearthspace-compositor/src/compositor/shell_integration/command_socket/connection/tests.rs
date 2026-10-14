use std::{
    io::{Read, Write},
    sync::mpsc,
};

use super::*;

#[test]
fn dispatch_delay_preserves_the_completed_producer_result() {
    for (reply, expected) in [
        (
            ControlReply::Payload(b"presented".to_vec()),
            b"ok 9\npresented".as_slice(),
        ),
        (
            ControlReply::Err("capture presentation timed out".into()),
            b"err capture presentation timed out\n".as_slice(),
        ),
    ] {
        let (server, mut client) = UnixStream::pair().unwrap();
        server.set_nonblocking(true).unwrap();
        client.set_nonblocking(true).unwrap();
        let now = Instant::now();
        let mut connection = Connection::new(Rc::default(), now);
        let (sender, receiver) = mpsc::channel();
        let mut receiver = Some(receiver);
        client.write_all(b"capture\n").unwrap();
        connection
            .advance(&server, true, now, |_| {
                ControlReply::Deferred(receiver.take().unwrap())
            })
            .unwrap();
        sender.send(reply).unwrap();

        // No wall-clock sleep: emulate the event loop not dispatching this
        // connection again until after its transport deadline.
        assert!(
            connection
                .advance(&server, false, now + COMMAND_TIMEOUT, |_| {
                    panic!("there is no second command")
                })
                .unwrap()
        );
        let mut received = Vec::new();
        let _ = client.read_to_end(&mut received);
        assert_eq!(received, expected);
    }
}
