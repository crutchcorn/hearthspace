use std::{
    io::{self, Read, Write},
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

    pub(crate) fn restart(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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

pub(crate) fn process_output_idle_duration(process: &Option<ManagedProcess>) -> Option<Duration> {
    let process = process.as_ref()?;
    let last_output = match process.output_activity.lock() {
        Ok(activity) => *activity,
        Err(error) => *error.into_inner(),
    };
    Some(Instant::now().duration_since(last_output))
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
    if matches!(process.child.try_wait(), Ok(Some(_))) {
        return;
    }
    let Ok(raw_pid) = i32::try_from(process.child.id()) else {
        warn!(
            name = process.spec.name,
            pid = process.child.id(),
            "cannot signal process with oversized pid"
        );
        return;
    };
    match signal::kill(Pid::from_raw(raw_pid), signal::Signal::SIGTERM) {
        Ok(()) => info!(name = process.spec.name, pid = raw_pid, "sent SIGTERM"),
        Err(error) => {
            warn!(name = process.spec.name, pid = raw_pid, %error, "failed to send SIGTERM")
        }
    }
}

pub(crate) fn wait_then_kill_process(process: &mut Option<ManagedProcess>) {
    let Some(process) = process else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match process.child.try_wait() {
            Ok(Some(status)) => {
                info!(name = process.spec.name, %status, "process exited during shutdown");
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                warn!(name = process.spec.name, %error, "failed to poll process during shutdown");
                return;
            }
        }
    }

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
