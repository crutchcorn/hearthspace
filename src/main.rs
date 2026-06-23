fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    if std::env::args().any(|arg| arg == "--shell-bar") {
        hearthspace::shell_bar::run()
    } else {
        hearthspace::run()
    }
}
