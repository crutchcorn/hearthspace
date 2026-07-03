use std::{
    env, fs,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{self, Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use clap::{ArgAction, Parser};
use nix::{sys::signal, unistd::Pid};
use tracing::{info, warn};

#[derive(Debug, Parser)]
#[command(about = "Start a Hearthspace desktop session")]
struct Args {
    /// Run child processes through `cargo run --bin ...`.
    #[arg(long, conflicts_with = "installed")]
    dev: bool,

    /// Run installed child binaries directly.
    #[arg(long, conflicts_with = "dev")]
    installed: bool,

    /// Wayland display socket name.
    #[arg(long)]
    wayland_display: Option<String>,

    /// Start development watchers for Hearthspace apps.
    #[arg(long, conflicts_with = "no_watch_apps")]
    watch_apps: bool,

    /// Do not start development watchers.
    #[arg(long, conflicts_with = "watch_apps")]
    no_watch_apps: bool,

    /// Do not restart shell if it exits.
    #[arg(long)]
    no_shell_restart: bool,

    /// Do not restart settingsd if it exits.
    #[arg(long)]
    no_settingsd_restart: bool,

    /// Set log level for session and children.
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Print the generated session environment.
    #[arg(long)]
    print_env: bool,

    /// Print what would be started without starting processes.
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunMode {
    DevCargo,
    InstalledBinaries,
}

#[derive(Debug)]
struct SessionPaths {
    runtime_dir: PathBuf,
    session_dir: PathBuf,
    applications_dir: PathBuf,
    settings_socket: PathBuf,
    session_socket: PathBuf,
    shell_command_socket: PathBuf,
}

#[derive(Debug)]
struct SessionEnv {
    wayland_display: String,
    vars: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct ProcessSpec {
    name: &'static str,
    program: String,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    current_dir: Option<PathBuf>,
}

struct ManagedProcess {
    spec: ProcessSpec,
    child: Child,
    restarts: usize,
    max_restarts: usize,
}

struct Supervisor {
    mode: RunMode,
    paths: SessionPaths,
    session_env: SessionEnv,
    shutdown_requested: Arc<AtomicBool>,
    settingsd: Option<ManagedProcess>,
    compositor: Option<ManagedProcess>,
    shell: Option<ManagedProcess>,
    watcher: Option<ManagedProcess>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    initialize_tracing(&args.log_level);

    let mode = resolve_mode(&args);
    let paths = create_session_paths()?;
    let session_env = create_session_env(&args, mode, &paths)?;

    if mode == RunMode::DevCargo {
        generate_dev_desktop_entries(&paths)?;
    }

    info!(?mode, session_dir = %paths.session_dir.display(), "prepared Hearthspace session");
    info!(wayland_display = %session_env.wayland_display, "prepared Wayland display");

    if args.print_env || args.dry_run {
        for (key, value) in &session_env.vars {
            println!("{key}={value}");
        }
    }

    if args.dry_run {
        return Ok(());
    }

    let shutdown_requested = install_signal_handler()?;
    let mut supervisor = Supervisor::new(mode, paths, session_env, shutdown_requested);

    supervisor.start_settingsd(if args.no_settingsd_restart { 0 } else { 3 })?;
    supervisor.wait_for_settingsd_ready()?;
    supervisor.start_compositor()?;
    supervisor.wait_for_compositor_ready()?;
    supervisor.start_shell(if args.no_shell_restart { 0 } else { 3 })?;
    supervisor.start_app_watcher_if_available(&args)?;

    let monitor_result = supervisor.monitor_until_exit();
    supervisor.shutdown();
    monitor_result
}

fn initialize_tracing(log_level: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(log_level));

    match env_filter {
        Ok(env_filter) => tracing_subscriber::fmt().with_env_filter(env_filter).init(),
        Err(_) => tracing_subscriber::fmt().init(),
    }
}

fn resolve_mode(args: &Args) -> RunMode {
    if args.dev {
        return RunMode::DevCargo;
    }
    if args.installed {
        return RunMode::InstalledBinaries;
    }

    if cfg!(debug_assertions) {
        RunMode::DevCargo
    } else {
        RunMode::InstalledBinaries
    }
}

fn create_session_paths() -> Result<SessionPaths, Box<dyn std::error::Error>> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let runtime_dir = PathBuf::from(runtime_dir);
    let session_dir = runtime_dir
        .join("hearthspace")
        .join(format!("session-{}", process::id()));
    let applications_dir = session_dir.join("share/applications");

    fs::create_dir_all(&applications_dir)?;

    Ok(SessionPaths {
        runtime_dir,
        settings_socket: session_dir.join("settings.sock"),
        session_socket: session_dir.join("session.sock"),
        shell_command_socket: session_dir.join("shell-command.sock"),
        session_dir,
        applications_dir,
    })
}

fn create_session_env(
    args: &Args,
    mode: RunMode,
    paths: &SessionPaths,
) -> Result<SessionEnv, Box<dyn std::error::Error>> {
    let wayland_display = args
        .wayland_display
        .clone()
        .unwrap_or_else(|| format!("hearthspace-dev-{}", process::id()));
    let session_dir = path_to_string(&paths.session_dir)?;
    let settings_socket = path_to_string(&paths.settings_socket)?;
    let session_socket = path_to_string(&paths.session_socket)?;
    let shell_command_socket = path_to_string(&paths.shell_command_socket)?;
    let mut vars = vec![
        ("WAYLAND_DISPLAY".to_string(), wayland_display.clone()),
        (
            "XDG_CURRENT_DESKTOP".to_string(),
            "Hearthspace".to_string(),
        ),
        ("DESKTOP_SESSION".to_string(), "hearthspace".to_string()),
        ("HEARTHSPACE_SESSION_DIR".to_string(), session_dir.clone()),
        (
            "HEARTHSPACE_SETTINGS_SOCKET".to_string(),
            settings_socket,
        ),
        ("HEARTHSPACE_SESSION_SOCKET".to_string(), session_socket),
        (
            hearthspace::config::SHELL_COMMAND_SOCKET_ENV.to_string(),
            shell_command_socket,
        ),
        ("RUST_LOG".to_string(), args.log_level.clone()),
        (
            "XDG_DATA_DIRS".to_string(),
            xdg_data_dirs(&path_to_string(&paths.session_dir.join("share"))?)?,
        ),
    ];

    if mode == RunMode::DevCargo {
        vars.push(("HEARTHSPACE_DEV".to_string(), "1".to_string()));
        vars.push((
            "HEARTHSPACE_REPO_ROOT".to_string(),
            env!("CARGO_MANIFEST_DIR").to_string(),
        ));
    }

    Ok(SessionEnv {
        wayland_display,
        vars,
    })
}

fn path_to_string(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(path
        .to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))?
        .to_string())
}

