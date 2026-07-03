use clap::{ArgAction, Parser};

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

fn main() {
    let args = Args::parse();
    initialize_tracing(&args.log_level);
    tracing::info!(?args, "parsed session supervisor arguments");
}

fn initialize_tracing(log_level: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(log_level));

    match env_filter {
        Ok(env_filter) => tracing_subscriber::fmt().with_env_filter(env_filter).init(),
        Err(_) => tracing_subscriber::fmt().init(),
    }
}
