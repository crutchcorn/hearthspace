//! Prepare a required development helper before it can own native resources.
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader},
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};

pub(crate) fn prepare_helper(
    binary: &str,
    repository: &Path,
    runtime: &Path,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<PathBuf> {
    crate::readiness::remaining(deadline, cancelled)?;
    let selection = super::selection(binary)?;
    // A clean Cargo build can emit far more than the small service-command
    // output limit. Spool JSON to the private runtime, then read it line by line.
    let log_path = runtime.join(format!("{binary}-build.jsonl"));
    let log = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&log_path)?;
    let mut build = Build(
        Command::new("cargo")
            .arg("build")
            .args(selection)
            .arg("--message-format=json-render-diagnostics")
            .current_dir(repository)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(Stdio::inherit())
            .process_group(0)
            .spawn()?,
    );
    loop {
        if let Some(status) = build.0.try_wait()? {
            if !status.success() {
                return Err(io::Error::other(format!(
                    "required helper {binary} build failed: {status}"
                )));
            }
            break;
        }
        thread::sleep(
            crate::readiness::remaining(deadline, cancelled)?.min(Duration::from_millis(20)),
        );
    }
    let mut executables = BTreeSet::new();
    for line in BufReader::new(File::open(log_path)?).lines() {
        crate::readiness::remaining(deadline, cancelled)?;
        let line = line?;
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue; // Procedural macros may write non-JSON stdout.
        };
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == binary
            && message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
            && message["profile"]["test"] != true
            && let Some(path) = message["executable"].as_str()
        {
            executables.insert(PathBuf::from(path));
        }
    }
    if executables.len() != 1 {
        return Err(io::Error::other(format!(
            "Cargo did not report one executable for required helper {binary}"
        )));
    }
    let executable = executables.pop_first().unwrap();
    if !executable.is_absolute()
        || !fs::metadata(&executable)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    {
        return Err(io::Error::other(format!(
            "required helper artifact is unavailable: {}",
            executable.display()
        )));
    }
    Ok(executable)
}

struct Build(Child);

impl Drop for Build {
    fn drop(&mut self) {
        // A cancellation must not leave cargo/rustc children compiling after
        // failed startup or let them race the caller's runtime cleanup.
        if let Ok(pid) = i32::try_from(self.0.id()) {
            let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
