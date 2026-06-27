use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
#[cfg(feature = "test-apps")]
const CLIENT_WIDTH: u32 = 800;
#[cfg(feature = "test-apps")]
const CLIENT_HEIGHT: u32 = 600;

static HEADLESS_TEST_LOCK: Mutex<()> = Mutex::new(());

struct HeadlessCompositor {
    child: Child,
}

impl HeadlessCompositor {
    fn spawn() -> Self {
        Self::spawn_with_size("320x240")
    }

    fn spawn_with_size(size: &str) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_hearthspace"))
            .args(["--headless", "--no-shell", "--headless-size", size])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn headless Hearthspace");

        Self { child }
    }

    fn wait_for_socket(&mut self) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(10);
        let path = command_socket_path();
        let mut last_error = None;

        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().expect("poll compositor") {
                panic!("headless Hearthspace exited before accepting commands: {status}");
            }

            match UnixStream::connect(&path) {
                Ok(stream) => return stream,
                Err(error) => last_error = Some(error),
            }

            thread::sleep(Duration::from_millis(50));
        }

        panic!(
            "timed out connecting to {}: {:?}",
            path.display(),
            last_error
        );
    }
}

impl Drop for HeadlessCompositor {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn headless_control_socket_drives_input_screenshot_and_quit() {
    let _guard = HEADLESS_TEST_LOCK.lock().expect("headless test lock");
    let mut compositor = HeadlessCompositor::spawn();
    let first_stream = compositor.wait_for_socket();
    drop(first_stream);

    for command in [
        "pointer-motion-abs 10 10",
        "pointer-motion-rel 5 5",
        "pointer-button-down 272",
        "pointer-button-up 272",
        "axis 0 -120",
        "key-down 30",
        "key-up 30",
    ] {
        assert_eq!(send_text_command(command), "ok\n", "command {command:?}");
    }

    let screenshot = take_screenshot();
    assert!(screenshot.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(png_dimensions(&screenshot), (WIDTH, HEIGHT));

    assert_eq!(send_text_command("quit"), "ok\n");
    wait_for_exit(&mut compositor.child);
}

#[cfg(feature = "test-apps")]
#[test]
fn headless_control_socket_spawns_and_drives_real_gtk_client() {
    let _guard = HEADLESS_TEST_LOCK.lock().expect("headless test lock");
    let mut compositor = HeadlessCompositor::spawn_with_size("800x600");
    let first_stream = compositor.wait_for_socket();
    drop(first_stream);

    let empty = take_screenshot();
    assert_eq!(png_dimensions(&empty), (CLIENT_WIDTH, CLIENT_HEIGHT));

    assert_eq!(send_text_command("spawn a11y-test"), "ok\n");
    let with_client = wait_for_screenshot_change(&empty);
    assert_eq!(png_dimensions(&with_client), (CLIENT_WIDTH, CLIENT_HEIGHT));
    let saw_accessible = wait_for_accessible_term("Research Workspace");
    if std::env::var_os("HEARTHSPACE_REQUIRE_ATSPI").is_some() {
        assert!(saw_accessible, "AT-SPI tree did not expose GTK test app");
    } else if !saw_accessible {
        eprintln!(
            "GTK test app did not appear on AT-SPI bus; set HEARTHSPACE_REQUIRE_ATSPI=1 to make this fatal"
        );
    }

    for command in [
        "pointer-motion-abs 420 260",
        "pointer-button-down 272",
        "pointer-button-up 272",
        "key-down 30",
        "key-up 30",
        "screenshot",
    ] {
        if command == "screenshot" {
            assert_eq!(
                png_dimensions(&take_screenshot()),
                (CLIENT_WIDTH, CLIENT_HEIGHT)
            );
        } else {
            assert_eq!(send_text_command(command), "ok\n", "command {command:?}");
        }
    }

    assert_eq!(send_text_command("quit"), "ok\n");
    wait_for_exit(&mut compositor.child);
}

fn command_socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("hearthspace-shell.sock")
}

fn send_text_command(command: &str) -> String {
    let mut stream = UnixStream::connect(command_socket_path()).expect("connect command socket");
    stream
        .write_all(format!("{command}\n").as_bytes())
        .expect("write command");
    read_line(&mut stream)
}

fn take_screenshot() -> Vec<u8> {
    let mut stream = UnixStream::connect(command_socket_path()).expect("connect command socket");
    stream.write_all(b"screenshot\n").expect("write screenshot");

    let header = read_line(&mut stream);
    let mut parts = header.split_whitespace();
    assert_eq!(parts.next(), Some("ok"));
    let len = parts
        .next()
        .expect("screenshot byte length")
        .parse::<usize>()
        .expect("valid screenshot byte length");
    assert_eq!(parts.next(), None);

    let mut payload = vec![0; len];
    stream.read_exact(&mut payload).expect("read PNG payload");
    payload
}

#[cfg(feature = "test-apps")]
fn wait_for_screenshot_change(previous: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let screenshot = take_screenshot();
        if screenshot != previous {
            return screenshot;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("screenshot did not change after spawning GTK test client");
}

#[cfg(feature = "test-apps")]
fn wait_for_accessible_term(term: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if async_io::block_on(hearthspace::accessibility::accessibility_tree_contains_term(term))
            .expect("query AT-SPI tree")
        {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

fn read_line(stream: &mut UnixStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).expect("read reply byte");
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    String::from_utf8(bytes).expect("utf-8 reply")
}

fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert!(png.len() >= 24);
    let width = u32::from_be_bytes(png[16..20].try_into().expect("PNG width bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("PNG height bytes"));
    (width, height)
}

fn wait_for_exit(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if child.try_wait().expect("poll compositor").is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("headless Hearthspace did not exit after quit command");
}
