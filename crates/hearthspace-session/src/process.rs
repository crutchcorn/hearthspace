use std::{
    io::{self, Read, Write},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use nix::{sys::signal, unistd::Pid};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub(crate) struct ProcessSpec {
    pub(crate) name: &'static str,
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) envs: Vec<(String, String)>,
    pub(crate) env_removals: Vec<String>,
    pub(crate) current_dir: Option<PathBuf>,
}

pub(crate) struct ManagedProcess {
    spec: ProcessSpec,
    child: Child,
    output_activity: Arc<Mutex<Instant>>,
    pub(crate) restarts: usize,
    pub(crate) max_restarts: usize,
}

#[derive(Debug, Clone, Copy)]
enum ChildOutputStream {
    Stdout,
    Stderr,
}

impl ManagedProcess {
    pub(crate) fn spawn(
        spec: ProcessSpec,
        max_restarts: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let output_activity = Arc::new(Mutex::new(Instant::now()));
        let child = spawn_child(&spec, Arc::clone(&output_activity))?;
        Ok(Self {
            spec,
            child,
            output_activity,
            restarts: 0,
            max_restarts,
        })
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    pub(crate) fn restart(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        kill_group(self);
        self.restarts += 1;
        record_output_activity(&self.output_activity);
        self.child = spawn_child(&self.spec, Arc::clone(&self.output_activity))?;
        Ok(())
    }
}

fn spawn_child(
    spec: &ProcessSpec,
    output_activity: Arc<Mutex<Instant>>,
) -> Result<Child, Box<dyn std::error::Error>> {
    info!(name = spec.name, program = %spec.program, args = ?spec.args, "starting process");
    let mut command = Command::new(&spec.program);
    for key in &spec.env_removals {
        command.env_remove(key);
    }
    command
        .args(&spec.args)
        .envs(spec.envs.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A separate process group still inherits the login's controlling terminal.
    // An application's interactive shell probe can then send SIGTTIN to that
    // entire background group, stopping the compositor and every client in it.
    // A new POSIX session removes the terminal while retaining PID == PGID for
    // our existing group cleanup; logind/cgroup session ownership is unchanged.
    // SAFETY: setsid is async-signal-safe, and conversion of its errno does not
    // allocate or acquire locks between fork and exec.
    unsafe {
        command.pre_exec(|| nix::unistd::setsid().map(|_| ()).map_err(io::Error::from));
    }
    if let Some(current_dir) = &spec.current_dir {
        command.current_dir(current_dir);
    }
    let mut child = command.spawn()?;
    if let Some(stdout) = child.stdout.take() {
        forward_child_output(
            spec.name,
            ChildOutputStream::Stdout,
            Box::new(stdout),
            Arc::clone(&output_activity),
        );
    }
    if let Some(stderr) = child.stderr.take() {
        forward_child_output(
            spec.name,
            ChildOutputStream::Stderr,
            Box::new(stderr),
            output_activity,
        );
    }
    Ok(child)
}

fn forward_child_output(
    process_name: &'static str,
    stream: ChildOutputStream,
    mut reader: Box<dyn Read + Send>,
    output_activity: Arc<Mutex<Instant>>,
) {
    thread::spawn(move || {
        let mut buffer = [0; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return,
                Ok(read) => {
                    record_output_activity(&output_activity);
                    if let Err(error) = write_child_output(stream, &buffer[..read]) {
                        warn!(process_name, ?stream, %error, "failed to forward child output");
                        return;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    warn!(process_name, ?stream, %error, "failed to read child output");
                    return;
                }
            }
        }
    });
}

fn write_child_output(stream: ChildOutputStream, bytes: &[u8]) -> io::Result<()> {
    match stream {
        ChildOutputStream::Stdout => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()
        }
        ChildOutputStream::Stderr => {
            let mut stderr = io::stderr().lock();
            stderr.write_all(bytes)?;
            stderr.flush()
        }
    }
}

fn record_output_activity(output_activity: &Arc<Mutex<Instant>>) {
    match output_activity.lock() {
        Ok(mut activity) => *activity = Instant::now(),
        Err(error) => *error.into_inner() = Instant::now(),
    }
}

pub(crate) fn poll_process(
    process: &mut Option<ManagedProcess>,
) -> Result<Option<ExitStatus>, Box<dyn std::error::Error>> {
    let Some(process) = process else {
        return Ok(None);
    };
    Ok(process.child.try_wait()?)
}

pub(crate) fn terminate_process(process: &mut Option<ManagedProcess>) {
    let Some(process) = process else {
        return;
    };
    let Ok(raw_pid) = i32::try_from(process.child.id()) else {
        warn!(
            name = process.spec.name,
            pid = process.child.id(),
            "cannot signal process with oversized pid"
        );
        return;
    };
    match signal::killpg(Pid::from_raw(raw_pid), signal::Signal::SIGTERM) {
        Ok(()) => info!(name = process.spec.name, pid = raw_pid, "sent SIGTERM"),
        Err(error) => {
            warn!(name = process.spec.name, pid = raw_pid, %error, "failed to send SIGTERM")
        }
    }
}

pub(crate) fn wait_then_kill_process(process: &mut Option<ManagedProcess>) {
    wait_then_kill_process_with_timeout(process, Duration::from_secs(3));
}

pub(crate) fn wait_then_kill_process_with_timeout(
    process: &mut Option<ManagedProcess>,
    timeout: Duration,
) {
    let Some(process) = process else {
        return;
    };
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                info!(name = process.spec.name, %status, "process exited during shutdown");
                // The leader can exit while grandchildren retain resources.
                kill_group(process);
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                warn!(name = process.spec.name, %error, "failed to poll process during shutdown");
                return;
            }
        }
    }

    kill_group(process);
    match process.child.kill() {
        Ok(()) => info!(
            name = process.spec.name,
            "killed process after shutdown timeout"
        ),
        Err(error) => {
            warn!(name = process.spec.name, %error, "failed to kill process after shutdown timeout")
        }
    }
    let _ = process.child.wait();
}

fn kill_group(process: &ManagedProcess) {
    if let Ok(pid) = i32::try_from(process.child.id()) {
        let _ = signal::killpg(Pid::from_raw(pid), signal::Signal::SIGKILL);
    }
}

#[cfg(test)]
mod tests;
