//! One supervisor, one worker, one owned Xwayland. No shared X sockets or env.
use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io::Write,
    os::{
        fd::AsRawFd,
        unix::{
            fs::DirBuilderExt,
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn environment(command: &mut Command, runtime: &Path) {
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", runtime)
        .env("XDG_RUNTIME_DIR", runtime)
        .env("XDG_CONFIG_HOME", runtime.join("config"))
        .env("XDG_CACHE_HOME", runtime.join("cache"))
        .env("TMPDIR", runtime)
        .env("LC_ALL", "C");
}

pub fn supervise(opts: &crate::Options) -> Result<i32> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--worker").arg("--xwayland").arg(&opts.binary);
    if opts.targets {
        command.arg("--surface-input-targets");
    }
    supervise_command(opts, command)
}

fn supervise_command(opts: &crate::Options, mut command: Command) -> Result<i32> {
    // Install before creating the private runtime. Handlers only record the
    // request; this thread retains all process and filesystem ownership.
    let cancellation = crate::cancellation::Cancellation::install()?;
    let artifacts = opts.output.as_ref().context("--output is required")?;
    // Fail if the caller's evidence path already exists; never reuse evidence.
    fs::DirBuilder::new().mode(0o700).create(artifacts)?;
    let artifacts = artifacts.canonicalize()?;
    let runtime = tempfile::Builder::new()
        .prefix("private-")
        .tempdir_in(&artifacts)?;
    fs::create_dir(runtime.path().join("config"))?;
    fs::create_dir(runtime.path().join("cache"))?;
    fs::write(
        artifacts.join("invocation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "selector": crate::SELECTOR, "binary": opts.binary,
            "surface_input_targets": opts.targets, "deadline_seconds": 60,
            "runtime": runtime.path(), "x11_transport": "private relative-path socket via -listenfd",
            "wayland_transport": "inherited socketpair", "host_environment": false
        }))?,
    )?;
    println!("ARTIFACTS={}", artifacts.display());
    std::io::stdout().flush()?;
    command
        .arg("--artifacts")
        .arg(&artifacts)
        .current_dir(runtime.path())
        .stdin(Stdio::null())
        .stdout(File::create(artifacts.join("worker.log"))?)
        .stderr(File::create(artifacts.join("worker-errors.log"))?);
    environment(&mut command, runtime.path());
    let parent = std::process::id() as libc::pid_t;
    // Only this not-yet-execed owned child changes process/resource settings.
    unsafe {
        command.pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            child_lifetime(parent)?;
            let limit = libc::rlimit {
                rlim_cur: 16 * 1024 * 1024,
                rlim_max: 16 * 1024 * 1024,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut worker = Group {
        child: command.spawn()?,
        reaped: false,
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    let code = loop {
        if let Some(code) = cancellation.exit_code() {
            worker.finish()?;
            break code;
        }
        if worker.exited()? {
            break worker.finish()?.code().unwrap_or(2);
        }
        if Instant::now() >= deadline {
            worker.finish()?;
            break 124;
        }
        thread::sleep(Duration::from_millis(20));
    };
    // Retain bounded evidence, remove only this invocation's private runtime.
    runtime.close()?;
    let code = cancellation.exit_code().unwrap_or(code);
    fs::write(artifacts.join("exit-code.txt"), format!("{code}\n"))?;
    println!("{}: exit {code}; {}", crate::SELECTOR, artifacts.display());
    Ok(code)
}

struct Group {
    child: Child,
    reaped: bool,
}

impl Group {
    fn exited(&self) -> std::io::Result<bool> {
        // Observe without reaping, so the pgid cannot be recycled before cleanup.
        unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            if libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(info.si_pid() != 0)
        }
    }

    fn finish(&mut self) -> std::io::Result<std::process::ExitStatus> {
        // Kill only this owned group, while its leader remains unreaped.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish();
        }
    }
}

fn child_lifetime(parent: libc::pid_t) -> std::io::Result<()> {
    // Called only from pre_exec, using async-signal-safe libc operations.
    unsafe {
        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::getppid() != parent {
            libc::_exit(125);
        }
    }
    Ok(())
}

pub struct Xwayland {
    child: Option<Child>,
    pub wayland: Option<UnixStream>,
    pub wm: Option<UnixStream>,
}

impl Xwayland {
    pub fn spawn(binary: &Path, targets: bool, artifacts: &Path) -> Result<Self> {
        // Relative address avoids sockaddr_un's length limit in the long ART path.
        // This process runs in the supervisor-created 0700 private directory.
        let listener = UnixListener::bind("x11")?;
        let (wl_server, wl_child) = UnixStream::pair()?;
        let (wm_server, wm_child) = UnixStream::pair()?;
        let fds = [
            listener.as_raw_fd(),
            wl_child.as_raw_fd(),
            wm_child.as_raw_fd(),
        ];
        let mut command = Command::new(binary);
        // -listenfd sets NoListenAll in Xwayland: no /tmp/.X11-unix socket,
        // abstract listener, TCP listener, or shared display-number lock.
        command
            .args([
                ":0",
                "-rootless",
                "-shm",
                "-noreset",
                "-ac",
                "-nolisten",
                "tcp",
            ])
            .arg("-listenfd")
            .arg(fds[0].to_string())
            .arg("-wm")
            .arg(fds[2].to_string())
            .stdin(Stdio::null())
            .stdout(File::create(artifacts.join("xwayland-stdout.log"))?)
            .stderr(File::create(artifacts.join("xwayland.log"))?);
        if targets {
            command.arg("-surface-input-targets");
        }
        environment(&mut command, &std::env::current_dir()?);
        command
            .env("WAYLAND_SOCKET", fds[1].to_string())
            .env("WAYLAND_DEBUG", "client");
        let parent = std::process::id() as libc::pid_t;
        unsafe {
            command.pre_exec(move || {
                child_lifetime(parent)?;
                for fd in fds {
                    if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let child = command
            .spawn()
            .context("spawn explicit isolated Xwayland")?;
        Ok(Self {
            child: Some(child),
            wayland: Some(wl_server),
            wm: Some(wm_server),
        })
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
        }
        Ok(())
    }
}

impl Drop for Xwayland {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests;
