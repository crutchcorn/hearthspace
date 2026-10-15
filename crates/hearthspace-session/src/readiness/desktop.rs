//! Bounded status and fresh-presentation probes on the private control socket.
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::{connect_until, remaining};

const STATUS_LIMIT: usize = 256 * 1024;
const CAPTURE_LIMIT: usize = 128 * 1024 * 1024;

pub(crate) fn probe(
    path: &Path,
    native: bool,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<bool> {
    let status: Value =
        serde_json::from_slice(&request(path, "status", STATUS_LIMIT, deadline, cancelled)?)?;
    let Some(outputs) = ready_outputs(&status, native) else {
        return Ok(false);
    };
    for (output, previous_frame) in &outputs {
        let payload = request(
            path,
            &format!("capture {output}"),
            CAPTURE_LIMIT,
            deadline,
            cancelled,
        )?;
        validate_capture(&payload, output, *previous_frame, native)?;
    }
    // Hotplug, deactivation or lock preparation during capture invalidates this
    // attempt. The next attempt must cover the current connected output set.
    let current: Value =
        serde_json::from_slice(&request(path, "status", STATUS_LIMIT, deadline, cancelled)?)?;
    Ok(ready_outputs(&current, native).is_some_and(|current| {
        current
            .iter()
            .map(|(name, _)| name)
            .eq(outputs.iter().map(|(name, _)| name))
    }))
}

fn ready_outputs(status: &Value, native: bool) -> Option<Vec<(String, u64)>> {
    let backend_matches = if native {
        status["backend"] == "native"
    } else {
        matches!(status["backend"].as_str(), Some("nested" | "headless"))
    };
    if !backend_matches
        || status["active"] != true
        || status["shell_ready"] != true
        || status["locked"] != false
        || (native && status["recovery_ready"] != true)
    {
        return None;
    }
    let outputs = status["outputs"].as_array()?;
    if outputs.is_empty() || outputs.len() > 32 {
        return None;
    }
    outputs
        .iter()
        .map(|output| {
            let name = output["name"].as_str()?;
            if name.is_empty() || name.len() > 256 || name.chars().any(char::is_whitespace) {
                return None;
            }
            Some((name.to_owned(), output["frame_id"].as_u64()?))
        })
        .collect()
}

fn validate_capture(payload: &[u8], output: &str, previous: u64, native: bool) -> io::Result<()> {
    let newline = payload
        .iter()
        .position(|byte| *byte == b'\n')
        .filter(|index| *index <= 4096)
        .ok_or_else(|| io::Error::other("missing capture metadata"))?;
    let metadata: Value = serde_json::from_slice(&payload[..newline])?;
    let presentation = if native { "page-flip" } else { "offscreen" };
    if metadata["output"] != output
        || metadata["presentation"] != presentation
        || !metadata["frame_id"]
            .as_u64()
            .is_some_and(|frame| frame > previous)
        || !payload[newline + 1..].starts_with(b"\x89PNG\r\n\x1a\n")
    {
        return Err(io::Error::other(
            "desktop has no matching fresh presentation",
        ));
    }
    Ok(())
}

fn request(
    path: &Path,
    command: &str,
    limit: usize,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<Vec<u8>> {
    let mut stream = connect_until(path, deadline, cancelled)?;
    stream.set_write_timeout(Some(
        remaining(deadline, cancelled)?.min(Duration::from_millis(100)),
    ))?;
    stream.write_all(format!("{command}\n").as_bytes())?;
    let mut header = Vec::new();
    loop {
        let mut byte = [0];
        read_bounded(&mut stream, &mut byte, deadline, cancelled)?;
        if byte[0] == b'\n' {
            break;
        }
        if header.len() == 2048 {
            return Err(io::Error::other("oversized desktop readiness header"));
        }
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header).map_err(io::Error::other)?;
    let size = header
        .strip_prefix("ok ")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|size| *size > 0 && *size <= limit)
        .ok_or_else(|| io::Error::other(format!("invalid desktop readiness response: {header}")))?;
    let mut payload = vec![0; size];
    read_bounded(&mut stream, &mut payload, deadline, cancelled)?;
    Ok(payload)
}

fn read_bounded(
    stream: &mut UnixStream,
    mut buffer: &mut [u8],
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    while !buffer.is_empty() {
        stream.set_read_timeout(Some(
            remaining(deadline, cancelled)?.min(Duration::from_millis(100)),
        ))?;
        match stream.read(buffer) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => buffer = &mut buffer[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
