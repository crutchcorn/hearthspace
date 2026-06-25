use std::{
    env, fs,
    io::{self, ErrorKind, Read},
    os::unix::{
        fs::symlink,
        net::{UnixListener as CommandListener, UnixStream},
    },
    path::PathBuf,
    process::Command,
};

use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, generic::Generic};

use crate::{
    config::*,
    geometry::CanvasPoint,
    shell::{
        ShellCommand, SpawnTarget,
        app_catalog::{DesktopApp, spawn_argv_with_env},
    },
};

use super::{App, CalloopData};

pub(super) fn command_socket_path() -> PathBuf {
    runtime_path(SHELL_COMMAND_SOCKET_NAME)
}

fn runtime_path(name: &str) -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join(name)
}

/// Snap instance names are embedded into a runtime directory path, so reject any
/// value containing path separators or other characters that could escape the
/// intended `snap.<name>` directory.
fn is_valid_snap_instance_name(instance_name: &str) -> bool {
    instance_name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn ensure_snap_wayland_socket(instance_name: &str) -> std::io::Result<String> {
    if !is_valid_snap_instance_name(instance_name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid snap instance name {instance_name:?}"),
        ));
    }

    let snap_runtime_dir = runtime_path(&format!("snap.{instance_name}"));
    fs::create_dir_all(&snap_runtime_dir)?;
    let snap_socket_path = snap_runtime_dir.join(WAYLAND_DISPLAY_NAME);

    match fs::remove_file(&snap_socket_path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    symlink(
        PathBuf::from("..").join(WAYLAND_DISPLAY_NAME),
        &snap_socket_path,
    )?;
    Ok(WAYLAND_DISPLAY_NAME.to_string())
}

fn launch_environment_for_app(app: &DesktopApp) -> std::io::Result<Vec<(String, String)>> {
    let base = app_state_base_dir().join(sanitized_path_component(&app.id));
    let config = base.join("config");
    let cache = base.join("cache");
    let data = base.join("data");
    let state = base.join("state");

    for dir in [&config, &cache, &data, &state] {
        fs::create_dir_all(dir)?;
    }

    Ok(vec![
        (
            "XDG_CONFIG_HOME".to_string(),
            config.to_string_lossy().into_owned(),
        ),
        (
            "XDG_CACHE_HOME".to_string(),
            cache.to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            data.to_string_lossy().into_owned(),
        ),
        (
            "XDG_STATE_HOME".to_string(),
            state.to_string_lossy().into_owned(),
        ),
    ])
}

fn app_state_base_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(env::temp_dir)
        .join("hearthspace/apps")
}

fn sanitized_path_component(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn remove_stale_socket(path: &PathBuf) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn spawn_shell_bar(command_socket_path: &PathBuf) {
    let current_exe = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Failed to locate current executable for shell bar: {error}");
            return;
        }
    };

    if let Err(error) = Command::new(current_exe)
        .arg("--shell-bar")
        .env("WAYLAND_DISPLAY", WAYLAND_DISPLAY_NAME)
        .env(SHELL_COMMAND_SOCKET_ENV, command_socket_path)
        .spawn()
    {
        eprintln!("Failed to spawn shell bar: {error}");
    }
}

fn ensure_gtk_client_settings() -> std::io::Result<PathBuf> {
    let config_dir = runtime_path(GTK_CLIENT_CONFIG_DIR_NAME);
    let settings = "[Settings]\ngtk-decoration-layout=:close\n";

    for gtk_version_dir in ["gtk-3.0", "gtk-4.0"] {
        let dir = config_dir.join(gtk_version_dir);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("settings.ini"), settings)?;
    }

    let gsettings_dir = config_dir.join("glib-2.0/settings");
    fs::create_dir_all(&gsettings_dir)?;
    fs::write(
        gsettings_dir.join("keyfile"),
        "[org/gnome/desktop/wm/preferences]\nbutton-layout=':close'\n",
    )?;

    Ok(config_dir)
}

fn apply_gtk_client_environment(command: &mut Command) {
    match ensure_gtk_client_settings() {
        Ok(config_dir) => {
            command.env("XDG_CONFIG_HOME", config_dir);
            command.env("GSETTINGS_BACKEND", "keyfile");
        }
        Err(error) => eprintln!("Failed to prepare GTK client settings: {error}"),
    }
}

/// Largest amount of unparsed command data we will buffer for a single
/// connection before giving up. Commands are short single lines, so this only
/// guards against a misbehaving client streaming data without a newline.
const MAX_COMMAND_BUFFER_BYTES: usize = 4096;

