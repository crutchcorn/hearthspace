//! The existing control socket, with a single deadline and bounded replies.
use std::{io, path::Path, time::Duration};

use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub(super) async fn command(path: &Path, command: &str) -> io::Result<Value> {
    // Reopening DRM can synchronously compile shaders before replying. Keep
    // it bounded without treating an ordinary native rebuild as a lost command.
    let timeout = if command == "session-resume" {
        Duration::from_secs(8)
    } else {
        Duration::from_millis(750)
    };
    tokio::time::timeout(timeout, exchange(path, command))
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "compositor power request timed out",
            )
        })?
}

async fn exchange(path: &Path, command: &str) -> io::Result<Value> {
    let mut stream = UnixStream::connect(path).await?;
    stream.write_all(format!("{command}\n").as_bytes()).await?;
    let mut header = Vec::new();
    loop {
        let byte = stream.read_u8().await?;
        if byte == b'\n' {
            break;
        }
        if header.len() == 2048 {
            return Err(io::Error::other("oversized control header"));
        }
        header.push(byte);
    }
    let header = std::str::from_utf8(&header).map_err(io::Error::other)?;
    if header == "ok" {
        return Ok(Value::Null);
    }
    let length = header
        .strip_prefix("ok ")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|length| *length <= 256 * 1024)
        .ok_or_else(|| io::Error::other(format!("compositor: {header}")))?;
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    serde_json::from_slice(&body).map_err(io::Error::other)
}

pub(super) fn confirmed(status: &Value) -> bool {
    status["locked"] == true && status["lock_confirmed"] == true
}

#[cfg(test)]
mod tests;
