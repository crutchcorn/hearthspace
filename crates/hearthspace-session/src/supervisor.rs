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
    compositor::{CompositorBackend, select_compositor_backend},
    process::{ManagedProcess, poll_process, terminate_process, wait_then_kill_process},
    session_env::{SessionEnv, SessionPaths},
    settingsd::settingsd_ping,
};

mod activation;
mod commands;
mod notifications;
mod private_bus;
mod shutdown;
mod startup;
mod wayvnc;

pub(crate) struct Supervisor {
    mode: RunMode,
    backend: Option<Backend>,
    private_session_bus: bool,
    screen_cast_enabled: bool,
    activation_environment: Option<crate::session_bus::ActivationEnvironment>,
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
    session_bus: Option<ManagedProcess>,
    desktop_services: crate::desktop_services::DesktopServices,
    appearance_portal: Option<crate::appearance_portal::AppearancePortal>,
    app_scopes: Option<crate::app_scopes::AppScopes>,
    power: Option<crate::power::PowerManager>,
    ai: Option<crate::ai::SessionModel>,
    wayvnc_config: Option<crate::wayvnc::Configuration>,
    wayvnc: Option<ManagedProcess>,
}

impl Supervisor {
    pub(crate) fn new(
        mode: RunMode,
        paths: SessionPaths,
        session_env: SessionEnv,
        shutdown_requested: Arc<AtomicBool>,
        args: &Args,
        wayvnc_config: Option<crate::wayvnc::Configuration>,
    ) -> Self {
        Self {
            mode,
            backend: args.backend,
            private_session_bus: args.private_session_bus,
            screen_cast_enabled: crate::desktop_services::enabled(args),
            activation_environment: None,
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
            session_bus: None,
            desktop_services: crate::desktop_services::DesktopServices::default(),
            appearance_portal: None,
            app_scopes: None,
            power: None,
            ai: None,
            wayvnc_config,
            wayvnc: None,
        }
    }

    pub(crate) fn start_login_guard(&self, args: &Args) -> std::io::Result<()> {
        crate::login_session::start_guard(args, &self.paths, &self.shutdown_requested)
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
        self.prepare_development_locker()?;
        let backend = match self.backend {
            Some(Backend::Native) => CompositorBackend::NativeTty,
            Some(Backend::Nested) => CompositorBackend::NestedWinit,
            None => select_compositor_backend(),
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
                self.mode,
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
        let spec = self.compositor_spec(backend);
        self.compositor = Some(ManagedProcess::spawn(spec, 0)?);
        self.record_processes()?;
        Ok(())
    }

    pub(crate) fn start_power(&mut self) -> std::io::Result<()> {
        self.power = Some(crate::power::PowerManager::start(
            self.paths.session_dir.clone(),
            self.paths.shell_command_socket.clone(),
        )?);
        Ok(())
    }

    pub(crate) fn start_shell(
        &mut self,
        max_restarts: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-shell", &[], true);
        self.shell = Some(ManagedProcess::spawn(spec, max_restarts)?);
        self.record_processes()?;
        self.wait_for_desktop_ready()?;
        self.ai = crate::ai::SessionModel::start();
        Ok(())
    }

    pub(crate) fn start_desktop_services(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let bus = self
            .session_env
            .vars
            .iter()
            .find(|(key, _)| key == "HEARTHSPACE_SESSION_BUS_ADDRESS")
            .map(|(_, value)| value.clone())
            .ok_or("appearance publication requires a session bus")?;
        self.appearance_portal = Some(crate::appearance_portal::AppearancePortal::start(
            bus,
            self.paths.settings_socket.clone(),
            self.startup_deadline,
            &self.shutdown_requested,
        )?);
        self.desktop_services.start(
            &self.paths,
            &self.session_env,
            crate::desktop_services::Configuration {
                mode: self.mode,
                binary_dir: self.binary_dir.as_deref(),
                private_session_bus: self.private_session_bus,
            },
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

            self.check_session_bus()?;

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
            if let Some(portal) = &mut self.appearance_portal {
                portal.poll(&self.shutdown_requested);
            }
            self.poll_wayvnc();

            thread::sleep(Duration::from_millis(100));
        }
    }

    fn check_startup_deadline(&mut self, child: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.check_session_bus()?;
        crate::readiness::check_startup_deadline(
            self.startup_deadline,
            self.shutdown_requested.load(Ordering::SeqCst),
            child,
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
