use clap::Parser;
use tracing::info;

mod cli;
mod compositor;
mod desktop_entries;
mod dev_cargo;
mod process;
mod session_env;
mod settingsd;
mod signals;
mod supervisor;

use cli::{Args, RunMode, resolve_mode};
use desktop_entries::generate_dev_desktop_entries;
use session_env::{create_session_env, create_session_paths};
use signals::install_signal_handler;
use supervisor::Supervisor;

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

    let monitor_result = (|| {
        supervisor.start_settingsd(if args.no_settingsd_restart { 0 } else { 3 })?;
        supervisor.wait_for_settingsd_ready()?;
        supervisor.start_compositor()?;
        supervisor.wait_for_compositor_ready()?;
        supervisor.start_shell(if args.no_shell_restart { 0 } else { 3 })?;
        supervisor.start_app_watcher_if_available(&args)?;

        supervisor.monitor_until_exit()
    })();
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
