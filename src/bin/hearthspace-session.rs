use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use clap::{ArgAction, Parser};
use tracing::info;

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
    session_dir: PathBuf,
    applications_dir: PathBuf,
    settings_socket: PathBuf,
    session_socket: PathBuf,
}

#[derive(Debug)]
struct SessionEnv {
    wayland_display: String,
    vars: Vec<(String, String)>,
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

    info!("session process supervision is not implemented yet");
    Ok(())
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
    let session_dir = PathBuf::from(runtime_dir)
        .join("hearthspace")
        .join(format!("session-{}", process::id()));
    let applications_dir = session_dir.join("share/applications");

    fs::create_dir_all(&applications_dir)?;

    Ok(SessionPaths {
        settings_socket: session_dir.join("settings.sock"),
        session_socket: session_dir.join("session.sock"),
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
        ("RUST_LOG".to_string(), args.log_level.clone()),
        ("XDG_DATA_DIRS".to_string(), xdg_data_dirs(&session_dir)?),
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

fn xdg_data_dirs(session_dir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut dirs = vec![PathBuf::from(session_dir)];
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
         X-Hearthspace-Dev=true\n",
        escape_exec_path(&exec)
    );
    let path = paths.applications_dir.join("hearthspace-settings.desktop");
    fs::write(&path, desktop_entry)?;
    info!(path = %path.display(), "generated development desktop entry");
    Ok(())
}

fn escape_exec_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
