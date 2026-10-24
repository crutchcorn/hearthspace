use super::*;

/// Read a length-framed control response under one connect/write/read deadline.
/// The caller supplies the payload cap for its protocol. Header reads never
/// consume coalesced body bytes and advertised sizes are checked before allocation.
pub fn request_payload(
    path: &Path,
    command: &str,
    timeout: Duration,
    max_payload: usize,
) -> io::Result<Vec<u8>> {
    if command.is_empty() || command.contains(['\n', '\r']) || command.len() >= MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid control command",
        ));
    }
    let deadline = Instant::now() + timeout;
    let mut stream = connect_until(path, deadline)?;
    write_until(&mut stream, format!("{command}\n").as_bytes(), deadline)?;
    let mut header = Vec::new();
    loop {
        if header.len() >= 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized response header",
            ));
        }
        let mut byte = [0];
        read_exact_until(&mut stream, &mut byte, deadline)?;
        if byte[0] == b'\n' {
            break;
        }
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if let Some(message) = header.strip_prefix("err ") {
        return Err(io::Error::other(message));
    }
    let length = header
        .strip_prefix("ok ")
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|length| *length <= max_payload)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid or oversized response")
        })?;
    let mut payload = vec![0; length];
    read_exact_until(&mut stream, &mut payload, deadline)?;
    Ok(payload)
}

fn read_exact_until(
    stream: &mut UnixStream,
    mut buffer: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    while !buffer.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(buffer) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(length) => buffer = &mut buffer[length..],
            Err(error) if retryable(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
