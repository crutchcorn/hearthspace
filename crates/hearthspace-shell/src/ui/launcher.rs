use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, flex_col, sized_box},
};

use super::{
    command_client::{launch_app, send_command},
    constants::{LAUNCHER_ITEM_WIDTH, MAX_RESULTS},
    state::{ShellOutputInstance, ShellState},
    window_catalog::RunningWindow,
};
use crate::app_catalog::DesktopApp;

#[derive(Clone)]
pub(super) enum Entry {
    Window(RunningWindow),
    App(DesktopApp),
}

impl Entry {
    fn label(&self) -> String {
        match self {
            Self::Window(window) => window.label(),
            Self::App(app) => format!("Open: {}", app.name),
        }
    }
}

pub(super) fn results(state: &ShellState, output: &ShellOutputInstance) -> Vec<Entry> {
    let query = output.query.trim();
    if query.is_empty() && !output.show_windows {
        return Vec::new();
    }
    let mut windows = state
        .windows
        .iter()
        .filter(|window| window.matches(query))
        .cloned()
        .collect::<Vec<_>>();
    windows.sort_by_key(|window| (!window.focused, window.id));
    let mut results = windows
        .into_iter()
        .take(MAX_RESULTS)
        .map(Entry::Window)
        .collect::<Vec<_>>();
    if !query.is_empty() {
        results.extend(
            state
                .catalog
                .search(query, MAX_RESULTS)
                .into_iter()
                .map(Entry::App),
        );
    }
    results
}

pub(super) fn select(
    state: &mut ShellState,
    output_key: &str,
    output_name: Option<&str>,
    entry: &Entry,
) {
    let result = match entry {
        Entry::App(app) => launch_app(&state.command_socket, &app.id, output_name),
        Entry::Window(window) => send_command(
            &state.command_socket,
            &format!("focus-window {}", window.id),
        ),
    };
    state.finish_launch(output_key, result);
}

pub(super) fn launcher_view(
    output: &ShellOutputInstance,
    entries: Vec<Entry>,
    windows_error: Option<String>,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let output_name = output.output_name.clone();
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = entries
        .into_iter()
        .map(|entry| {
            let key = key.clone();
            let output_name = output_name.clone();
            components::launcher_item(
                entry.label(),
                LAUNCHER_ITEM_WIDTH,
                move |state: &mut ShellState| {
                    select(state, &key, output_name.as_deref(), &entry);
                },
            )
            .boxed()
        })
        .collect();
    if rows.is_empty() {
        rows.push(
            components::muted_text(if output.query.trim().is_empty() {
                "No running windows"
            } else {
                "No matching apps or windows"
            })
            .boxed(),
        );
    }
    if output.show_windows && output.query.trim().is_empty() {
        rows.push(components::muted_text("Type a title or app name to find more windows.").boxed());
    }
    if let Some(error) = output.launch_error.as_ref().or(windows_error.as_ref()) {
        rows.insert(
            0,
            sized_box(components::field_label(format!(
                "Could not complete the request: {error}"
            )))
            .background_color(components::DANGER_SUBTLE)
            .padding(8.0.px())
            .boxed(),
        );
    }
    sized_box(
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(4.0.px()),
    )
    .background_color(components::SURFACE)
    .padding(6.0.px())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn state() -> ShellState {
        let mut state = ShellState::new(PathBuf::from("/tmp/unused"), vec!["eDP-1".into()]);
        state.catalog = Default::default();
        state.windows = vec![
            RunningWindow {
                id: 42,
                title: "Research Notes".into(),
                app_id: "editor".into(),
                focused: false,
            },
            RunningWindow {
                id: 43,
                title: "Browser".into(),
                app_id: "firefox".into(),
                focused: true,
            },
        ];
        state
    }

    #[test]
    fn window_search_uses_live_titles_and_never_silently_selects_an_unmatched_window() {
        let mut state = state();
        state.outputs[0].query = "EDITOR research".into();
        let entries = results(&state, &state.outputs[0]);
        assert!(matches!(entries.as_slice(), [Entry::Window(window)] if window.id == 42));
        state.outputs[0].query = "unmatched".into();
        assert!(results(&state, &state.outputs[0]).is_empty());
    }

    #[test]
    fn windows_button_lists_running_windows_with_the_focused_window_first() {
        let mut state = state();
        assert!(results(&state, &state.outputs[0]).is_empty());
        state.outputs[0].show_windows = true;
        let entries = results(&state, &state.outputs[0]);
        assert!(
            matches!(entries.as_slice(), [Entry::Window(first), Entry::Window(second)] if first.id == 43 && second.id == 42)
        );
    }

    #[test]
    fn selecting_a_window_uses_its_id_and_preserves_query_when_it_has_closed() {
        use std::{io::Read, io::Write, os::unix::net::UnixListener, thread};

        for (reply, accepted) in [("ok\n", true), ("err unknown window ID 42\n", false)] {
            let mut state = state();
            let dir = tempfile::tempdir().unwrap();
            state.command_socket = dir.path().join("control.sock");
            let listener = UnixListener::bind(&state.command_socket).unwrap();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                stream.read_to_string(&mut request).unwrap();
                assert_eq!(request, "focus-window 42\n");
                stream.write_all(reply.as_bytes()).unwrap();
            });
            state.outputs[0].query = "Research".into();
            state.outputs[0].show_windows = true;
            let entry = Entry::Window(state.windows[0].clone());
            select(&mut state, "eDP-1", Some("eDP-1"), &entry);
            assert_eq!(state.outputs[0].query.is_empty(), accepted);
            assert_eq!(!state.outputs[0].show_windows, accepted);
            assert_eq!(state.outputs[0].launch_error.is_none(), accepted);
            server.join().unwrap();
        }
    }
}
