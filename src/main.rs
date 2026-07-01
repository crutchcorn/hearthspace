use std::fs::OpenOptions;

use tracing::{debug, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_tracing();

    let args = std::env::args().collect::<Vec<_>>();
    debug!(?args, "parsed process arguments");

    if args
        .iter()
        .any(|arg| arg == hearthspace::config::GTK_TEST_APP_FLAG)
    {
        info!("starting GTK test app client");
        run_gtk_test_app()
    } else if args
        .iter()
        .any(|arg| arg == hearthspace::config::SHELL_FLAG)
    {
        info!("starting shell client");
        hearthspace::shell::xilem_shell::run()
    } else {
        let backend = parse_backend_selection(&args)?;
        let headless_output_size = parse_headless_output_size(&args)?;
        let headless_output_scale = parse_headless_output_scale(&args)?;
        let exit_after = parse_exit_after(&args)?;
        let options = hearthspace::RunOptions {
            scroll_zooms_without_super: args
                .iter()
                .any(|arg| arg == hearthspace::config::SCROLL_ZOOMS_FLAG),
            backend,
            headless_output_size,
            headless_output_scale,
            exit_after,
            start_shell: !args
                .iter()
                .any(|arg| arg == hearthspace::config::NO_SHELL_FLAG),
        };
        info!(?options, "starting compositor");
        hearthspace::run_with_options(options)
    }
}

fn initialize_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env();
    if let Some(log_path) = std::env::var_os(hearthspace::config::LOG_FILE_ENV) {
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

fn parse_exit_after(args: &[String]) -> Result<Option<std::time::Duration>, String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == hearthspace::config::EXIT_AFTER_MS_FLAG {
            let Some(value) = args.get(index + 1) else {
                return Err(format!(
                    "{} requires a positive integer",
                    hearthspace::config::EXIT_AFTER_MS_FLAG
                ));
            };
            return parse_positive_duration_ms(value).map(Some);
        }

        if let Some(value) =
            arg.strip_prefix(&format!("{}=", hearthspace::config::EXIT_AFTER_MS_FLAG))
        {
            return parse_positive_duration_ms(value).map(Some);
        }
    }

    Ok(None)
}

fn parse_backend_selection(args: &[String]) -> Result<hearthspace::BackendSelection, String> {
    let mut selected = Vec::new();
    if args
        .iter()
        .any(|arg| arg == hearthspace::config::HEADLESS_FLAG)
    {
        selected.push((
            hearthspace::config::HEADLESS_FLAG,
            hearthspace::BackendSelection::Headless,
        ));
    }
    if args.iter().any(|arg| arg == hearthspace::config::TTY_FLAG) {
        selected.push((
            hearthspace::config::TTY_FLAG,
            hearthspace::BackendSelection::Udev,
        ));
    }
    if args
        .iter()
        .any(|arg| arg == hearthspace::config::WINIT_FLAG)
    {
        selected.push((
            hearthspace::config::WINIT_FLAG,
            hearthspace::BackendSelection::Winit,
        ));
    }

    match selected.as_slice() {
        [] => Ok(hearthspace::BackendSelection::Auto),
        [(_, backend)] => Ok(*backend),
        _ => Err(format!(
            "choose only one backend flag: {}, {}, or {}",
            hearthspace::config::HEADLESS_FLAG,
            hearthspace::config::TTY_FLAG,
            hearthspace::config::WINIT_FLAG
        )),
    }
}

fn parse_headless_output_scale(args: &[String]) -> Result<Option<i32>, String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == hearthspace::config::HEADLESS_SCALE_FLAG {
            let Some(value) = args.get(index + 1) else {
                return Err(format!(
                    "{} requires a positive integer",
                    hearthspace::config::HEADLESS_SCALE_FLAG
                ));
            };
            return parse_positive_dimension(value, "scale").map(Some);
        }

        if let Some(value) =
            arg.strip_prefix(&format!("{}=", hearthspace::config::HEADLESS_SCALE_FLAG))
        {
            return parse_positive_dimension(value, "scale").map(Some);
        }
    }

    Ok(None)
}

