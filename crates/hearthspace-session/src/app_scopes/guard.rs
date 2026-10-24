//! A user-manager service tying app scopes to one live session supervisor.
use std::{fs, io, path::Path, thread, time::Duration};

use crate::identity::{CLOSING_FILE, READY_FILE, process_start_time};

pub fn alive(pid: u32, start_time: &str, runtime: &Path) -> bool {
    runtime.is_dir()
        && !runtime.join(CLOSING_FILE).exists()
        && fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .filter(|stat| {
                !matches!(
                    stat.rsplit_once(')')
                        .and_then(|(_, fields)| fields.split_whitespace().next()),
                    Some("Z" | "X" | "x")
                )
            })
            .and_then(|stat| process_start_time(&stat).map(str::to_owned))
            .is_some_and(|actual| actual == start_time)
}

pub fn run(args: &[String]) -> io::Result<()> {
    let [pid, start_time, runtime] = args else {
        return Err(io::Error::other(
            "expected supervisor PID, start time and runtime",
        ));
    };
    let pid = pid.parse::<u32>().map_err(io::Error::other)?;
    if pid == 0 || start_time.parse::<u64>().is_err() || !Path::new(runtime).is_absolute() {
        return Err(io::Error::other("invalid supervisor identity"));
    }
    if !alive(pid, start_time, Path::new(runtime)) {
        return Err(io::Error::other("session supervisor has already ended"));
    }
    fs::write(
        Path::new(runtime).join(READY_FILE),
        std::process::id().to_string(),
    )?;
    while alive(pid, start_time, Path::new(runtime)) {
        thread::sleep(Duration::from_millis(100));
    }
    let retired = crate::activation_cleanup::retire_from_session_dir(Path::new(runtime), None);
    let _ = fs::remove_file(Path::new(runtime).join(READY_FILE));
    retired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_reused_pid_and_ended_runtime() {
        let pid = std::process::id();
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let start = process_start_time(&stat).unwrap();
        assert!(alive(pid, start, Path::new("/tmp")));
        assert!(!alive(pid, "0", Path::new("/tmp")));
        assert!(!alive(
            pid,
            start,
            Path::new("/nonexistent/hearthspace-runtime")
        ));
    }

    #[test]
    fn closing_marker_prevents_late_service_reactivation() {
        let runtime =
            std::env::temp_dir().join(format!("hearthspace-guard-{}", std::process::id()));
        fs::create_dir(&runtime).unwrap();
        let pid = std::process::id();
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let start = process_start_time(&stat).unwrap();
        fs::write(runtime.join(CLOSING_FILE), "").unwrap();
        assert!(!alive(pid, start, &runtime));
        assert!(run(&[pid.to_string(), start.into(), runtime.display().to_string()]).is_err());
        fs::remove_dir_all(runtime).unwrap();
    }
}
