use hearthspace_ipc::{Settings, SettingsRequest};
use std::io;

pub(super) fn apply(settings: &mut Settings, request: SettingsRequest) -> io::Result<()> {
    match request {
        SettingsRequest::Get => {}
        SettingsRequest::SetTest(value) => settings.test = value,
        SettingsRequest::SetColorMode(value) => settings.appearance.color_mode = value,
        SettingsRequest::SetAccentColor(value) => settings.appearance.accent_color = value,
        SettingsRequest::SetBackgroundMode(value) => settings.background_mode = value,
        SettingsRequest::SetBackgroundImagePath(value) => settings.background_image_path = value,
        SettingsRequest::SetMonitorPosition { id, x, y } => {
            let monitor = monitor(settings, &id)?;
            monitor.x = x;
            monitor.y = y;
        }
        SettingsRequest::SetMonitorScale { id, scale } => monitor(settings, &id)?.scale = scale,
        SettingsRequest::SetDisplayLayout(monitors) => {
            hearthspace_ipc::validate_display_layout(&monitors).map_err(io::Error::other)?;
            for monitor in monitors {
                if let Some(saved) = settings
                    .monitors
                    .iter_mut()
                    .find(|saved| saved.id == monitor.id)
                {
                    *saved = monitor;
                } else {
                    settings.monitors.push(monitor);
                }
            }
        }
    }
    Ok(())
}

fn monitor<'a>(
    settings: &'a mut Settings,
    id: &str,
) -> io::Result<&'a mut hearthspace_ipc::MonitorConfig> {
    settings
        .monitors
        .iter_mut()
        .find(|monitor| monitor.id == id)
        .ok_or_else(|| io::Error::other(format!("unknown monitor id: {id}")))
}
