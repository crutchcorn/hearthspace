use std::fs::OpenOptions;

use tracing::{debug, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_tracing();

    let args = std::env::args().collect::<Vec<_>>();
    debug!(?args, "parsed compositor arguments");

    let options = hearthspace_compositor::RunOptions {
        scroll_zooms_without_super: args
            .iter()
            .any(|arg| arg == hearthspace_compositor::config::SCROLL_ZOOMS_FLAG),
        backend: parse_backend_selection(&args)?,
        headless_output_size: parse_headless_output_size(&args)?,
        headless_output_scale: parse_headless_output_scale(&args)?,
        exit_after: parse_exit_after(&args)?,
        start_shell: !args
            .iter()
            .any(|arg| arg == hearthspace_compositor::config::NO_SHELL_FLAG),
    };
    info!(?options, "starting Hearthspace compositor");
    hearthspace_compositor::run_with_options(options)
}

fn initialize_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env();
    if let Some(log_path) = std::env::var_os(hearthspace_compositor::config::LOG_FILE_ENV) {
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

fn parse_backend_selection(
    args: &[String],
) -> Result<hearthspace_compositor::BackendSelection, String> {
    let mut selected = Vec::new();
    if args
        .iter()
        .any(|arg| arg == hearthspace_compositor::config::HEADLESS_FLAG)
    {
        selected.push((
            hearthspace_compositor::config::HEADLESS_FLAG,
            hearthspace_compositor::BackendSelection::Headless,
        ));
    }
    if args.iter().any(|arg| arg == hearthspace_compositor::config::TTY_FLAG) {
        selected.push((
            hearthspace_compositor::config::TTY_FLAG,
            hearthspace_compositor::BackendSelection::Udev,
        ));
    }
    if args
        .iter()
        .any(|arg| arg == hearthspace_compositor::config::WINIT_FLAG)
    {
        selected.push((
            hearthspace_compositor::config::WINIT_FLAG,
            hearthspace_compositor::BackendSelection::Winit,
        ));
    }

    match selected.as_slice() {
        [] => Ok(hearthspace_compositor::BackendSelection::Auto),
        [(_, backend)] => Ok(*backend),
        _ => Err(format!(
            "choose only one backend flag: {}, {}, or {}",
            hearthspace_compositor::config::HEADLESS_FLAG,
            hearthspace_compositor::config::TTY_FLAG,
            hearthspace_compositor::config::WINIT_FLAG
        )),
    }
}

fn parse_exit_after(args: &[String]) -> Result<Option<std::time::Duration>, String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == hearthspace_compositor::config::EXIT_AFTER_MS_FLAG {
            let Some(value) = args.get(index + 1) else {
                return Err(format!(
                    "{} requires a positive integer",
                    hearthspace_compositor::config::EXIT_AFTER_MS_FLAG
                ));
            };
            return parse_positive_duration_ms(value).map(Some);
        }

        if let Some(value) =
            arg.strip_prefix(&format!(
                "{}=",
                hearthspace_compositor::config::EXIT_AFTER_MS_FLAG
            ))
        {
            return parse_positive_duration_ms(value).map(Some);
        }
    }

    Ok(None)
}

fn parse_headless_output_size(args: &[String]) -> Result<Option<(i32, i32)>, String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == hearthspace_compositor::config::HEADLESS_SIZE_FLAG {
            let Some(value) = args.get(index + 1) else {
                return Err(format!(
                    "{} requires WIDTHxHEIGHT",
                    hearthspace_compositor::config::HEADLESS_SIZE_FLAG
                ));
            };
            return parse_size(value).map(Some);
        }

        if let Some(value) =
            arg.strip_prefix(&format!(
                "{}=",
                hearthspace_compositor::config::HEADLESS_SIZE_FLAG
            ))
        {
            return parse_size(value).map(Some);
        }
    }

    Ok(None)
}

fn parse_headless_output_scale(args: &[String]) -> Result<Option<i32>, String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == hearthspace_compositor::config::HEADLESS_SCALE_FLAG {
            let Some(value) = args.get(index + 1) else {
                return Err(format!(
                    "{} requires a positive integer",
                    hearthspace_compositor::config::HEADLESS_SCALE_FLAG
                ));
            };
            return parse_positive_dimension(value, "scale").map(Some);
        }

        if let Some(value) =
            arg.strip_prefix(&format!(
                "{}=",
                hearthspace_compositor::config::HEADLESS_SCALE_FLAG
            ))
        {
            return parse_positive_dimension(value, "scale").map(Some);
        }
    }

    Ok(None)
}

fn parse_size(value: &str) -> Result<(i32, i32), String> {
    let Some((width, height)) = value.split_once('x').or_else(|| value.split_once('X')) else {
        return Err(format!("invalid size {value:?}; expected WIDTHxHEIGHT"));
    };
    let width = parse_positive_dimension(width, "width")?;
    let height = parse_positive_dimension(height, "height")?;
    Ok((width, height))
}

fn parse_positive_dimension(value: &str, name: &str) -> Result<i32, String> {
    let parsed = value
        .parse::<i32>()
        .map_err(|_| format!("invalid {name} {value:?}"))?;
    if parsed <= 0 {
        return Err(format!("{name} must be positive"));
    }
    Ok(parsed)
}

fn parse_positive_duration_ms(value: &str) -> Result<std::time::Duration, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("invalid duration {value:?}"))?;
    if parsed == 0 {
        return Err("duration must be positive".into());
    }
    Ok(std::time::Duration::from_millis(parsed))
}
