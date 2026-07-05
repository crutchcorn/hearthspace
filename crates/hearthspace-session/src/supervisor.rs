use std::{
    fs,
    process::ExitStatus,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use tracing::{info, warn};

use crate::{
    cli::{Args, RunMode},
    compositor::{CompositorBackend, configure_compositor_environment, select_compositor_backend},
    dev_cargo::dev_cargo_args,
    process::{
        ManagedProcess, ProcessSpec, poll_process, process_output_idle_duration, terminate_process,
        wait_then_kill_process,
    },
    session_env::{SessionEnv, SessionPaths, repo_root},
    settingsd::settingsd_ping,
};

const STARTUP_OUTPUT_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) struct Supervisor {
    mode: RunMode,
    paths: SessionPaths,
    session_env: SessionEnv,
    shutdown_requested: Arc<AtomicBool>,
    settingsd: Option<ManagedProcess>,
    compositor: Option<ManagedProcess>,
    shell: Option<ManagedProcess>,
    watcher: Option<ManagedProcess>,
}

impl Supervisor {
    pub(crate) fn new(
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

    pub(crate) fn start_settingsd(
        &mut self,
        max_restarts: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-settingsd", &[], true);
        self.settingsd = Some(ManagedProcess::spawn(spec, max_restarts)?);
        Ok(())
    }

    pub(crate) fn start_compositor(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let backend = select_compositor_backend();
        let backend_flag = match backend {
            CompositorBackend::NativeTty => "--tty",
            CompositorBackend::NestedWinit => "--winit",
        };
        let mut spec = self.binary_spec(
            "hearthspace-compositor",
            &[hearthspace_common::NO_SHELL_FLAG, backend_flag],
            false,
        );
        configure_compositor_environment(&mut spec, backend);
        self.compositor = Some(ManagedProcess::spawn(spec, 0)?);
        Ok(())
    }

    pub(crate) fn start_shell(
        &mut self,
        max_restarts: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-shell", &[], true);
        self.shell = Some(ManagedProcess::spawn(spec, max_restarts)?);
        Ok(())
    }

    pub(crate) fn start_app_watcher_if_available(
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
                warn!(
                    "cargo-watch/watchexec not found; Hearthspace Settings will build on first launch"
                );
            }
            return Ok(());
        };

        self.watcher = Some(ManagedProcess::spawn(spec, 0)?);
        Ok(())
    }

    pub(crate) fn wait_for_settingsd_ready(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!(socket = %self.paths.settings_socket.display(), "waiting for settingsd readiness");
        loop {
            if let Some(status) = poll_process(&mut self.settingsd)? {
                return Err(format!("settingsd exited before readiness: {status}").into());
            }
            if settingsd_ping(&self.paths.settings_socket).is_ok() {
                info!("settingsd ready");
                return Ok(());
            }
            if process_output_idle_duration(&self.settingsd)
                .is_some_and(|idle| idle >= STARTUP_OUTPUT_IDLE_TIMEOUT)
            {
                return Err(format!(
                    "timed out waiting for settingsd readiness after {}s without child output",
                    STARTUP_OUTPUT_IDLE_TIMEOUT.as_secs()
                )
                .into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    pub(crate) fn wait_for_compositor_ready(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let socket = self
            .paths
            .session_dir
            .join(&self.session_env.wayland_display);
        info!(socket = %socket.display(), "waiting for compositor readiness");
        loop {
            if let Some(status) = poll_process(&mut self.compositor)? {
                return Err(format!("compositor exited before readiness: {status}").into());
            }
            if socket.exists() {
                info!("compositor ready");
                return Ok(());
            }
            if process_output_idle_duration(&self.compositor)
                .is_some_and(|idle| idle >= STARTUP_OUTPUT_IDLE_TIMEOUT)
            {
                return Err(format!(
                    "timed out waiting for compositor readiness after {}s without child output",
                    STARTUP_OUTPUT_IDLE_TIMEOUT.as_secs()
                )
                .into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    pub(crate) fn monitor_until_exit(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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

    pub(crate) fn shutdown(&mut self) {
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
            Ok(()) => {
                info!(path = %self.paths.session_dir.display(), "removed session runtime directory")
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                warn!(path = %self.paths.session_dir.display(), %error, "failed to remove session runtime directory")
            }
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
                hearthspace_common::HEARTHSPACE_WAYLAND_DISPLAY_ENV.to_string(),
                self.session_env.wayland_display.clone(),
            ));
        }

        let current_dir = (self.mode == RunMode::DevCargo).then(repo_root);
        match self.mode {
            RunMode::DevCargo => {
                let args = dev_cargo_args(binary, binary_args);
                ProcessSpec {
                    name: binary,
                    program: "cargo".to_string(),
                    args,
                    envs,
                    env_removals: Vec::new(),
                    current_dir,
                }
            }
            RunMode::InstalledBinaries => ProcessSpec {
                name: binary,
                program: binary.to_string(),
                args: binary_args.iter().map(|arg| (*arg).to_string()).collect(),
                envs,
                env_removals: Vec::new(),
                current_dir,
            },
        }
    }

    fn child_env(&self, include_client_wayland: bool) -> Vec<(String, String)> {
        self.session_env
            .vars
            .iter()
            .filter(|(key, _)| {
                include_client_wayland || key != hearthspace_common::WAYLAND_DISPLAY_ENV
            })
            .cloned()
            .collect()
    }

    fn watcher_spec(&self) -> Option<ProcessSpec> {
        let envs = self.child_env(true);
        let current_dir = Some(repo_root());
        if which::which("cargo-watch").is_ok() {
            return Some(ProcessSpec {
                name: "settings-watch",
                program: "cargo".to_string(),
                args: vec![
                    "watch".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-settings".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-ipc".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-common".to_string(),
                    "-x".to_string(),
                    "build -p hearthspace-settings".to_string(),
                ],
                envs,
                env_removals: Vec::new(),
                current_dir,
            });
        }
        if which::which("watchexec").is_ok() {
            return Some(ProcessSpec {
                name: "settings-watch",
                program: "watchexec".to_string(),
                args: vec![
                    "-w".to_string(),
                    "crates/hearthspace-settings".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-ipc".to_string(),
                    "-w".to_string(),
                    "crates/hearthspace-common".to_string(),
                    "--".to_string(),
                    "cargo".to_string(),
                    "build".to_string(),
                    "-p".to_string(),
                    "hearthspace-settings".to_string(),
                ],
                envs,
                env_removals: Vec::new(),
                current_dir,
            });
        }
        None
    }
}
