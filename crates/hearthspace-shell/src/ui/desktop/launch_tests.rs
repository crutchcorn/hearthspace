//! Real task-tree regression with discovery blocked on a disposable FIFO.
use crate::ui::{desktop, launch, launch::tests::worker, state::ShellState};
use std::{
    fs,
    io::Write,
    os::unix::net::UnixListener,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const CHILD: &str = "HEARTHSPACE_STALLED_DISCOVERY_PROBE";
const NAME: &str = "ui::desktop::task::tests::launches::stalled_discovery_does_not_delay_launch_acceptance_or_completion";

#[test]
fn stalled_discovery_does_not_delay_launch_acceptance_or_completion() {
    if let Some(directory) = std::env::var_os(CHILD) {
        run_stalled_discovery(std::path::Path::new(&directory));
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture"])
        .env_clear()
        .env(CHILD, directory.path())
        .env("HOME", directory.path())
        .env("XDG_DATA_HOME", directory.path())
        .env("XDG_DATA_DIRS", directory.path())
        .env("XDG_CONFIG_HOME", directory.path())
        .env("XDG_CONFIG_DIRS", directory.path())
        .env("XDG_RUNTIME_DIR", directory.path())
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated launch regression timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn run_stalled_discovery(directory: &std::path::Path) {
    let mut state = ShellState::new(directory.join("control.sock"), vec!["eDP-1".into()]);
    state.favorites.tracks_installed_apps = true;
    state.outputs[0].show_search = true;
    state.outputs[0].query = "editor".into();
    // Create the blocking catalog entry after initial ShellState discovery.
    let applications = directory.join("applications");
    fs::create_dir(&applications).unwrap();
    let fifo = applications.join("stalled.desktop");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let stopping = Arc::new(AtomicBool::new(false));
    let stop = stopping.clone();
    let (launched, received) = mpsc::channel();
    let server = std::thread::spawn(move || {
        while !stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => serve(stream, &launched),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => panic!("fake endpoint: {error}"),
            }
        }
    });
    let mut task = worker::Worker::new(desktop::task(), &mut state);
    // A nonblocking writer succeeds only when real discovery has opened the
    // FIFO. Keeping it open without data holds that worker in read_to_string.
    let deadline = Instant::now() + Duration::from_secs(3);
    let writer = loop {
        match rustix::fs::openat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        ) {
            Ok(fd) => break fs::File::from(fd),
            Err(rustix::io::Errno::NXIO) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1))
            }
            Err(error) => panic!("discovery did not reach FIFO: {error}"),
        }
    };
    launch::app(&mut state, "eDP-1", "editor.desktop", Some("eDP-1"), false);
    assert!(state.outputs[0].launch_pending.is_some());
    received
        .recv_timeout(Duration::from_secs(2))
        .expect("launch must reach control while discovery is still blocked");
    task.complete(&mut state);
    assert!(!state.outputs[0].show_search);
    assert!(state.outputs[0].launch_error.is_none());
    assert!(state.outputs[0].launch_pending.is_none());
    // Release the filesystem boundary only after the launch UI completion.
    fs::remove_file(&fifo).unwrap();
    drop(writer);
    drop(task);
    stopping.store(true, Ordering::Release);
    server.join().unwrap();
}

fn serve(mut stream: std::os::unix::net::UnixStream, launched: &mpsc::Sender<()>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let request = worker::read_request(&mut stream);
    if request.starts_with("launch-app") {
        assert_eq!(
            request,
            format!(
                "{}\n",
                crate::ShellCommand::LaunchAppOnOutput {
                    app_id: "editor.desktop".into(),
                    output_name: "eDP-1".into(),
                }
                .wire_name()
            )
        );
        launched.send(()).unwrap();
        stream.write_all(b"ok\n").unwrap();
    } else if request == "status\n" {
        worker::payload(
            &mut stream,
            r#"{"version":1,"backend":"headless","active":true,"locked":false,"recovery_ready":true}"#,
        );
    } else if request == "desktop-status\n" {
        worker::payload(
            &mut stream,
            r#"{"version":1,"revision":0,"select_mode":true,"items":[]}"#,
        );
    } else {
        // Initial catalog registration may resume while the task is torn down.
        assert!(
            request.starts_with("desktop-"),
            "unexpected request: {request}"
        );
        let _ = stream.write_all(b"ok\n");
    }
}
