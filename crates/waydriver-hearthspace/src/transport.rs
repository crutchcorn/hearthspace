use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    process::Child,
};
use waydriver::{Error, Result};

pub(crate) async fn wait_for_socket(child: Option<&mut Child>, path: &Path) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut child = child;
    let mut last_error = None;

    while tokio::time::Instant::now() < deadline {
        if let Some(child) = child.as_deref_mut()
            && let Some(status) = child
                .try_wait()
                .map_err(|e| Error::process_with("poll hearthspace", e))?
        {
            return Err(Error::process(format!(
                "hearthspace exited before accepting commands: {status}"
            )));
        }

        match UnixStream::connect(path).await {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error),
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    Err(Error::Timeout(format!(
        "timed out connecting to {}: {:?}",
        path.display(),
        last_error
    )))
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PAYLOAD: usize = 128 * 1024 * 1024;

pub(crate) async fn send_text_command(path: &Path, command: &str) -> Result<Vec<u8>> {
    tokio::time::timeout(REQUEST_TIMEOUT, request(path, command))
        .await
        .map_err(|_| Error::Timeout(format!("Hearthspace command timed out: {command}")))?
}

pub(crate) async fn take_screenshot(path: &Path) -> Result<Vec<u8>> {
    let payload = send_text_command(path, "screenshot").await?;
    if !payload.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(Error::screenshot("invalid or empty screenshot payload"));
    }
    Ok(payload)
}

async fn request(path: &Path, command: &str) -> Result<Vec<u8>> {
    if command.contains(['\n', '\r']) || command.len() > 4094 {
        return Err(Error::process("invalid command line"));
    }
    let mut stream = UnixStream::connect(path)
        .await
        .map_err(|e| Error::process_with("connect command socket", e))?;
    stream
        .write_all(format!("{command}\n").as_bytes())
        .await
        .map_err(|e| Error::process_with("write command", e))?;
    let header = read_line(&mut stream).await?;
    if let Some(message) = header.strip_prefix("err ") {
        return Err(Error::process(message.trim_end()));
    }
    if header == "ok\n" {
        return Ok(Vec::new());
    }
    let length = header
        .strip_prefix("ok ")
        .and_then(|rest| rest.trim_end().parse::<usize>().ok())
        .ok_or_else(|| Error::process(format!("invalid response header: {header:?}")))?;
    if length > MAX_PAYLOAD {
        return Err(Error::process("response exceeds 128 MiB limit"));
    }
    let mut payload = vec![0; length];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| Error::process_with("read complete command response", e))?;
    Ok(payload)
}

async fn read_line(stream: &mut UnixStream) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        if bytes.len() >= 4096 {
            return Err(Error::process("oversized response header"));
        }
        let byte = stream
            .read_u8()
            .await
            .map_err(|e| Error::process_with("read reply", e))?;
        bytes.push(byte);
        if byte == b'\n' {
            break;
        }
    }
    String::from_utf8(bytes).map_err(|e| Error::process_with("decode reply", e))
}
