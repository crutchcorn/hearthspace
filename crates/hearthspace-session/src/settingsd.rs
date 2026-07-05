use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::Path,
};

pub(crate) fn settingsd_ping(socket_path: &Path) -> std::io::Result<()> {
    if !socket_path.exists() {
        return Err(std::io::ErrorKind::NotFound.into());
    }
    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(b"get\n")?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.starts_with("ok ") {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "unexpected settingsd response: {response:?}"
        )))
    }
}
