fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let args = std::env::args().collect::<Vec<_>>();

    if args.iter().any(|arg| arg == "--shell-bar") {
        hearthspace::shell_bar::run()
    } else {
        hearthspace::run_with_options(hearthspace::RunOptions {
            scroll_zooms_without_super: args
                .iter()
                .any(|arg| arg == hearthspace::config::SCROLL_ZOOMS_FLAG),
        })
    }
}
