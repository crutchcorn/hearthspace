//! The workspace Smithay dependency owns termination across callback/drop order.
#![cfg(feature = "xwayland")]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

use smithay::{
    reexports::rustix::{
        io::Errno,
        process::{Pid, WaitId, WaitIdOptions, waitid},
    },
    xwayland::{XWayland, XWaylandClientData},
};
use wayland_server::{Client, Display, backend::DisconnectReason};

struct Fixture {
    xwayland: Option<XWayland>,
    client: Client,
    display: Display<()>,
    pid: Pid,
    _root: tempfile::TempDir,
}

impl Fixture {
    fn new(exit_early: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("selected-Xwayland");
        let marker = root.path().join("pid");
        fs::write(
            &binary,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$MARKER\"\n{}\n",
                if exit_early {
                    "exit 17"
                } else {
                    "exec /bin/sleep 90"
                }
            ),
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let display = Display::new().unwrap();
        let (xwayland, client) = XWayland::spawn_with_executable(
            &display.handle(),
            &binary,
            None,
            [("MARKER", marker.as_os_str())],
            ["-surface-input-targets"],
            false,
            Stdio::null(),
            Stdio::inherit(),
            |_| (),
        )
        .unwrap();
        // Keep XWayland's owned Drop guard live during this bounded wait too.
        let deadline = Instant::now() + Duration::from_secs(2);
        let pid = loop {
            if let Some(pid) = fs::read_to_string(&marker)
                .ok()
                .and_then(|text| text.trim().parse().ok())
                .and_then(Pid::from_raw)
            {
                break pid;
            }
            assert!(
                Instant::now() < deadline,
                "fake child did not record its PID"
            );
            thread::sleep(Duration::from_millis(5));
        };
        Self {
            xwayland: Some(xwayland),
            client,
            display,
            pid,
            _root: root,
        }
    }

    fn data(&self) -> &XWaylandClientData {
        self.client.get_data().unwrap()
    }

    fn disconnect(&self) {
        self.display
            .handle()
            .backend_handle()
            .kill_client(self.client.id(), DisconnectReason::ConnectionClosed);
    }

    fn assert_reaped(&self) {
        // The fake server is our direct child. ECHILD proves cleanup already
        // reaped it; a live child or zombie would return Ok(None/Some).
        assert_eq!(
            waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
            )
            .unwrap_err(),
            Errno::CHILD,
        );
        assert!(self.data().terminate_and_wait().unwrap().is_none());
    }
}

#[test]
fn explicit_cleanup_terminates_and_reaps_before_handle_drop() {
    let fixture = Fixture::new(false);
    assert!(fixture.data().terminate_and_wait().unwrap().is_some());
    fixture.assert_reaped();
}

#[test]
fn early_disconnect_reaps_before_later_handle_drop() {
    let mut fixture = Fixture::new(false);
    fixture.disconnect();
    fixture.assert_reaped();
    drop(fixture.xwayland.take());
    fixture.assert_reaped();
}

#[test]
fn handle_drop_reaps_before_later_disconnect() {
    let mut fixture = Fixture::new(false);
    drop(fixture.xwayland.take());
    fixture.assert_reaped();
    fixture.disconnect();
    fixture.assert_reaped();
}

#[test]
fn concurrent_cleanup_keeps_one_owned_child_and_reaps_once() {
    let fixture = Fixture::new(false);
    let data = fixture.data();
    let statuses = thread::scope(|scope| {
        let first = scope.spawn(|| data.terminate_and_wait().unwrap());
        let second = scope.spawn(|| data.terminate_and_wait().unwrap());
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(statuses.iter().filter(|status| status.is_some()).count(), 1);
    fixture.assert_reaped();
}

#[test]
fn natural_early_exit_callback_reaps_without_losing_the_child() {
    let mut fixture = Fixture::new(true);
    let deadline = Instant::now() + Duration::from_secs(2);
    while fixture
        .display
        .handle()
        .backend_handle()
        .get_client_data(fixture.client.id())
        .is_ok()
    {
        fixture.display.dispatch_clients(&mut ()).unwrap();
        assert!(
            Instant::now() < deadline,
            "exited client's disconnect was not delivered"
        );
        thread::sleep(Duration::from_millis(5));
    }
    fixture.assert_reaped();
    drop(fixture.xwayland.take());
    fixture.assert_reaped();
}
