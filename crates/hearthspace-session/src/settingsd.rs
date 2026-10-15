use std::{
    io::{Read, Write},
    net::Shutdown,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use crate::readiness::{connect_until, remaining};

pub(crate) fn settingsd_ping(
    socket_path: &Path,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> std::io::Result<()> {
    let deadline = deadline.min(Instant::now() + Duration::from_millis(250));
    let mut stream = connect_until(socket_path, deadline, cancelled)?;
    stream.set_write_timeout(Some(remaining(deadline, cancelled)?))?;
    stream.write_all(b"get\n")?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        stream.set_read_timeout(Some(remaining(deadline, cancelled)?))?;
        let length = stream.read(&mut buffer)?;
        if length == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        response.extend_from_slice(&buffer[..length]);
        if response.len() > 8192 {
            return Err(std::io::Error::other("oversized settings response"));
        }
        if response.contains(&b'\n') {
            return if response.starts_with(b"ok ") {
                Ok(())
            } else {
                Err(std::io::Error::other("settings service returned an error"))
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixListener, sync::mpsc, thread, time::Instant};

    #[test]
    fn trickling_settings_reply_cannot_extend_deadline() {
        let path = std::env::temp_dir().join(format!(
            "hearthspace-settings-trickle-{}.sock",
            std::process::id()
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            for _ in 0..20 {
                if stream.write_all(b"x").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(40));
            }
        });
        let start = Instant::now();
        assert!(
            settingsd_ping(
                &path,
                start + Duration::from_millis(100),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_millis(500));
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

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
        assert!(
            settingsd_ping(
                &path,
                Instant::now() + Duration::from_secs(2),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        done.send(()).unwrap();
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