/// Accept every pending command connection and register each one as its own
/// non-blocking calloop source. Reading is incremental, so a client that
/// connects but never finishes sending cannot block the event loop.
pub(super) fn accept_command_connections<'l>(
    listener: &CommandListener,
    handle: &LoopHandle<'l, CalloopData>,
) -> io::Result<()> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true)?;
                register_command_connection(handle, stream);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn register_command_connection<'l>(handle: &LoopHandle<'l, CalloopData>, stream: UnixStream) {
    let mut buffer: Vec<u8> = Vec::new();
    let source = Generic::new(stream, Interest::READ, Mode::Level);
    if let Err(error) = handle.insert_source(source, move |_, stream, data| {
        Ok(read_command_connection(
            stream,
            &mut buffer,
            &mut data.state,
        ))
    }) {
        eprintln!("Failed to register shell command connection: {error}");
    }
}

fn read_command_connection(
    stream: &UnixStream,
    buffer: &mut Vec<u8>,
    state: &mut App,
) -> PostAction {
    // `Read` is implemented for `&UnixStream`, so read through a shared ref.
    let mut reader = stream;
    let mut chunk = [0u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                // Client closed: run any trailing line that lacked a newline.
                if !buffer.is_empty() {
                    run_command_line(buffer, state);
                }
                return PostAction::Remove;
            }
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                drain_complete_commands(buffer, state);
                if buffer.len() > MAX_COMMAND_BUFFER_BYTES {
                    eprintln!(
                        "Shell command exceeded {MAX_COMMAND_BUFFER_BYTES} bytes without a newline; dropping connection"
                    );
                    return PostAction::Remove;
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == ErrorKind::WouldBlock => return PostAction::Continue,
            Err(error) => {
                eprintln!("Failed to read shell command: {error}");
                return PostAction::Remove;
            }
        }
    }
}

fn drain_complete_commands(buffer: &mut Vec<u8>, state: &mut App) {
    for command in take_complete_commands(buffer) {
        state.run_control_action(command);
    }
}

/// Split every complete newline-terminated command off the front of `buffer`,
/// returning the parsed commands in order. Unterminated trailing bytes are left
/// in `buffer` for the next read. Lines that fail to parse are silently skipped.
fn take_complete_commands(buffer: &mut Vec<u8>) -> Vec<ShellCommand> {
    let mut commands = Vec::new();
    while let Some(newline) = buffer.iter().position(|&byte| byte == b'\n') {
        let line: Vec<u8> = buffer.drain(..=newline).collect();
        if let Some(command) = parse_command_line(&line) {
            commands.push(command);
        }
    }
    commands
}

fn run_command_line(bytes: &[u8], state: &mut App) {
    if let Some(command) = parse_command_line(bytes) {
        state.run_control_action(command);
    }
}

fn parse_command_line(bytes: &[u8]) -> Option<ShellCommand> {
    let text = std::str::from_utf8(bytes).ok()?;
    ShellCommand::parse(text.trim())
}

impl App {
    fn run_control_action(&mut self, action: ShellCommand) {
        self.advance_viewport_animation();

        match action {
            ShellCommand::Spawn(SpawnTarget::A11yTest) => self.spawn_a11y_test(),
            ShellCommand::Spawn(SpawnTarget::Foot) => self.spawn_foot(),
            ShellCommand::LaunchApp(app_id) => self.launch_app(&app_id),
            ShellCommand::PanLeft => self.pan_viewport_by(-self.horizontal_pan_step(), 0),
            ShellCommand::PanRight => self.pan_viewport_by(self.horizontal_pan_step(), 0),
            ShellCommand::PanUp => self.pan_viewport_by(0, -self.vertical_pan_step()),
            ShellCommand::PanDown => self.pan_viewport_by(0, self.vertical_pan_step()),
            ShellCommand::ZoomIn => self.animate_zoom_around_viewport_center(ZOOM_STEP),
            ShellCommand::ZoomOut => self.animate_zoom_around_viewport_center(1.0 / ZOOM_STEP),
            ShellCommand::LogAccessibilityTree => {
                crate::accessibility::log_accessibility_tree(self.accessibility_window_snapshot())
            }
        }
        self.request_redraw();
    }

    fn prepare_spawn_position(&mut self) {
        self.next_spawn_position = CanvasPoint {
            x: self.viewport_offset.x
                + (f64::from(self.output_size.w) / 2.0 / self.viewport_scale).round() as i32
                - MIN_WINDOW_WIDTH / 2
                + self.spawn_offset,
            y: self.viewport_offset.y
                + (f64::from(self.output_size.h) / 2.0 / self.viewport_scale).round() as i32
                - 180
                + self.spawn_offset,
        };
        self.spawn_offset = (self.spawn_offset + SPAWN_OFFSET_STEP) % SPAWN_OFFSET_WRAP;
    }

