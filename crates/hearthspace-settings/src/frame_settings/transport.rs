use std::{
    io::{self, Read},
    path::Path,
    time::{Duration, Instant},
};

use super::{Operation, Response, Snapshot};

const TIMEOUT: Duration = Duration::from_millis(750);
const MAX_STATUS_BYTES: usize = hearthspace_ipc::transport::MAX_RESPONSE_BYTES;

pub(super) fn execute(path: &Path, operation: Operation) -> Response {
    let action_error = operation.command().and_then(|command| {
        match hearthspace_ipc::transport::request_line(path, &format!("{command}\n"), TIMEOUT) {
            Ok(response) if response == "ok\n" => None,
            Ok(response) => Some(
                response
                    .trim()
                    .strip_prefix("err ")
                    .unwrap_or("The compositor did not accept the layout change.")
                    .to_string(),
            ),
            Err(error) => Some(format!("Could not confirm the layout change: {error}")),
        }
    });
    Response {
        operation,
        snapshot: query(path).map_err(|error| error.to_string()),
        action_error,
    }
}

fn query(path: &Path) -> io::Result<Snapshot> {
    use hearthspace_ipc::transport::{connect_until, write_until};
    let deadline = Instant::now() + TIMEOUT;
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "layout status deadline exceeded")
            })
    };
    let mut stream = connect_until(path, deadline)?;
    write_until(&mut stream, b"frame-settings-status\n", deadline)?;
    // Read the header without consuming any coalesced payload bytes.
    let mut header = Vec::new();
    loop {
        if header.len() >= 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized layout status header",
            ));
        }
        stream.set_read_timeout(Some(remaining()?))?;
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header).map_err(invalid_data)?;
    if let Some(message) = header.strip_prefix("err ") {
        return Err(io::Error::other(message));
    }
    let size = header
        .strip_prefix("ok ")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|size| *size <= MAX_STATUS_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid layout status size"))?;
    let mut payload = vec![0; size];
    let mut received = 0;
    while received < size {
        stream.set_read_timeout(Some(remaining()?))?;
        let count = stream.read(&mut payload[received..])?;
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        received += count;
    }
    let value: serde_json::Value = serde_json::from_slice(&payload).map_err(invalid_data)?;
    let allow_hide_tools = (value["version"].as_u64() == Some(1))
        .then(|| value["allow_hide_tools"].as_bool())
        .flatten()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unsupported layout status"))?;
    Ok(Snapshot { allow_hide_tools })
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests;
