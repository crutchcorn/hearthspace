use std::{io, process::Stdio};

use tokio::{io::AsyncReadExt, process::Command};

use super::AudioBackend;

const OUTPUT_LIMIT: u64 = 1024 * 1024;
const ERROR_LIMIT: u64 = 16 * 1024;
const READ_ERROR: &str = "Audio controls could not read the audio service response.";

impl AudioBackend {
    pub(super) async fn run(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        // No shell, no sudo, no replacement audio daemon. Keep the inherited
        // PulseAudio server/runtime/cookie settings intact.
        let mut child = Command::new(&self.program)
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => "Volume controls require pactl (pulseaudio-utils).",
                _ => "The audio control utility could not be started.",
            })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let result = tokio::time::timeout(self.timeout, async {
            tokio::try_join!(
                read_limited(stdout, OUTPUT_LIMIT),
                read_limited(stderr, ERROR_LIMIT),
                async { child.wait().await.map_err(|_| READ_ERROR.to_string()) },
            )
        })
        .await;
        let (output, error, status) = match result {
            Ok(Ok(result)) => result,
            result => {
                // Reap failures/timeouts promptly. kill_on_drop also handles a
                // cancelled request (for example when the shell exits).
                let _ = tokio::time::timeout(self.timeout, child.kill()).await;
                return Err(match result {
                    Ok(Err(error)) => error,
                    _ => "The audio service is not responding.".into(),
                });
            }
        };
        if !status.success() {
            let error = String::from_utf8_lossy(&error);
            return Err(if error.contains("No such entity") {
                "No audio output is available."
            } else if error.contains("Connection failure") || error.contains("Connection refused") {
                "The audio service is unavailable."
            } else if error.contains("unrecognized option") || error.contains("invalid option") {
                "Volume controls require a pactl version with JSON support."
            } else {
                "The audio service could not complete this request."
            }
            .into());
        }
        Ok(output)
    }
}

async fn read_limited(
    stream: impl tokio::io::AsyncRead + Unpin,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    stream
        .take(limit + 1)
        .read_to_end(&mut output)
        .await
        .map_err(|_| READ_ERROR)?;
    if output.len() as u64 > limit {
        return Err("The audio service response was too large.".into());
    }
    Ok(output)
}
