//! Exercise job control on a private PTY, never the test runner's terminal.
use std::{
    fs,
    os::fd::{FromRawFd, OwnedFd},
    path::Path,
};

use nix::{
    fcntl::{FcntlArg, FdFlag, fcntl},
    libc,
    sys::signal::{SigHandler, Signal},
};

use super::*;

const PROBE_ENV: &str = "HEARTHSPACE_PROCESS_TERMINAL_TEST";
const TEST_NAME: &str = "process::tests::interactive_login_probe_cannot_stop_the_compositor_group";

#[test]
fn interactive_login_probe_cannot_stop_the_compositor_group() {
    if let Some(directory) = std::env::var_os(PROBE_ENV) {
        run_terminal_probe(Path::new(&directory));
        return;
    }

    let directory = std::env::temp_dir().join(format!(
        "hearthspace-process-terminal-{}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let (_master, slave) = private_pty();
    // Files keep failure collection bounded even if an abruptly killed fixture
    // leaves a descendant holding its inherited output descriptors open.
    let log_path = directory.join("output");
    let log = fs::File::create(&log_path).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(PROBE_ENV, &directory)
        .stdin(Stdio::from(slave))
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log));
    // SAFETY: only async-signal-safe session, terminal and signal syscalls run
    // in the child. This private session cannot signal the Cargo runner group.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setsid().map_err(io::Error::from)?;
            if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            for signal in [Signal::SIGTTIN, Signal::SIGTTOU, Signal::SIGTSTP] {
                signal::signal(signal, SigHandler::SigDfl).map_err(io::Error::from)?;
            }
            Ok(())
        });
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut timed_out = false;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            if let Ok(pid) = fs::read_to_string(directory.join("managed-pid"))
                && let Ok(pid) = pid.parse::<i32>()
            {
                let _ = signal::killpg(Pid::from_raw(pid), Signal::SIGKILL);
            }
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let status = child.wait().unwrap();
    let output = fs::read_to_string(log_path).unwrap();
    let _ = fs::remove_dir_all(directory);
    assert!(
        !timed_out && status.success(),
        "isolated terminal regression failed (timeout: {timed_out}):\n{output}"
    );
}

fn run_terminal_probe(directory: &Path) {
    assert!(
        fs::File::open("/dev/tty").is_ok(),
        "the fixture supervisor must own a controlling terminal"
    );
    let report = directory.join("probe");
    let spec = ProcessSpec {
        name: "terminal-isolation-probe",
        program: "/bin/sh".into(),
        args: [
            "-c",
            // Electron starts an interactive login shell to read its environment.
            // Avoid user startup files, but exercise the same job-control path.
            "/bin/bash --noprofile --norc -il -c \
             'if { : < /dev/tty; } 2>/dev/null; then exit 99; fi; env -0' \
             > /dev/null || exit $?; /bin/sleep 60 & \
             printf '%s\\n' \"$$\" \"$!\" > \"$1.tmp\"; mv \"$1.tmp\" \"$1\"; wait",
            "probe",
        ]
        .into_iter()
        .map(str::to_owned)
        .chain([report.to_string_lossy().into_owned()])
        .collect(),
        envs: Vec::new(),
        env_removals: Vec::new(),
        current_dir: None,
    };
    let process = ManagedFixture(Some(ManagedProcess::spawn(spec, 0).unwrap()));
    let pid = process.0.as_ref().unwrap().pid();
    fs::write(directory.join("managed-pid"), pid.to_string()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !report.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let report = fs::read_to_string(report).unwrap_or_else(|error| {
        panic!(
            "interactive login probe did not finish: {error}; process stat: {:?}",
            fs::read_to_string(format!("/proc/{pid}/stat"))
        )
    });
    let pids = report.lines().collect::<Vec<_>>();
    assert_eq!(pids.len(), 2);
    assert_eq!(pids[0], pid.to_string());
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    assert_eq!(
        fields[2],
        pid.to_string(),
        "cleanup group must remain child PID"
    );
    assert_eq!(
        fields[3],
        pid.to_string(),
        "child must own a new POSIX session"
    );
    assert_eq!(fields[4], "0", "child must not retain the login terminal");
    let descendant = pids[1].parse::<u32>().unwrap();
    assert!(process_running(descendant));
    drop(process);
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_running(descendant) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!process_running(pid), "managed process survived shutdown");
    assert!(
        !process_running(descendant),
        "application descendant survived group cleanup"
    );
}

struct ManagedFixture(Option<ManagedProcess>);

impl Drop for ManagedFixture {
    fn drop(&mut self) {
        terminate_process(&mut self.0);
        wait_then_kill_process_with_timeout(&mut self.0, Duration::from_millis(500));
    }
}

fn process_running(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')
                .and_then(|(_, fields)| fields.split_whitespace().next())
                .map(|state| !matches!(state, "Z" | "X" | "x"))
        })
        .unwrap_or(false)
}

fn private_pty() -> (OwnedFd, OwnedFd) {
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: openpty writes to both valid output pointers; optional termios,
    // window-size and name pointers are null to use the platform defaults.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(result, 0, "openpty failed: {}", io::Error::last_os_error());
    // SAFETY: successful openpty returned two distinct, newly owned descriptors.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        fcntl(fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).unwrap();
    }
    (master, slave)
}
