//! Substitute only Xwayland to inspect its inherited descriptors and lifetime.
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    process::{
        Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, getpgid, kill_process, kill_process_group,
        pidfd_open, test_kill_process, waitid,
    },
};
use std::{
    fs,
    io::{ErrorKind, Read},
    os::{
        fd::{AsFd, OwnedFd},
        unix::{fs::PermissionsExt, net::UnixStream, process::CommandExt},
    },
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Helper {
    child: Child,
    pidfd: OwnedFd,
    nested: Option<OwnedFd>,
    stopped: bool,
}

impl Helper {
    fn new(mut child: Child) -> Self {
        let pid = Pid::from_raw(child.id() as i32).unwrap();
        let pidfd = pidfd_open(pid, PidfdFlags::empty()).unwrap_or_else(|error| {
            let _ = kill_process_group(pid, Signal::KILL);
            let _ = child.wait();
            panic!("could not acquire helper identity: {error}");
        });
        Self {
            child,
            pidfd,
            nested: None,
            stopped: false,
        }
    }

    fn stop(&mut self) -> std::io::Result<()> {
        if self.stopped {
            return Ok(());
        }
        waitid(
            WaitId::PidFd(self.pidfd.as_fd()),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )?;
        let pid = Pid::from_raw(self.child.id() as i32).unwrap();
        if getpgid(Some(pid))? != pid {
            return Err(std::io::Error::other("lost helper group"));
        }
        kill_process_group(pid, Signal::KILL)?;
        wait_exited(&self.pidfd)?;
        if let Some(nested) = &self.nested {
            wait_exited(nested)?;
        }
        self.child.wait()?;
        self.stopped = true;
        Ok(())
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("helper cleanup requires recovery: {error}");
        }
    }
}

fn wait_exited(fd: &OwnedFd) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let mut descriptors = [PollFd::new(fd, PollFlags::IN)];
        match poll(
            &mut descriptors,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            }),
        ) {
            Ok(n) if n > 0 => return Ok(()),
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(std::io::Error::new(
        ErrorKind::TimedOut,
        "owned process did not exit",
    ))
}

#[test]
fn nested_xwayland_cannot_keep_private_wayland_or_readiness_open() {
    let runtime = tempfile::tempdir().unwrap();
    let executable = runtime.path().join("Xwayland");
    let record = runtime.path().join("nested.txt");
    fs::write(&executable, r#"#!/usr/bin/python3 -I
import os, time
with open(os.environ["SATELLITE_TEST_RECORD"], "w") as output:
    output.write(f"{os.getpid()}\n{os.readlink('/proc/self/fd/0')}\n{os.readlink('/proc/self/fd/1')}\n")
time.sleep(30)
"#).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let (mut host, peer) = UnixStream::pair().unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_hearthspace-satellite"))
        .arg("--embedded")
        .process_group(0)
        .env("PATH", runtime.path())
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("SATELLITE_TEST_RECORD", &record)
        .env("RUST_LOG", "warn")
        .stdin(Stdio::from(OwnedFd::from(peer)))
        .stdout(Stdio::piped())
        .stderr(Stdio::from(
            fs::File::create(runtime.path().join("helper.log")).unwrap(),
        ))
        .spawn()
        .unwrap();
    let mut helper = Helper::new(child);
    let mut readiness = helper.child.stdout.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let lines = loop {
        if let Ok(text) = fs::read_to_string(&record) {
            let lines: Vec<_> = text.lines().map(str::to_owned).collect();
            if lines.len() == 3 {
                break lines;
            }
        }
        assert!(
            Instant::now() < deadline,
            "nested process did not record its descriptors"
        );
        thread::sleep(Duration::from_millis(10));
    };
    let nested = Pid::from_raw(lines[0].parse().unwrap()).unwrap();
    helper.nested = Some(pidfd_open(nested, PidfdFlags::empty()).unwrap());
    let leader = Pid::from_raw(helper.child.id() as i32).unwrap();
    assert_eq!(getpgid(Some(nested)).unwrap(), leader);
    assert_eq!(&lines[1..], ["/dev/null", "/dev/null"]);
    // Kill only the leader. Keep it unreaped to anchor group ownership while
    // proving that its surviving child holds neither private connection.
    kill_process(leader, Signal::KILL).unwrap();
    wait_exited(&helper.pidfd).unwrap();
    assert!(test_kill_process(nested).is_ok());
    host.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    assert_eq!(
        host.read(&mut [0]).unwrap(),
        0,
        "nested process retained the host socket"
    );
    fcntl_setfl(
        &readiness,
        fcntl_getfl(&readiness).unwrap() | OFlags::NONBLOCK,
    )
    .unwrap();
    let until = Instant::now() + Duration::from_millis(500);
    loop {
        match readiness.read(&mut [0]) {
            Ok(0) => break,
            Err(error) if error.kind() == ErrorKind::WouldBlock && Instant::now() < until => {
                thread::sleep(Duration::from_millis(5));
            }
            result => panic!("readiness pipe did not close with its owner: {result:?}"),
        }
    }
    helper.stop().unwrap();
}
