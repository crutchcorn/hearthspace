fn main() -> Result<(), Box<dyn std::error::Error>> {
    match tracing_subscriber::EnvFilter::try_from_default_env() {
        Ok(env_filter) => {
            tracing_subscriber::fmt().with_env_filter(env_filter).init();
        }
        _ => {
            tracing_subscriber::fmt().init();
        }
    }

    let args = std::env::args().collect::<Vec<_>>();

    if args
        .iter()
        .any(|arg| arg == hearthspace::config::GTK_TEST_APP_FLAG)
    {
        run_gtk_test_app()
    } else if args
        .iter()
        .any(|arg| arg == hearthspace::config::SHELL_FLAG)
    {
        hearthspace::shell::xilem_shell::run()
    } else {
        hearthspace::run_with_options(hearthspace::RunOptions {
            scroll_zooms_without_super: args
                .iter()
                .any(|arg| arg == hearthspace::config::SCROLL_ZOOMS_FLAG),
        })
    }
}

#[cfg(feature = "test-apps")]
fn run_gtk_test_app() -> Result<(), Box<dyn std::error::Error>> {
    hearthspace::test_apps::gtk::run()
}

#[cfg(not(feature = "test-apps"))]
fn run_gtk_test_app() -> Result<(), Box<dyn std::error::Error>> {
    Err("GTK test app support is not enabled; rebuild with `--features test-apps`".into())
}
