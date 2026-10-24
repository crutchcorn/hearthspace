use std::{io, path::Path, time::Duration};

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
    let payload = hearthspace_ipc::transport::request_payload(
        path,
        "frame-settings-status",
        TIMEOUT,
        MAX_STATUS_BYTES,
    )?;
    let value: hearthspace_ipc::frame_layout::SettingsSnapshot =
        serde_json::from_slice(&payload).map_err(invalid_data)?;
    if value.version != 1 {
        return Err(invalid_data("unsupported layout status"));
    }
    Ok(Snapshot {
        allow_hide_tools: value.allow_hide_tools,
    })
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests;
