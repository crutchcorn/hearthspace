use super::*;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

pub(super) struct Bus {
    child: Child,
    pub(super) address: String,
    pub(super) directory: PathBuf,
}

impl Bus {
    pub(super) fn start() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "hearthspace-appearance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let config = directory.join("bus.conf");
        // No host activation directories: a test may not spawn an ordinary
        // dconf/portal service or read a different desktop's session bus.
        fs::write(
            &config,
            "<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>\
             <auth>EXTERNAL</auth><policy context=\"default\">\
             <allow own=\"*\"/><allow send_destination=\"*\"/>\
             <allow receive_sender=\"*\"/></policy></busconfig>",
        )
        .unwrap();
        let mut child = Command::new("dbus-daemon")
            .args(["--nofork", "--print-address=1", "--nopidfile"])
            .arg(format!("--config-file={}", config.display()))
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required for isolated appearance portal tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().to_owned(),
            directory,
        }
    }

    pub(super) async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .method_timeout(Duration::from_secs(3))
            .build()
            .await
            .unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub(super) struct Daemon {
    pub(super) socket: PathBuf,
    state: Arc<Mutex<Appearance>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Daemon {
    pub(super) fn start(directory: &Path) -> Self {
        let socket = directory.join("settings.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(Appearance::default()));
        let stopped = Arc::new(AtomicBool::new(false));
        let current = state.clone();
        let stop = stopped.clone();
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut request = String::new();
                        stream.read_to_string(&mut request).unwrap();
                        assert_eq!(request, "get\n");
                        let settings = hearthspace_ipc::Settings {
                            appearance: *current.lock().unwrap(),
                            ..Default::default()
                        };
                        let _ = stream
                            .write_all(hearthspace_ipc::settings_response(settings).as_bytes());
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("mock settingsd: {error}"),
                }
            }
        });
        Self {
            socket,
            state,
            stopped,
            worker: Some(worker),
        }
    }

    pub(super) fn set(&self, appearance: Appearance) {
        *self.state.lock().unwrap() = appearance;
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.worker.take().unwrap().join();
        let _ = fs::remove_file(&self.socket);
    }
}
