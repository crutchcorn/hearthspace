use super::*;
use std::{panic::catch_unwind, path::Path, process::Stdio};

fn fixture(path: &Path, tail: &str) -> OwnedProcess {
    OwnedProcess::spawn(
        Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "trap '' TERM; sleep 20 & printf '%s\\n' \"$!\" > \"$1\"; {tail}"
            ))
            .arg("owned-fixture")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .unwrap()
}

fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "fixture condition timed out");
        thread::sleep(POLL);
    }
}

fn descendant(path: &Path) -> Process {
    until(|| fs::read_to_string(path).is_ok_and(|value| value.trim().parse::<u32>().is_ok()));
    let pid = fs::read_to_string(path).unwrap().trim().parse().unwrap();
    read_process(pid)
        .unwrap()
        .expect("fixture descendant started")
}

fn stopped(process: Process) -> bool {
    !read_process(process.pid)
        .unwrap()
        .is_some_and(|current| current.start_ticks == process.start_ticks && current.live())
}

fn assert_stopped(process: Process) {
    assert!(stopped(process), "owned process survived: {process:?}");
}

#[test]
fn cleanup_kills_term_resistant_leader_and_descendant() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("child.pid");
    let mut owned = fixture(&path, "wait");
    let leader = owned.leader().unwrap();
    let child = descendant(&path);
    assert_eq!(child.group, leader.pid);
    assert!(owned.is_running().unwrap());
    owned.stop().unwrap();
    assert_stopped(leader);
    assert_stopped(child);
    assert!(
        read_process(leader.pid).unwrap().is_none(),
        "leader was reaped"
    );
}

#[test]
fn exited_leader_still_owns_descendant_cleanup() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("child.pid");
    let mut owned = fixture(&path, "exit 0");
    let leader = owned.leader().unwrap();
    let child = descendant(&path);
    until(|| !owned.is_running().unwrap());
    assert_eq!(
        owned.leader().unwrap().state,
        b'Z',
        "leader must stay unreaped"
    );
    assert!(read_process(child.pid).unwrap().unwrap().live());
    owned.stop().unwrap();
    assert_stopped(leader);
    assert_stopped(child);
}

#[test]
fn panic_cleanup_preserves_an_unrelated_group() {
    let runtime = tempfile::tempdir().unwrap();
    let mut sentinel = OwnedProcess::spawn(Command::new("/bin/sleep").arg("20")).unwrap();
    let path = runtime.path().join("child.pid");
    let mut identities = None;
    let panic = catch_unwind(std::panic::AssertUnwindSafe(|| {
        let owned = fixture(&path, "wait");
        identities = Some((owned.leader().unwrap(), descendant(&path)));
        panic!("exercise fixture unwinding");
    }));
    assert!(panic.is_err());
    let (leader, child) = identities.unwrap();
    assert_stopped(leader);
    assert_stopped(child);
    assert!(
        sentinel.is_running().unwrap(),
        "cleanup reached an unrelated group"
    );
    sentinel.stop().unwrap();
}

#[test]
fn failed_census_still_kills_owned_group_without_reaping_its_anchor() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("child.pid");
    let mut owned = fixture(&path, "wait");
    let leader = owned.leader().unwrap();
    let child = descendant(&path);
    // Substitute only the external /proc group census. Signal delivery, exact
    // leader verification, process lifetimes, and the final inspection are real.
    let error = owned
        .stop_with_census(&mut |_, _| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture census denied",
            ))
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(error.to_string(), "fixture census denied");
    until(|| stopped(leader) && stopped(child));
    assert_eq!(
        owned.leader().unwrap().state,
        b'Z',
        "unverified cleanup keeps its anchor"
    );
    assert!(owned.exited().unwrap(), "leader remains waitable");
    owned.stop().unwrap();
    assert!(read_process(leader.pid).unwrap().is_none());
}

#[test]
fn term_census_deadline_allows_verified_kill_cleanup() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("child.pid");
    let mut owned = fixture(&path, "wait");
    let leader = owned.leader().unwrap();
    let child = descendant(&path);
    let mut exhausted_term = false;
    let mut complete_scans = 0;
    owned
        .stop_with_census(&mut |group, deadline| {
            if !exhausted_term {
                exhausted_term = true;
                assert!(!stopped(leader));
                assert!(!stopped(child));
                // Exercise the production scanner's deadline boundary without
                // racing a sleep against the TERM grace period.
                let census = group_members(group, Instant::now())?;
                assert!(matches!(&census, Census::DeadlineReached));
                return Ok(census);
            }
            let census = group_members(group, deadline)?;
            if matches!(&census, Census::Complete(_)) {
                complete_scans += 1;
            }
            Ok(census)
        })
        .unwrap();
    assert!(exhausted_term);
    assert!(complete_scans >= 2, "KILL cleanup requires complete scans");
    assert_stopped(leader);
    assert_stopped(child);
    assert!(
        read_process(leader.pid).unwrap().is_none(),
        "leader was reaped"
    );
}