fn xdg_data_dirs(session_share_dir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut dirs = vec![PathBuf::from(session_share_dir)];
    if let Some(existing) = env::var_os("XDG_DATA_DIRS") {
        dirs.extend(env::split_paths(&existing));
    } else {
        dirs.push(PathBuf::from("/usr/local/share"));
        dirs.push(PathBuf::from("/usr/share"));
    }
    Ok(env::join_paths(dirs)?.to_string_lossy().into_owned())
}

fn generate_dev_desktop_entries(paths: &SessionPaths) -> Result<(), Box<dyn std::error::Error>> {
    let exec = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/dev-run-app");
    let desktop_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Hearthspace Settings\n\
         Comment=Configure Hearthspace\n\
         Exec=\"{}\" hearthspace-settings\n\
         Icon=hearthspace\n\
         Categories=Settings;System;\n\
         Keywords=hearthspace;settings;config;preferences;\n\
         OnlyShowIn=Hearthspace;\n\
         X-Hearthspace-Dev=true\n",
        escape_exec_path(&exec)
    );
    let path = paths
        .applications_dir
        .join("dev.hearthspace.Settings.desktop");
    fs::write(&path, desktop_entry)?;
    info!(path = %path.display(), "generated development desktop entry");
    Ok(())
}

fn escape_exec_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn install_signal_handler() -> Result<Arc<AtomicBool>, Box<dyn std::error::Error>> {
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let signal_shutdown = Arc::clone(&shutdown_requested);
    ctrlc::set_handler(move || {
        signal_shutdown.store(true, Ordering::SeqCst);
    })?;
    Ok(shutdown_requested)
}

