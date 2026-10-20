use std::{fs, io, io::Write, path::Path, process};

use hearthspace_ipc::{BackgroundMode, MonitorConfig, Settings};

pub(super) fn load_settings_from_path(path: impl AsRef<Path>) -> io::Result<Settings> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_settings(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error),
    }
}

pub(super) fn save_settings_to_path(settings: Settings, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension(format!("tmp-{}", process::id()));
    let mut file = fs::File::create(&temp_path)?;
    file.write_all(format_settings(settings).as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

fn parse_settings(contents: &str) -> io::Result<Settings> {
    let mut settings = Settings::default();
    settings.monitors.clear();
    let mut current_monitor: Option<MonitorConfig> = None;
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line == "[[monitor]]" {
            if let Some(monitor) = current_monitor.take() {
                settings.monitors.push(monitor);
            }
            current_monitor = Some(MonitorConfig {
                id: String::new(),
                name: String::new(),
                width: 1920,
                height: 1080,
                scale: 1,
                x: 0,
                y: 0,
            });
            continue;
        }

        if line.starts_with('[') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid settings line: {line:?}"),
            ));
        };
        let key = key.trim();
        let value = value.trim();

        if let Some(monitor) = &mut current_monitor {
            match key {
                "id" => monitor.id = value.to_string(),
                "name" => monitor.name = value.to_string(),
                "width" => monitor.width = parse_i32(value, "monitor width")?,
                "height" => monitor.height = parse_i32(value, "monitor height")?,
                "scale" => monitor.scale = parse_positive_i32(value, "monitor scale")?,
                "x" => monitor.x = parse_i32(value, "monitor x")?,
                "y" => monitor.y = parse_i32(value, "monitor y")?,
                _ => {}
            }
        } else if key == "test" {
            settings.test = parse_bool(value).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid test setting: {error}"),
                )
            })?;
        } else if key == "background_mode" {
            settings.background_mode = value.parse::<BackgroundMode>().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid background_mode setting: {error}"),
                )
            })?;
        } else if key == "background_moves_with_canvas" {
            let moves_with_canvas = parse_bool(value).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid background_moves_with_canvas setting: {error}"),
                )
            })?;
            settings.background_mode =
                BackgroundMode::from_legacy_moves_with_canvas(moves_with_canvas);
        } else if key == "background_image_path" {
            settings.background_image_path = parse_background_image_path(value);
        } else if key == "color_mode" {
            settings.appearance.color_mode = value.parse().map_err(io::Error::other)?;
        } else if key == "accent_color" {
            settings.appearance.accent_color = value.parse().map_err(io::Error::other)?;
        }
    }

    if let Some(monitor) = current_monitor.take() {
        settings.monitors.push(monitor);
    }
    settings.monitors.retain(|monitor| !monitor.id.is_empty());
    if settings.monitors.is_empty() {
        settings.monitors = Settings::default().monitors;
    }
    Ok(settings)
}

fn format_settings(settings: Settings) -> String {
    let mut contents = format!(
        "test = {}\nbackground_mode = {}\nbackground_image_path = {}\ncolor_mode = {}\naccent_color = {}\n",
        settings.test,
        settings.background_mode,
        settings
            .background_image_path
            .as_deref()
            .unwrap_or("default"),
        settings.appearance.color_mode,
        settings.appearance.accent_color,
    );
    for monitor in settings.monitors {
        contents.push_str(&format!(
            "\n[[monitor]]\n\
             id = {}\n\
             name = {}\n\
             width = {}\n\
             height = {}\n\
             scale = {}\n\
             x = {}\n\
             y = {}\n",
            monitor.id,
            monitor.name,
            monitor.width,
            monitor.height,
            monitor.scale,
            monitor.x,
            monitor.y,
        ));
    }
    contents
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected true or false, got {value:?}")),
    }
}

fn parse_background_image_path(value: &str) -> Option<String> {
    (value != "default").then(|| value.to_string())
}

fn parse_i32(value: &str, label: &str) -> io::Result<i32> {
    value.parse::<i32>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid {label}: {error}"),
        )
    })
}

fn parse_positive_i32(value: &str, label: &str) -> io::Result<i32> {
    let value = parse_i32(value, label)?;
    if value <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} must be positive"),
        ));
    }
    Ok(value)
}
