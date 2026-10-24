use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use crate::favorites::FavoriteStore;

use super::*;

struct Writer(Child);

impl Writer {
    fn start(directory: &Path, app: &str) -> Self {
        Self(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "favorites::storage::transaction::tests::writer_child",
                    "--nocapture",
                ])
                .env("HEARTHSPACE_FAVORITES_TEST_DIR", directory)
                .env("HEARTHSPACE_FAVORITES_TEST_APP", app)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        )
    }

    fn finish(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "writer did not exit");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn writer_child() {
    let Some(directory) = std::env::var_os("HEARTHSPACE_FAVORITES_TEST_DIR") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let app = std::env::var("HEARTHSPACE_FAVORITES_TEST_APP").unwrap();
    let mut store = FavoriteStore::load(directory.join("favorites.json")).unwrap();
    let previous = store.entries().to_vec();
    let mut channel = UnixStream::connect(directory.join("ready.sock")).unwrap();
    channel
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    channel.write_all(b"R").unwrap();
    let mut go = [0];
    channel.read_exact(&mut go).unwrap();
    assert_eq!(&go, b"G");
    let result = match store.add(&app) {
        Ok(()) => b'O',
        Err(error) => {
            assert_eq!(store.entries(), previous, "failed save mutated memory");
            if error.0.contains("changed on disk") {
                b'C'
            } else {
                assert!(error.0.contains("another favorites save"), "{error}");
                b'B'
            }
        }
    };
    channel.write_all(&[result]).unwrap();
}

fn accept(listener: &UnixListener) -> UnixStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut channel = loop {
        match listener.accept() {
            Ok((channel, _)) => break channel,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "writer did not start");
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("{error}"),
        }
    };
    channel
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut ready = [0];
    channel.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"R");
    channel
}

#[test]
fn competing_processes_commit_once_and_reject_the_stale_writer() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    let listener = UnixListener::bind(directory.path().join("ready.sock")).unwrap();
    let guard = lock(&path).unwrap();
    let mut first = Writer::start(directory.path(), "first.desktop");
    let mut second = Writer::start(directory.path(), "second.desktop");
    let mut a = accept(&listener);
    let mut b = accept(&listener);
    a.write_all(b"G").unwrap();
    b.write_all(b"G").unwrap();
    // Both production writers must respect the stable lock before comparing
    // or renaming. The original implementation returns success here.
    a.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let mut result = [0];
    let error = a.read_exact(&mut result).unwrap_err();
    assert!(matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ));
    assert!(!path.exists());
    drop(guard);
    a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    a.read_exact(&mut result).unwrap();
    let mut other = [0];
    b.read_exact(&mut other).unwrap();
    let mut outcomes = [result[0], other[0]];
    outcomes.sort();
    assert_eq!(outcomes, [b'C', b'O']);
    first.finish();
    second.finish();
    assert_eq!(FavoriteStore::load(&path).unwrap().entries().len(), 1);
    let mut fresh = FavoriteStore::load(&path).unwrap();
    fresh.add("fresh.desktop").unwrap();
    assert_eq!(FavoriteStore::load(path).unwrap().entries().len(), 2);
}

#[test]
fn busy_writer_is_bounded_and_a_later_retry_succeeds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    let listener = UnixListener::bind(directory.path().join("ready.sock")).unwrap();
    let guard = lock(&path).unwrap();
    let mut writer = Writer::start(directory.path(), "blocked.desktop");
    let mut channel = accept(&listener);
    channel.write_all(b"G").unwrap();
    let mut result = [0];
    channel.read_exact(&mut result).unwrap();
    assert_eq!(&result, b"B");
    writer.finish();
    assert!(!path.exists());
    drop(guard);
    let mut fresh = FavoriteStore::load(&path).unwrap();
    fresh.add("retry.desktop").unwrap();
    assert_eq!(
        FavoriteStore::load(path).unwrap().entries(),
        fresh.entries()
    );
}

#[test]
fn unsafe_lock_files_are_rejected_without_changing_favorites() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    let other = directory.path().join("other");
    fs::write(&other, b"keep").unwrap();
    std::os::unix::fs::symlink(&other, lock_path(&path)).unwrap();
    assert!(lock(&path).is_err());
    assert_eq!(fs::read(&other).unwrap(), b"keep");
    assert!(!path.exists());
}
