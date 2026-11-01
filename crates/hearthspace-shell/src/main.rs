use std::fs::OpenOptions;

mod startup_resources;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_tracing();
    tracing::info!("starting Hearthspace shell");
    startup_resources::raise_file_descriptor_limit();
    hearthspace_shell::ui::run()
}

fn initialize_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env();
    if let Some(log_path) = std::env::var_os(hearthspace_common::LOG_FILE_ENV) {
        let make_writer = move || {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .expect("failed to open Hearthspace log file")
        };
        let subscriber = tracing_subscriber::fmt().with_writer(make_writer);
        match env_filter {
            Ok(env_filter) => subscriber.with_env_filter(env_filter).init(),
            Err(_) => subscriber.init(),
        }
        return;
    }

    match env_filter {
        Ok(env_filter) => tracing_subscriber::fmt().with_env_filter(env_filter).init(),
        Err(_) => tracing_subscriber::fmt().init(),
    }
}
