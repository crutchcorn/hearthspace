use std::{env, io::Write, os::unix::net::UnixStream, path::PathBuf};

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, IntoElement, Window,
    WindowBounds, WindowDecorations, WindowOptions,
};

use crate::{
    config::{CONTROL_BAR_HEIGHT, SHELL_BAR_APP_ID, SHELL_COMMAND_SOCKET_ENV},
    shell::{ShellCommand, SpawnTarget},
};

struct ShellBar {
    command_socket: PathBuf,
    spawn_menu_open: bool,
}

impl Render for ShellBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap_2()
            .size_full()
            .px_2()
            .bg(rgb(0x191e29))
            .text_color(rgb(0xeaf2ff));

        row = row.child(spawn_dropdown(
            self.command_socket.clone(),
            self.spawn_menu_open,
            cx,
        ));

        for command in [
            ShellCommand::PanLeft,
            ShellCommand::PanRight,
            ShellCommand::PanUp,
            ShellCommand::PanDown,
            ShellCommand::ZoomIn,
            ShellCommand::ZoomOut,
            ShellCommand::LogAccessibilityTree,
        ] {
            let command_socket = self.command_socket.clone();
            row = row.child(shell_button(
                command.wire_name(),
                command.label(),
                move || {
                    send_command(&command_socket, command);
                },
            ));
        }

        row
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command_socket = env::var_os(SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{SHELL_COMMAND_SOCKET_ENV} is not set"))?;

    Application::new().run(move |cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(760.0), px(CONTROL_BAR_HEIGHT as f32)),
                    cx,
                ))),
                titlebar: None,
                focus: false,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                app_id: Some(SHELL_BAR_APP_ID.to_string()),
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| ShellBar {
                    command_socket: command_socket.clone(),
                    spawn_menu_open: false,
                })
            },
        )
        .unwrap();
    });

    Ok(())
}

fn spawn_dropdown(
    command_socket: PathBuf,
    is_open: bool,
    cx: &mut Context<ShellBar>,
) -> impl IntoElement {
    let mut group = div().flex().items_center().gap_1().flex_none();

    group = group.child(
        div()
            .id("spawn-dropdown")
            .flex_none()
            .px_3()
            .py_1()
            .rounded_sm()
            .bg(rgb(0x43506d))
            .hover(|this| this.bg(rgb(0x506080)))
            .active(|this| this.opacity(0.82))
            .cursor_pointer()
            .child(if is_open { "SPAWN ^" } else { "SPAWN v" })
            .on_click(cx.listener(|bar, _, _, cx| {
                bar.spawn_menu_open = !bar.spawn_menu_open;
                cx.notify();
            })),
    );

    if is_open {
        for target in SpawnTarget::ALL {
            let command = ShellCommand::Spawn(target);
            let command_socket = command_socket.clone();
            group = group.child(shell_button(
                target.wire_name(),
                target.label(),
                move || {
                    send_command(&command_socket, command);
                },
            ));
        }
    }

    group
}

fn shell_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn() + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .px_3()
        .py_1()
        .rounded_sm()
        .bg(rgb(0x30394d))
        .hover(|this| this.bg(rgb(0x3a4660)))
        .active(|this| this.opacity(0.82))
        .cursor_pointer()
        .child(label)
        .on_click(move |_, _, _| on_click())
}

fn send_command(command_socket: &PathBuf, command: ShellCommand) {
    match UnixStream::connect(command_socket).and_then(|mut stream| {
        stream.write_all(command.wire_name().as_bytes())?;
        stream.write_all(b"\n")
    }) {
        Ok(()) => {}
        Err(error) => eprintln!("failed to send shell command {:?}: {error}", command),
    }
}
