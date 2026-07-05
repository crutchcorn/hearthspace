use clap::{ArgAction, Parser};

#[derive(Debug, Parser)]
#[command(about = "Start a Hearthspace desktop session")]
pub(crate) struct Args {
    /// Run child processes through `cargo run -p ...`.
    #[arg(long, conflicts_with = "installed")]
    pub(crate) dev: bool,

    /// Run installed child binaries directly.
    #[arg(long, conflicts_with = "dev")]
    pub(crate) installed: bool,

    /// Wayland display socket name.
    #[arg(long)]
    pub(crate) wayland_display: Option<String>,

    /// Start development watchers for Hearthspace apps.
    #[arg(long, conflicts_with = "no_watch_apps")]
    pub(crate) watch_apps: bool,

    /// Do not start development watchers.
    #[arg(long, conflicts_with = "watch_apps")]
    pub(crate) no_watch_apps: bool,

    /// Do not restart shell if it exits.
    #[arg(long)]
    pub(crate) no_shell_restart: bool,

    /// Do not restart settingsd if it exits.
    #[arg(long)]
    pub(crate) no_settingsd_restart: bool,

    /// Set log level for session and children.
    #[arg(long, default_value = "info")]
    pub(crate) log_level: String,

    /// Print the generated session environment.
    #[arg(long)]
    pub(crate) print_env: bool,

    /// Print what would be started without starting processes.
    #[arg(long, action = ArgAction::SetTrue)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunMode {
    DevCargo,
    InstalledBinaries,
}

pub(crate) fn resolve_mode(args: &Args) -> RunMode {
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
