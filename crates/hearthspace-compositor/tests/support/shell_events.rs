//! Action assertions share the socket with independent layout invalidations.
use serde_json::Value;
use std::{
    io,
    os::unix::net::UnixDatagram,
    time::{Duration, Instant},
};

pub(super) fn next_action(events: &UnixDatagram) -> io::Result<Value> {
    let timeout = events.read_timeout()?.unwrap_or(Duration::from_secs(2));
    let deadline = Instant::now() + timeout;
    let result = (|| {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "no shell action before deadline",
                ));
            }
            events.set_read_timeout(Some(remaining))?;
            let mut bytes = [0; 4096];
            let count = events.recv(&mut bytes)?;
            let event: Value =
                serde_json::from_slice(&bytes[..count]).expect("valid shell event JSON");
            assert_eq!(
                event["version"], 1,
                "unexpected shell event version: {event}"
            );
            if event["action"] != "layout-changed" {
                return Ok(event);
            }
        }
    })();
    events.set_read_timeout(Some(timeout))?;
    result
}

pub(super) fn receive(events: &UnixDatagram, expected: &str) -> Value {
    let event = next_action(events).expect("expected shell action before deadline");
    assert_eq!(
        event["action"], expected,
        "unexpected shell action: {event}"
    );
    event
}
