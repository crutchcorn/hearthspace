//! Adopt a D-Bus-activated helper only after proving it belongs to this runtime.
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};
use std::{
    fs, io,
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub(super) struct OwnedPid {
    pid: u32,
    start_time: String,
}

impl OwnedPid {
    pub(super) fn adopt(pid: u32, program: &Path, runtime: &Path, bus: &str) -> io::Result<Self> {
        let proc = Path::new("/proc").join(pid.to_string());
        let executable = fs::read_link(proc.join("exe"))?;
        let env = fs::read(proc.join("environ"))?;
        if executable != fs::canonicalize(program)? || !environment_matches(&env, runtime, bus) {
            return Err(io::Error::other(format!(
                "refusing to adopt foreign desktop service PID {pid}"
            )));
        }
        let start_time = start_time(&fs::read_to_string(proc.join("stat"))?)
            .ok_or_else(|| io::Error::other("desktop service already exited"))?
            .to_string();
        Ok(Self { pid, start_time })
    }

    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    pub(super) fn running(&self) -> bool {
        fs::read_to_string(format!("/proc/{}/stat", self.pid))
            .is_ok_and(|stat| start_time(&stat) == Some(self.start_time.as_str()))
    }

    pub(super) fn signal(&self, signal: Signal) {
        if self.running()
            && let Ok(pid) = i32::try_from(self.pid)
        {
            let _ = signal::kill(Pid::from_raw(pid), signal);
        }
    }

    pub(super) fn wait_then_kill(&self) {
        let deadline = Instant::now() + Duration::from_millis(500);
        while self.running() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        self.signal(Signal::SIGKILL);
    }
}

fn start_time(stat: &str) -> Option<&str> {
    let (_, fields) = stat.rsplit_once(") ")?;
    let mut fields = fields.split_whitespace();
    if fields.next()? == "Z" {
        return None;
    }
    fields.nth(18)
}

fn environment_matches(environment: &[u8], runtime: &Path, bus: &str) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let value = |key: &[u8]| {
        environment
            .split(|byte| *byte == 0)
            .find_map(|item| item.strip_prefix(key))
    };
    value(b"XDG_RUNTIME_DIR=") == Some(runtime.as_os_str().as_bytes())
        && value(b"DBUS_SESSION_BUS_ADDRESS=") == Some(bus.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adoption_requires_both_private_runtime_and_bus() {
        let env = b"XDG_RUNTIME_DIR=/private\0DBUS_SESSION_BUS_ADDRESS=unix:path=/private/bus\0";
        assert!(environment_matches(
            env,
            Path::new("/private"),
            "unix:path=/private/bus"
        ));
        assert!(!environment_matches(
            env,
            Path::new("/host"),
            "unix:path=/private/bus"
        ));
        assert!(!environment_matches(
            env,
            Path::new("/private"),
            "unix:path=/host/bus"
        ));
    }
    #[test]
    fn process_identity_uses_start_time_and_rejects_zombies() {
        let stat = "12 (a name) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 12345 0";
        assert_eq!(start_time(stat), Some("12345"));
        assert_eq!(start_time(&stat.replace(") S", ") Z")), None);
    }
}
