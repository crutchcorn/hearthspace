use clap::Parser;
use tracing::info;

mod ai;
mod app_scopes;
mod appearance_portal;
mod audio_env;
mod bounded_command;
mod cli;
mod compositor;
mod desktop_entries;
mod desktop_services;
mod dev_cargo;
mod gtk_defaults;
mod login_session;
mod portal_cleanup;
mod power;
mod private_bus_address;
mod privileges;
mod process;
mod profile_env;
mod readiness;
mod session_bus;
mod session_env;
mod settingsd;
mod signals;
mod supervisor;
mod wayvnc;

use cli::{Args, resolve_mode};
use desktop_entries::generate_desktop_entries;
use session_env::{create_session_env, create_session_paths};
use signals::install_signal_handler;
use supervisor::Supervisor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    privileges::clear_inherited()?;
    let mut args = Args::parse();
    login_session::validate(&args)?;
    initialize_tracing(&args.log_level);

    let wayvnc = wayvnc::prepare(&args)?;

    let mode = resolve_mode(&args);
    let owned_bus = session_env::automatic_private(&args, mode)
        .then(private_bus_address::generate)
        .transpose()?;
    if owned_bus.is_some() {
        args.private_session_bus = true;
    }
    let paths = create_session_paths(args.session_dir.as_deref(), args.private_session_bus)?;
    if desktop_services::enabled(&args) {
        desktop_services::prepare(&paths, args.private_session_bus)?;
    }
    let session_env = create_session_env(&args, mode, &paths, owned_bus.as_deref())?;
    gtk_defaults::prepare(&paths.session_dir, &session_env);

    generate_desktop_entries(&paths, mode, args.binary_dir.as_deref())?;

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
    let mut supervisor =
        Supervisor::new(mode, paths, session_env, shutdown_requested, &args, wayvnc);

    let monitor_result = (|| {
        supervisor.start_session_bus(owned_bus.as_deref())?;
        if args.login_session {
            supervisor.start_login_guard(&args)?;
        }
        supervisor.start_settingsd(if args.no_settingsd_restart { 0 } else { 3 })?;
        supervisor.wait_for_settingsd_ready()?;
        supervisor.start_compositor()?;
        supervisor.wait_for_compositor_ready()?;
        supervisor.prepare_activation_environment()?;
        if desktop_services::enabled(&args) {
            supervisor.start_power()?;
            supervisor.start_notifications()?;
            supervisor.start_desktop_services()?;
        }
        supervisor.start_shell(if args.no_shell_restart { 0 } else { 3 })?;
        supervisor.start_wayvnc();
        supervisor.start_app_watcher_if_available(&args)?;

        supervisor.monitor_until_exit()
    })();
    let cleanup_result = supervisor.shutdown();
    monitor_result?;
    cleanup_result.map_err(Into::into)
}

fn initialize_tracing(log_level: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(log_level));

    match env_filter {
        Ok(env_filter) => tracing_subscriber::fmt().with_env_filter(env_filter).init(),
        Err(_) => tracing_subscriber::fmt().init(),
    }
}