impl Supervisor {
    fn new(
        mode: RunMode,
        paths: SessionPaths,
        session_env: SessionEnv,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            mode,
            paths,
            session_env,
            shutdown_requested,
            settingsd: None,
            compositor: None,
            shell: None,
            watcher: None,
        }
    }

    fn start_settingsd(&mut self, max_restarts: usize) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-settingsd", &[], true);
        self.settingsd = Some(ManagedProcess::spawn(spec, max_restarts)?);
        Ok(())
    }

    fn start_compositor(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec(
            "hearthspace-compositor",
            &[hearthspace::config::NO_SHELL_FLAG],
            false,
        );
        self.compositor = Some(ManagedProcess::spawn(spec, 0)?);
        Ok(())
    }

    fn start_shell(&mut self, max_restarts: usize) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-shell", &[], true);
        self.shell = Some(ManagedProcess::spawn(spec, max_restarts)?);
        Ok(())
    }

    fn start_app_watcher_if_available(
        &mut self,
        args: &Args,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.mode != RunMode::DevCargo || args.no_watch_apps {
            return Ok(());
        }

        let Some(spec) = self.watcher_spec() else {
            if args.watch_apps {
                warn!("app watcher requested, but neither cargo-watch nor watchexec was found");
            } else {
                warn!("cargo-watch/watchexec not found; Hearthspace Settings will build on first launch");
            }
            return Ok(());
        };

        self.watcher = Some(ManagedProcess::spawn(spec, 0)?);
        Ok(())
    }

    fn wait_for_settingsd_ready(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!(socket = %self.paths.settings_socket.display(), "waiting for settingsd readiness");
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if let Some(status) = poll_process(&mut self.settingsd)? {
                return Err(format!("settingsd exited before readiness: {status}").into());
            }
            if settingsd_ping(&self.paths.settings_socket).is_ok() {
                info!("settingsd ready");
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err("timed out waiting for settingsd readiness".into())
    }

    fn wait_for_compositor_ready(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let socket = self.paths.runtime_dir.join(&self.session_env.wayland_display);
        info!(socket = %socket.display(), "waiting for compositor readiness");
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if let Some(status) = poll_process(&mut self.compositor)? {
                return Err(format!("compositor exited before readiness: {status}").into());
            }
            if socket.exists() {
                info!("compositor ready");
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err("timed out waiting for compositor readiness".into())
    }

    fn monitor_until_exit(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("monitoring Hearthspace session processes");
        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) {
                info!("shutdown requested");
                return Ok(());
            }

            if let Some(status) = poll_process(&mut self.compositor)? {
                info!(%status, "compositor exited; ending session");
                return Ok(());
            }

            if let Some(status) = poll_process(&mut self.settingsd)? {
                if self.restart_process("settingsd", status)? {
                    self.wait_for_settingsd_ready()?;
                } else {
                    return Err(format!("settingsd exited: {status}").into());
                }
            }

            if let Some(status) = poll_process(&mut self.shell)?
                && !self.restart_process("shell", status)?
            {
                return Err(format!("shell exited: {status}").into());
            }

            if let Some(status) = poll_process(&mut self.watcher)? {
                warn!(%status, "app watcher exited; continuing session");
                self.watcher = None;
            }

            thread::sleep(Duration::from_millis(100));
        }
    }

    fn shutdown(&mut self) {
        info!("shutting down Hearthspace session processes");
        terminate_process(&mut self.shell);
        terminate_process(&mut self.compositor);
        terminate_process(&mut self.settingsd);
        terminate_process(&mut self.watcher);

        wait_then_kill_process(&mut self.shell);
        wait_then_kill_process(&mut self.compositor);
        wait_then_kill_process(&mut self.settingsd);
        wait_then_kill_process(&mut self.watcher);

        match fs::remove_dir_all(&self.paths.session_dir) {
            Ok(()) => info!(path = %self.paths.session_dir.display(), "removed session runtime directory"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => warn!(path = %self.paths.session_dir.display(), %error, "failed to remove session runtime directory"),
        }
    }

    fn restart_process(
        &mut self,
        name: &'static str,
        status: ExitStatus,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let process = match name {
            "settingsd" => &mut self.settingsd,
            "shell" => &mut self.shell,
            _ => return Ok(false),
        };
        let Some(process) = process else {
            return Ok(false);
        };
        if process.restarts >= process.max_restarts {
            return Ok(false);
        }
        warn!(name, %status, restart = process.restarts + 1, max_restarts = process.max_restarts, "restarting process");
        process.restart()?;
        Ok(true)
    }

    fn binary_spec(
        &self,
        binary: &'static str,
        binary_args: &[&str],
        include_client_wayland: bool,
    ) -> ProcessSpec {
        let mut envs = self.child_env(include_client_wayland);
        if !include_client_wayland {
            envs.push((
                hearthspace::config::HEARTHSPACE_WAYLAND_DISPLAY_ENV.to_string(),
                self.session_env.wayland_display.clone(),
            ));
        }

        let current_dir = (self.mode == RunMode::DevCargo)
            .then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        match self.mode {
            RunMode::DevCargo => {
                let mut args = vec!["run".to_string(), "--bin".to_string(), binary.to_string()];
                args.push("--".to_string());
                args.extend(binary_args.iter().map(|arg| (*arg).to_string()));
                ProcessSpec {
                    name: binary,
                    program: "cargo".to_string(),
                    args,
                    envs,
                    current_dir,
                }
            }
            RunMode::InstalledBinaries => ProcessSpec {
                name: binary,
                program: binary.to_string(),
                args: binary_args.iter().map(|arg| (*arg).to_string()).collect(),
                envs,
                current_dir,
            },
        }
    }

    fn child_env(&self, include_client_wayland: bool) -> Vec<(String, String)> {
        self.session_env
            .vars
            .iter()
            .filter(|(key, _)| include_client_wayland || key != hearthspace::config::WAYLAND_DISPLAY_ENV)
            .cloned()
            .collect()
    }

    fn watcher_spec(&self) -> Option<ProcessSpec> {
        let envs = self.child_env(true);
        let current_dir = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        if which::which("cargo-watch").is_ok() {
            return Some(ProcessSpec {
                name: "settings-watch",
                program: "cargo".to_string(),
                args: vec![
                    "watch".to_string(),
                    "-w".to_string(),
                    "src/bin/hearthspace-settings.rs".to_string(),
                    "-w".to_string(),
                    "src/settings".to_string(),
                    "-x".to_string(),
                    "build --bin hearthspace-settings".to_string(),
                ],
                envs,
                current_dir,
            });
        }
        if which::which("watchexec").is_ok() {
            return Some(ProcessSpec {
                name: "settings-watch",
                program: "watchexec".to_string(),
                args: vec![
                    "-w".to_string(),
                    "src/bin/hearthspace-settings.rs".to_string(),
                    "-w".to_string(),
                    "src/settings".to_string(),
                    "--".to_string(),
                    "cargo".to_string(),
                    "build".to_string(),
                    "--bin".to_string(),
                    "hearthspace-settings".to_string(),
                ],
                envs,
                current_dir,
            });
        }
        None
    }
}

