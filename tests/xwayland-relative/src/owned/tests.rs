//! Cancellation exercises the real supervisor, with only the worker substituted.
use super::{Group, environment, supervise_command};
use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const CHILD_OUTPUT: &str = "HEARTHSPACE_CANCELLATION_TEST_OUTPUT";
const TEST_NAME: &str = "owned::tests::cancellation_reaps_worker_and_closes_private_runtime";
const WORKER: &str = r#"
import json, os, signal, socket, sys
from pathlib import Path

# This external worker owns a real private socket; no Wayland/X11 scenario is
# replaced in the production fixture. Its supervisor is the production code.
listener = socket.socket(socket.AF_UNIX)
listener.bind('x11')
ready = Path(sys.argv[-1]) / 'worker-ready.json'
value = {'pid': os.getpid(), 'runtime': str(Path.cwd()),
         'starttime': Path('/proc/self/stat').read_text().rsplit(')', 1)[1].split()[19]}
ready.with_suffix('.tmp').write_text(json.dumps(value))
ready.with_suffix('.tmp').rename(ready)
signal.pause()
"#;

fn run_supervisor(output: PathBuf) {
    let opts = crate::Options {
        binary: PathBuf::from("/usr/bin/python3"),
        targets: false,
        worker: false,
        artifacts: None,
        output: Some(output),
    };
    let mut command = Command::new("/usr/bin/python3");
    command.args(["-B", "-c", WORKER]);
    let code = supervise_command(&opts, command).expect("supervisor cancellation cleanup");
    assert_eq!(code, 128 + libc::SIGTERM, "cancellation must never pass");
}

fn wait_for(supervisor: &Group, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(!supervisor.exited().expect("observe unreaped supervisor"));
        assert!(Instant::now() < deadline, "supervisor readiness deadline");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cancellation_reaps_worker_and_closes_private_runtime() {
    // A separate test process contains the signal dispositions, so the test
    // never changes handlers or sends cancellation to the parent test harness.
    if let Some(output) = std::env::var_os(CHILD_OUTPUT) {
        run_supervisor(output.into());
        return;
    }
    let directory = tempfile::tempdir().expect("test evidence");
    let output = directory.path().join("evidence");
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command.args(["--exact", TEST_NAME, "--nocapture"]);
    environment(&mut command, directory.path());
    command
        .env(CHILD_OUTPUT, &output)
        .process_group(0)
        .stdin(Stdio::null());
    let mut supervisor = Group {
        child: command.spawn().expect("isolated supervisor"),
        reaped: false,
    };
    let ready = output.join("worker-ready.json");
    wait_for(&supervisor, || ready.is_file());
    let worker: serde_json::Value =
        serde_json::from_slice(&fs::read(ready).expect("worker evidence")).expect("worker JSON");
    let runtime = Path::new(worker["runtime"].as_str().expect("private runtime"));
    assert!(runtime.join("x11").exists(), "cancel after socket creation");
    // Our direct child is not reaped until all signals and group cleanup end.
    let sent = unsafe { libc::kill(supervisor.child.id() as libc::pid_t, libc::SIGTERM) };
    assert_eq!(sent, 0, "signal owned supervisor");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !supervisor.exited().expect("observe cancelled supervisor") {
        assert!(Instant::now() < deadline, "bounded cancellation cleanup");
        thread::sleep(Duration::from_millis(10));
    }
    let status = supervisor.finish().expect("reap owned supervisor");
    assert!(status.success(), "isolated cancellation assertion failed");
    assert!(!runtime.exists(), "private directory/socket left behind");
    assert_eq!(
        fs::read_to_string(output.join("exit-code.txt")).expect("recorded cancellation"),
        format!("{}\n", 128 + libc::SIGTERM)
    );
    assert!(
        !output.join("report.json").exists(),
        "cancellation reported a pass"
    );
    let pid = worker["pid"].as_u64().expect("owned worker pid");
    let same_worker_exists = fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')?
                .1
                .split_whitespace()
                .nth(19)
                .map(str::to_owned)
        })
        .is_some_and(|starttime| Some(starttime.as_str()) == worker["starttime"].as_str());
    assert!(!same_worker_exists, "owned worker was not reaped");
}