    fn spawn_a11y_test(&mut self) {
        self.prepare_spawn_position();

        let current_exe = match env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("Failed to locate current executable for GTK test app: {error}");
                return;
            }
        };

        let mut command = Command::new(current_exe);
        command
            .arg(GTK_TEST_APP_FLAG)
            .env("WAYLAND_DISPLAY", WAYLAND_DISPLAY_NAME)
            .env("GDK_BACKEND", "wayland");
        apply_gtk_client_environment(&mut command);

        match command.spawn() {
            Ok(_) => {}
            Err(error) => eprintln!("Failed to spawn GTK test app: {error}"),
        }
    }

    fn spawn_foot(&mut self) {
        self.prepare_spawn_position();

        match Command::new("foot")
            .env("WAYLAND_DISPLAY", WAYLAND_DISPLAY_NAME)
            .spawn()
        {
            Ok(_) => {}
            Err(error) => eprintln!("Failed to spawn foot: {error}"),
        }
    }

    fn launch_app(&mut self, app_id: &str) {
        let Some(app) = self.app_catalog.app_by_id(app_id) else {
            eprintln!("No launchable desktop app found for {app_id}");
            return;
        };

        let app_command = match app.launch_argv() {
            Ok(command) => command,
            Err(error) => {
                eprintln!("Failed to build command for {app_id}: {error}");
                return;
            }
        };
        let command = if app.terminal {
            match self.app_catalog.terminal_command_for(app_command) {
                Ok(command) => command,
                Err(error) => {
                    eprintln!("Failed to find terminal for {app_id}: {error}");
                    return;
                }
            }
        } else {
            app_command
        };

        let wayland_display = if let Some(instance_name) = &app.snap_instance_name {
            match ensure_snap_wayland_socket(instance_name) {
                Ok(display_name) => display_name,
                Err(error) => {
                    eprintln!("Failed to prepare Snap Wayland socket for {app_id}: {error}");
                    return;
                }
            }
        } else {
            WAYLAND_DISPLAY_NAME.to_string()
        };

        let launch_env = match launch_environment_for_app(app) {
            Ok(env) => env,
            Err(error) => {
                eprintln!("Failed to prepare app environment for {app_id}: {error}");
                return;
            }
        };
        let launch_env_refs = launch_env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();

        self.prepare_spawn_position();
        if let Err(error) = spawn_argv_with_env(&command, &wayland_display, &launch_env_refs) {
            eprintln!("Failed to launch {app_id}: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::SpawnTarget;

    #[test]
    fn drains_only_complete_newline_terminated_commands() {
        let mut buffer = b"zoom-in\npan-left\n".to_vec();
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::ZoomIn, ShellCommand::PanLeft]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn keeps_an_unterminated_trailing_command_buffered() {
        let mut buffer = b"zoom-in\npan-le".to_vec();
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::ZoomIn]);
        assert_eq!(buffer, b"pan-le");

        // The remainder completes once the newline arrives.
        buffer.extend_from_slice(b"ft\n");
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::PanLeft]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn skips_unparseable_lines_while_draining() {
        let mut buffer = b"not-a-command\nspawn foot\n".to_vec();
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::Spawn(SpawnTarget::Foot)]);
    }

    #[test]
    fn parse_command_line_trims_and_rejects_invalid_utf8() {
        assert_eq!(
            parse_command_line(b"  zoom-out \n"),
            Some(ShellCommand::ZoomOut)
        );
        assert_eq!(parse_command_line(&[0xff, 0xfe]), None);
        assert_eq!(parse_command_line(b"\n"), None);
    }

    #[test]
    fn sanitized_path_component_replaces_unsafe_characters() {
        assert_eq!(
            sanitized_path_component("org.gnome.Calculator"),
            "org.gnome.Calculator"
        );
        // Path separators become underscores; dots are preserved.
        assert_eq!(sanitized_path_component("a/../b"), "a_.._b");
        assert_eq!(sanitized_path_component("weird name!"), "weird_name_");
    }

    #[test]
    fn snap_instance_names_reject_path_separators() {
        assert!(is_valid_snap_instance_name("firefox"));
        assert!(is_valid_snap_instance_name("foo_bar-1.2"));
        assert!(!is_valid_snap_instance_name("../escape"));
        assert!(!is_valid_snap_instance_name("with/slash"));
        assert!(!is_valid_snap_instance_name("space here"));
    }
}
