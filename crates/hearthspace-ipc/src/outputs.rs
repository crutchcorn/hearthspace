//! Authoritative compositor output identity, independent of wl_output order.
use std::{io, path::Path, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryOutput {
    pub version: u32,
    pub primary: Option<String>,
}

pub fn client_get_primary_output() -> io::Result<Option<String>> {
    let path = std::env::var_os(hearthspace_common::SHELL_COMMAND_SOCKET_ENV)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "compositor socket is not set"))?;
    client_get_primary_output_at(Path::new(&path))
}

pub fn client_get_primary_output_at(path: &Path) -> io::Result<Option<String>> {
    let bytes = crate::transport::request_payload(
        path,
        "output-primary",
        Duration::from_millis(500),
        8192,
    )?;
    let reply: PrimaryOutput = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if reply.version != 1
        || reply
            .primary
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.len() > 4096)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid output identity",
        ));
    }
    Ok(reply.primary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
    };

    #[test]
    fn primary_reply_preserves_connector_identity_and_rejects_other_versions() {
        for (body, expected) in [
            (
                r#"{"version":1,"primary":"Virtual output 2"}"#,
                Ok(Some("Virtual output 2")),
            ),
            (r#"{"version":1,"primary":null}"#, Ok(None)),
            (r#"{"version":2,"primary":"eDP-1"}"#, Err(())),
            (r#"{"version":1,"primary":""}"#, Err(())),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("control.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut command = [0; 15];
                stream.read_exact(&mut command).unwrap();
                assert_eq!(&command, b"output-primary\n");
                write!(stream, "ok {}\n{body}", body.len()).unwrap();
            });
            let actual = client_get_primary_output_at(&socket);
            server.join().unwrap();
            assert_eq!(
                actual.as_ref().map(|name| name.as_deref()).map_err(|_| ()),
                expected
            );
        }
    }
}
