use std::{
    fs,
    path::PathBuf,
    process::ExitStatus,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use tracing::{info, warn};

use crate::{
    cli::{Args, Backend, RunMode},
    compositor::{CompositorBackend, configure_compositor_environment, select_compositor_backend},
    process::{ManagedProcess, poll_process, terminate_process, wait_then_kill_process},
    session_env::{SessionEnv, SessionPaths},
    settingsd::settingsd_ping,
};

mod commands;
mod notifications;

pub(crate) struct Supervisor {
    mode: RunMode,
    backend: Option<Backend>,
    binary_dir: Option<PathBuf>,
    compositor_binary: Option<PathBuf>,
    startup_deadline: Instant,
    startup_timeout: Duration,
    paths: SessionPaths,
    session_env: SessionEnv,
    shutdown_requested: Arc<AtomicBool>,
    settingsd: Option<ManagedProcess>,
    compositor: Option<ManagedProcess>,
    shell: Option<ManagedProcess>,
    watcher: Option<ManagedProcess>,
    notifications: Option<ManagedProcess>,
    desktop_services: crate::desktop_services::DesktopServices,
    app_scopes: Option<crate::app_scopes::AppScopes>,
}

impl Supervisor {
    pub(crate) fn new(
        mode: RunMode,
        paths: SessionPaths,
        session_env: SessionEnv,
        shutdown_requested: Arc<AtomicBool>,
        args: &Args,
    ) -> Self {
        Self {
            mode,
            backend: args.backend,
            binary_dir: args.binary_dir.clone(),
            compositor_binary: args.compositor_binary.clone(),
            startup_timeout: Duration::from_secs(args.startup_timeout),
            startup_deadline: Instant::now() + Duration::from_secs(args.startup_timeout),
            paths,
            session_env,
            shutdown_requested,
            settingsd: None,
            compositor: None,
            shell: None,
            watcher: None,
            notifications: None,
            desktop_services: crate::desktop_services::DesktopServices::default(),
            app_scopes: None,
        }
    }

    pub(crate) fn prepare_activation_environment(
        &self,
        private_session_bus: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if private_session_bus {
            crate::session_bus::update_activation_environment(
                &self.session_env,
                self.startup_deadline,
                &self.shutdown_requested,
            )?;
        }
        Ok(())
    }

    pub(crate) fn start_settingsd(
        &mut self,
        max_restarts: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-settingsd", &[], true);
        self.settingsd = Some(ManagedProcess::spawn(spec, max_restarts)?);
        self.record_processes()?;
        Ok(())
    }

    pub(crate) fn start_compositor(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let backend = match self.backend {
            Some(Backend::Native) => CompositorBackend::NativeTty,
            Some(Backend::Nested) => CompositorBackend::NestedWinit,
            None => select_compositor_backend(),
        };
        let backend_flag = match backend {
            CompositorBackend::NativeTty => "--tty",
            CompositorBackend::NestedWinit => "--winit",
        };
        if backend == CompositorBackend::NativeTty
            && self
                .session_env
                .vars
                .iter()
                .any(|(key, _)| key == "HEARTHSPACE_SESSION_BUS_ADDRESS")
        {
            self.app_scopes = crate::app_scopes::AppScopes::start(
                &self.paths,
                self.binary_dir.as_deref(),
                self.startup_deadline,
                &self.shutdown_requested,
            )?;
            if let Some(owner) = &self.app_scopes {
                self.session_env
                    .vars
                    .push((crate::app_scopes::OWNER_ENV.into(), owner.unit().into()));
            }
        }
        let mut spec = self.binary_spec(
            "hearthspace-compositor",
            &[hearthspace_common::NO_SHELL_FLAG, backend_flag],
            false,
        );
        configure_compositor_environment(&mut spec, backend);
        self.compositor = Some(ManagedProcess::spawn(spec, 0)?);
        self.record_processes()?;
        Ok(())
    }

    pub(crate) fn start_shell(
        &mut self,
        max_restarts: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-shell", &[], true);
        self.shell = Some(ManagedProcess::spawn(spec, max_restarts)?);
        self.record_processes()?;
        Ok(())
    }

    pub(crate) fn start_desktop_services(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.desktop_services.start(
            &self.paths,
            &self.session_env,
            self.startup_deadline,
            &self.shutdown_requested,
        )?;
        self.record_processes()?;
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
            self.check_startup_deadline("settingsd")?;
            if let Some(status) = poll_process(&mut self.settingsd)? {
                return Err(format!("settingsd exited before readiness: {status}").into());
            }
            if settingsd_ping(
                &self.paths.settings_socket,
                self.startup_deadline,
                &self.shutdown_requested,
            )
            .is_ok()
            {
                info!("settingsd ready");
                return Ok(());
            }
            self.check_startup_deadline("settingsd")?;
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
            self.check_startup_deadline("compositor")?;
            if let Some(status) = poll_process(&mut self.compositor)? {
                return Err(format!("compositor exited before readiness: {status}").into());
            }
            if crate::readiness::connect_until(
                &socket,
                self.startup_deadline
                    .min(Instant::now() + Duration::from_millis(250)),
                &self.shutdown_requested,
            )
            .is_ok()
            {
                info!("compositor ready");
                return Ok(());
            }
            self.check_startup_deadline("compositor")?;
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
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("compositor failed: {status}").into())
                };
            }

            if let Some(status) = poll_process(&mut self.settingsd)? {
                if self.restart_process("settingsd", status)? {
                    self.startup_deadline = Instant::now() + self.startup_timeout;
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

            if let Some(status) = poll_process(&mut self.notifications)?
                && !self.restart_process("notifications", status)?
            {
                warn!(%status, "notification service exhausted its restart budget");
                self.notifications = None;
                self.record_processes()?;
            }

            if let Some(status) = poll_process(&mut self.watcher)? {
                warn!(%status, "app watcher exited; continuing session");
                self.watcher = None;
            }

            if self.desktop_services.poll(&self.shutdown_requested) {
                self.record_processes()?;
            }

            thread::sleep(Duration::from_millis(100));
        }
    }

    pub(crate) fn shutdown(&mut self) -> std::io::Result<()> {
        info!("shutting down Hearthspace session processes");
        terminate_process(&mut self.shell);
        terminate_process(&mut self.compositor);
        terminate_process(&mut self.settingsd);
        terminate_process(&mut self.watcher);
        terminate_process(&mut self.notifications);

        self.desktop_services.shutdown();

        wait_then_kill_process(&mut self.shell);
        wait_then_kill_process(&mut self.compositor);
        wait_then_kill_process(&mut self.settingsd);
        wait_then_kill_process(&mut self.watcher);
        wait_then_kill_process(&mut self.notifications);

        if let Some(owner) = &self.app_scopes {
            owner.shutdown()?;
        }

        // Do not traverse/remove a still-mounted desktop filesystem on failure.
        crate::portal_cleanup::unmount_runtime(&self.paths.session_dir)?;

        match fs::remove_dir_all(&self.paths.session_dir) {
            Ok(()) => {
                info!(path = %self.paths.session_dir.display(), "removed session runtime directory")
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn check_startup_deadline(&self, child: &str) -> Result<(), Box<dyn std::error::Error>> {
        crate::readiness::check_startup_deadline(
            self.startup_deadline,
            self.shutdown_requested.load(Ordering::SeqCst),
            child,
        )
    }

    fn record_processes(&self) -> std::io::Result<()> {
        let pid =
            |process: &Option<ManagedProcess>| process.as_ref().map_or(0, ManagedProcess::pid);
        fs::write(
            self.paths.session_dir.join("processes.json"),
            format!(
                "{{\"supervisor\":{},\"settingsd\":{},\"compositor\":{},\"shell\":{},\"notifications\":{}{}}}\n",
                std::process::id(),
                pid(&self.settingsd),
                pid(&self.compositor),
                pid(&self.shell),
                pid(&self.notifications),
                self.desktop_services.process_json_fields(),
            ),
        )
    }

    fn restart_process(
        &mut self,
        name: &'static str,
        status: ExitStatus,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let process = match name {
            "settingsd" => &mut self.settingsd,
            "shell" => &mut self.shell,
            "notifications" => &mut self.notifications,
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
        self.record_processes()?;
        Ok(true)
    }
}