impl ManagedProcess {
    fn spawn(spec: ProcessSpec, max_restarts: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let child = spawn_child(&spec)?;
        Ok(Self {
            spec,
            child,
            restarts: 0,
            max_restarts,
        })
    }

    fn restart(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.restarts += 1;
        self.child = spawn_child(&self.spec)?;
        Ok(())
    }
}

fn spawn_child(spec: &ProcessSpec) -> Result<Child, Box<dyn std::error::Error>> {
    info!(name = spec.name, program = %spec.program, args = ?spec.args, "starting process");
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(spec.envs.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(current_dir) = &spec.current_dir {
        command.current_dir(current_dir);
    }
    Ok(command.spawn()?)
}

fn poll_process(
    process: &mut Option<ManagedProcess>,
) -> Result<Option<ExitStatus>, Box<dyn std::error::Error>> {
    let Some(process) = process else {
        return Ok(None);
    };
    Ok(process.child.try_wait()?)
}

fn settingsd_ping(socket_path: &Path) -> std::io::Result<()> {
    if !socket_path.exists() {
        return Err(std::io::ErrorKind::NotFound.into());
    }
    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(b"get\n")?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.starts_with("ok ") {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "unexpected settingsd response: {response:?}"
        )))
    }
}

fn terminate_process(process: &mut Option<ManagedProcess>) {
    let Some(process) = process else {
        return;
    };
    if matches!(process.child.try_wait(), Ok(Some(_))) {
        return;
    }
    let Ok(raw_pid) = i32::try_from(process.child.id()) else {
        warn!(name = process.spec.name, pid = process.child.id(), "cannot signal process with oversized pid");
        return;
    };
    match signal::kill(Pid::from_raw(raw_pid), signal::Signal::SIGTERM) {
        Ok(()) => info!(name = process.spec.name, pid = raw_pid, "sent SIGTERM"),
        Err(error) => warn!(name = process.spec.name, pid = raw_pid, %error, "failed to send SIGTERM"),
    }
}

fn wait_then_kill_process(process: &mut Option<ManagedProcess>) {
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
        Ok(()) => info!(name = process.spec.name, "killed process after shutdown timeout"),
        Err(error) => warn!(name = process.spec.name, %error, "failed to kill process after shutdown timeout"),
    }
    let _ = process.child.wait();
}