#[test]
fn kill_census_deadline_keeps_unverified_cleanup_failed_and_unreaped() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("child.pid");
    let mut owned = fixture(&path, "wait");
    let leader = owned.leader().unwrap();
    let child = descendant(&path);
    let mut incomplete_scans = 0;
    let error = owned
        .stop_with_census(&mut |group, _| {
            let census = group_members(group, Instant::now())?;
            assert!(matches!(&census, Census::DeadlineReached));
            incomplete_scans += 1;
            Ok(census)
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(incomplete_scans, 2, "both TERM and KILL must be attempted");
    until(|| stopped(leader) && stopped(child));
    assert_eq!(owned.leader().unwrap().state, b'Z', "anchor stays unreaped");
    assert!(owned.exited().unwrap(), "leader remains waitable");
    // Restore inspection before releasing the pinned leader PID.
    owned.stop().unwrap();
    assert!(read_process(leader.pid).unwrap().is_none());
}

#[test]
fn census_io_timeout_remains_an_error_after_verified_kill_cleanup() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("child.pid");
    let mut owned = fixture(&path, "wait");
    let leader = owned.leader().unwrap();
    let child = descendant(&path);
    let mut injected = false;
    let error = owned
        .stop_with_census(&mut |group, deadline| {
            if !injected {
                injected = true;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fixture census I/O timed out",
                ));
            }
            group_members(group, deadline)
        })
        .unwrap_err();
    assert!(injected);
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(error.to_string(), "fixture census I/O timed out");
    assert_stopped(leader);
    assert_stopped(child);
    assert!(
        read_process(leader.pid).unwrap().is_none(),
        "leader was reaped"
    );
}

#[test]
fn zombie_main_thread_does_not_hide_a_live_descendant_worker() {
    let runtime = tempfile::tempdir().unwrap();
    let mut owned = OwnedProcess::spawn(
        Command::new("/bin/sh")
            .args([
                "-c",
                "trap '' TERM; /usr/bin/python3 -I -B -c \"$1\" \"$2\" & exit 0",
            ])
            .arg("threaded-fixture")
            .arg(THREADED_DESCENDANT)
            .arg(runtime.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null()),
    )
    .unwrap();
    let child = descendant(&runtime.path().join("child.pid"));
    let worker_tid: u32 = fs::read_to_string(runtime.path().join("worker.tid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let worker_stat = format!("/proc/{worker_tid}/stat");
    until(|| {
        !owned.is_running().unwrap()
            && read_process(child.pid)
                .unwrap()
                .is_some_and(|process| process.state == b'Z' && process.threads > 1)
    });
    assert!(
        fs::read(&worker_stat).is_ok(),
        "worker exists after its main thread exits"
    );
    owned.stop().unwrap();
    // Independent of Process::live(): the actual worker TID must disappear.
    // With the old zombie-only check, cleanup returns while this worker runs
    // for its remaining 20-second lifetime, failing this bounded observation.
    until(|| match fs::read(&worker_stat) {
        Ok(_) => false,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()) =>
        {
            true
        }
        Err(error) => panic!("cannot inspect owned worker: {error}"),
    });
    assert_stopped(child);
}

const THREADED_DESCENDANT: &str = r#"
import ctypes, os, pathlib, signal, sys, threading, time
root = pathlib.Path(sys.argv[1])
signal.signal(signal.SIGTERM, signal.SIG_IGN)
ready = threading.Event()
deadline = time.monotonic() + 20
def worker():
    (root / 'worker.tid').write_text(str(threading.get_native_id()))
    ready.set()
    while time.monotonic() < deadline:
        time.sleep(0.01)
    os._exit(0)
threading.Thread(target=worker, daemon=True).start()
if not ready.wait(3):
    os._exit(1)
(root / 'child.pid').write_text(str(os.getpid()))
pthread_exit = ctypes.CDLL(None).pthread_exit
pthread_exit.argtypes = [ctypes.c_void_p]
pthread_exit.restype = None
pthread_exit(None)
os._exit(2)
"#;
