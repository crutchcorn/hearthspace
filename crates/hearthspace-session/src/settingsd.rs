use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub(crate) fn settingsd_ping(socket_path: &Path) -> std::io::Result<()> {
    if !socket_path.exists() {
        return Err(std::io::ErrorKind::NotFound.into());
    }
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(Duration::from_millis(250)))?;
    stream.write_all(b"get\n")?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = String::new();
    stream.take(8192).read_to_string(&mut response)?;
    if response.starts_with("ok ") {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "unexpected settingsd response: {response:?}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixListener, sync::mpsc, thread, time::Instant};

    #[test]
    fn unresponsive_settings_service_cannot_block_startup() {
        let path = std::env::temp_dir().join(format!(
            "hearthspace-settings-ping-{}.sock",
            std::process::id()
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let (done, wait) = mpsc::channel::<()>();
        let worker = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            let _ = wait.recv_timeout(Duration::from_secs(3));
        });
        let start = Instant::now();
        assert!(settingsd_ping(&path).is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        done.send(()).unwrap();
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
