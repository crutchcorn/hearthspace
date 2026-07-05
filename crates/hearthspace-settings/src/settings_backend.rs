use std::{
    env, io,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
};

use hearthspace_ipc::{MonitorConfig, Settings, client_set_monitor_position};

pub(crate) fn save_monitor_positions(
    monitors: &[MonitorConfig],
) -> std::io::Result<Option<Settings>> {
    let mut saved = None;
    for monitor in monitors {
        saved = Some(client_set_monitor_position(
            &monitor.id,
            monitor.x,
            monitor.y,
        )?);
    }
    Ok(saved)
}

pub(crate) fn notify_compositor_settings_changed() -> io::Result<()> {
    let command_socket = command_socket_path()?;
    let mut stream = UnixStream::connect(&command_socket)?;
    stream.write_all(b"reload-settings\n")?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response == "ok\n" {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "unexpected compositor response: {response:?}"
        )))
    }
}

fn command_socket_path() -> io::Result<PathBuf> {
    env::var_os(hearthspace_common::SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{} is not set",
                    hearthspace_common::SHELL_COMMAND_SOCKET_ENV
                ),
            )
        })
}