#[cfg(feature = "test-apps")]
fn run_gtk_test_app() -> Result<(), Box<dyn std::error::Error>> {
    hearthspace::test_apps::gtk::run()
}

#[cfg(not(feature = "test-apps"))]
fn run_gtk_test_app() -> Result<(), Box<dyn std::error::Error>> {
    Err("GTK test app support is not enabled; rebuild with `--features test-apps`".into())
}

fn parse_headless_output_size(args: &[String]) -> Result<Option<(i32, i32)>, String> {
    for (index, arg) in args.iter().enumerate() {
        if arg == hearthspace::config::HEADLESS_SIZE_FLAG {
            let Some(value) = args.get(index + 1) else {
                return Err(format!(
                    "{} requires WIDTHxHEIGHT",
                    hearthspace::config::HEADLESS_SIZE_FLAG
                ));
            };
            return parse_size(value).map(Some);
        }

        if let Some(value) =
            arg.strip_prefix(&format!("{}=", hearthspace::config::HEADLESS_SIZE_FLAG))
        {
            return parse_size(value).map(Some);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headless_size_forms() {
        assert_eq!(
            parse_headless_output_size(&["hearthspace".into(), "--headless-size=800x600".into()]),
            Ok(Some((800, 600)))
        );
        assert_eq!(
            parse_headless_output_size(&[
                "hearthspace".into(),
                "--headless-size".into(),
                "1024X768".into(),
            ]),
            Ok(Some((1024, 768)))
        );
    }

    #[test]
    fn parses_headless_scale_forms() {
        assert_eq!(
            parse_headless_output_scale(&["hearthspace".into(), "--headless-scale=2".into()]),
            Ok(Some(2))
        );
        assert_eq!(
            parse_headless_output_scale(&[
                "hearthspace".into(),
                "--headless-scale".into(),
                "3".into(),
            ]),
            Ok(Some(3))
        );
    }

    #[test]
    fn parses_exit_after_forms() {
        assert_eq!(
            parse_exit_after(&["hearthspace".into(), "--exit-after-ms=250".into()]),
            Ok(Some(std::time::Duration::from_millis(250)))
        );
        assert_eq!(
            parse_exit_after(&["hearthspace".into(), "--exit-after-ms".into(), "500".into(),]),
            Ok(Some(std::time::Duration::from_millis(500)))
        );
    }

    #[test]
    fn parses_backend_selection() {
        assert!(matches!(
            parse_backend_selection(&["hearthspace".into()]),
            Ok(hearthspace::BackendSelection::Auto)
        ));
        assert!(matches!(
            parse_backend_selection(&["hearthspace".into(), "--headless".into()]),
            Ok(hearthspace::BackendSelection::Headless)
        ));
        assert!(matches!(
            parse_backend_selection(&["hearthspace".into(), "--tty".into()]),
            Ok(hearthspace::BackendSelection::Udev)
        ));
        assert!(matches!(
            parse_backend_selection(&["hearthspace".into(), "--winit".into()]),
            Ok(hearthspace::BackendSelection::Winit)
        ));
    }

    #[test]
    fn rejects_conflicting_backend_selection() {
        assert!(
            parse_backend_selection(&["hearthspace".into(), "--headless".into(), "--tty".into()])
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_headless_size() {
        assert!(parse_size("800").is_err());
        assert!(parse_size("0x600").is_err());
        assert!(parse_size("800xnope").is_err());
        assert!(
            parse_headless_output_scale(&["hearthspace".into(), "--headless-scale=0".into()])
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_exit_after() {
        assert!(parse_exit_after(&["hearthspace".into(), "--exit-after-ms=0".into()]).is_err());
        assert!(parse_exit_after(&["hearthspace".into(), "--exit-after-ms".into()]).is_err());
    }
}
