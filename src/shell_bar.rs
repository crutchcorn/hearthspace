use std::{env, io::Write, os::unix::net::UnixStream, path::PathBuf};

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, FocusHandle, Focusable,
    IntoElement, KeyDownEvent, SharedString, Window, WindowBounds, WindowDecorations,
    WindowOptions,
};

use crate::{
    app_catalog::{AppCatalog, DesktopApp},
    config::{CONTROL_BAR_HEIGHT, SHELL_BAR_APP_ID, SHELL_COMMAND_SOCKET_ENV},
    shell::ShellCommand,
};

struct ShellBar {
    command_socket: PathBuf,
    catalog: AppCatalog,
    query: String,
    selected_result: usize,
    focus_handle: FocusHandle,
}

impl Focusable for ShellBar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ShellBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap_2()
            .size_full()
            .px_2()
            .bg(rgb(0x191e29))
            .text_color(rgb(0xeaf2ff))
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(Self::on_key_down));

        row = row.child(self.search_box(window, cx));

        for app in self.search_results() {
            row = row.child(app_result_button(self.command_socket.clone(), app, cx));
        }

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
            let id = command.wire_name();
            let label = command.label();
            row = row.child(shell_button(id, label, move || {
                send_command(&command_socket, &command.wire_name());
            }));
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
                focus: true,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                app_id: Some(SHELL_BAR_APP_ID.to_string()),
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| ShellBar {
                    command_socket: command_socket.clone(),
                    catalog: AppCatalog::load(),
                    query: String::new(),
                    selected_result: 0,
                    focus_handle: cx.focus_handle(),
                })
            },
        )
        .unwrap();
    });

    Ok(())
}

impl ShellBar {
    fn search_box(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle(cx).is_focused(window);
        let text = if self.query.is_empty() {
            "Search apps...".to_string()
        } else {
            self.query.clone()
        };

        div()
            .id("app-search")
            .flex()
            .items_center()
            .h(px(32.0))
            .w(px(240.0))
            .px_3()
            .rounded_sm()
            .border_1()
            .border_color(if is_focused {
                rgb(0x8fb4ff)
            } else {
                rgb(0x30394d)
            })
            .bg(if is_focused {
                rgb(0x24304a)
            } else {
                rgb(0x202636)
            })
            .text_color(if self.query.is_empty() {
                rgb(0x8f9ab1)
            } else {
                rgb(0xeaf2ff)
            })
            .cursor_pointer()
            .child(text)
            .on_click(cx.listener(|bar, _, window, cx| {
                window.focus(&bar.focus_handle(cx));
                cx.notify();
            }))
    }

    fn search_results(&self) -> Vec<DesktopApp> {
        if self.query.trim().is_empty() {
            return Vec::new();
        }

        self.catalog.search(&self.query, 4)
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        match key {
            "backspace" => {
                self.query.pop();
                self.selected_result = 0;
                cx.notify();
            }
            "escape" => {
                self.query.clear();
                self.selected_result = 0;
                cx.notify();
            }
            "enter" => {
                if let Some(app) = self.search_results().get(self.selected_result) {
                    send_launch_command(&self.command_socket, &app.id);
                    self.query.clear();
                    self.selected_result = 0;
                    cx.notify();
                }
            }
            "down" => {
                let result_count = self.search_results().len();
                if result_count > 0 {
                    self.selected_result = (self.selected_result + 1).min(result_count - 1);
                    cx.notify();
                }
            }
            "up" => {
                self.selected_result = self.selected_result.saturating_sub(1);
                cx.notify();
            }
            _ => {
                if event.keystroke.modifiers.control
                    || event.keystroke.modifiers.alt
                    || event.keystroke.modifiers.platform
                {
                    return;
                }
                if let Some(key_char) = &event.keystroke.key_char {
                    if !key_char.chars().all(char::is_control) {
                        self.query.push_str(key_char);
                        self.selected_result = 0;
                        cx.notify();
                    }
                }
            }
        }
    }
}

fn shell_button(
    id: String,
    label: &'static str,
    on_click: impl Fn() + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
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

fn app_result_button(
    command_socket: PathBuf,
    app: DesktopApp,
    cx: &mut Context<ShellBar>,
) -> impl IntoElement {
    let app_id = app.id.clone();
    let element_id = format!("app-result-{app_id}");
    div()
        .id(SharedString::from(element_id))
        .flex_none()
        .max_w(px(180.0))
        .px_3()
        .py_1()
        .rounded_sm()
        .bg(rgb(0x273553))
        .hover(|this| this.bg(rgb(0x34476f)))
        .active(|this| this.opacity(0.82))
        .cursor_pointer()
        .child(app.name)
        .on_click(cx.listener(move |bar, _, _, cx| {
            send_launch_command(&command_socket, &app_id);
            bar.query.clear();
            bar.selected_result = 0;
            cx.notify();
        }))
}

fn send_launch_command(command_socket: &PathBuf, app_id: &str) {
    send_command(
        command_socket,
        &ShellCommand::LaunchApp(app_id.to_string()).wire_name(),
    );
}

fn send_command(command_socket: &PathBuf, command: &str) {
    match UnixStream::connect(command_socket).and_then(|mut stream| {
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")
    }) {
        Ok(()) => {}
        Err(error) => eprintln!("failed to send shell command {command:?}: {error}"),
    }
}
