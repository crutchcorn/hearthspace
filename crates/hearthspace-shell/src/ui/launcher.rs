use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        parley::fontique::GenericFamily,
        properties::LineBreaking,
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, FlexSpacer, button, flex_col, flex_row, sized_box, text_input,
    },
};

use super::{
    command_client::{launch_app, send_command},
    constants::{MAX_RESULTS, SEARCH_BOX_HEIGHT},
    icons::{Icon, icon_button},
    search_focus::{FocusTarget, focus_on_activation, track_search},
    state::{ShellOutputInstance, ShellState},
    window_catalog::RunningWindow,
};
use crate::app_catalog::DesktopApp;

#[derive(Clone)]
pub(super) enum Entry {
    Window(RunningWindow),
    App(Box<DesktopApp>),
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
    if query.is_empty() {
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
                .map(|app| Entry::App(Box::new(app))),
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
) -> impl WidgetView<ShellState, Widget = super::search_focus::SearchFocus> + use<> {
    let focus_target = FocusTarget::default();
    let search = search_input(output, focus_target.clone());
    let close_key = output.key().to_string();
    let close = icon_button(Icon::Close, "Close search", false, move |state| {
        state.dismiss_search(&close_key)
    });
    let key = output.key().to_string();
    let output_name = output.output_name.clone();
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = entries
        .into_iter()
        .map(|entry| {
            let key = key.clone();
            let output_name = output_name.clone();
            button(
                components::field_label(entry.label())
                    .text_size(16.0)
                    .line_break_mode(LineBreaking::Clip)
                    .width(Dim::Stretch),
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
                "Type to search apps and open windows"
            } else {
                "No matching apps or windows"
            })
            .boxed(),
        );
    }
    if let Some(error) = output.launch_error.as_ref().or(windows_error.as_ref()) {
        rows.insert(
            0,
            sized_box(
                components::field_label(format!("Could not complete the request: {error}"))
                    .line_break_mode(LineBreaking::WordWrap),
            )
            .background_color(components::DANGER_SUBTLE)
            .padding(8.0.px())
            .boxed(),
        );
    }
    focus_on_activation(
        sized_box(
            flex_col((
                flex_row((
                    components::section_title("Search"),
                    FlexSpacer::Flex(1.0),
                    close,
                )),
                search,
                components::scroll_area(
                    flex_col(rows)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .gap(4.0.px()),
                )
                .flex(1.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        )
        .background_color(components::SURFACE)
        .padding(12.0.px())
        .border(components::BORDER, 1.0.px())
        .corner_radius(10.0.px()),
        focus_target,
    )
}

fn search_input(
    output: &ShellOutputInstance,
    focus_target: FocusTarget,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let submit_key = key.clone();
    let cancel_key = key.clone();
    let output_name = output.output_name.clone();
    let search = text_input(
        output.query.clone(),
        move |state: &mut ShellState, value: String| {
            state.change_query(&key, value);
        },
    )
    .placeholder("Search apps and windows...")
    .font(GenericFamily::SystemUi)
    .on_enter(move |state: &mut ShellState, value: String| {
        state.change_query(&submit_key, value);
        let entry = state
            .outputs
            .iter()
            .find(|output| output.key() == submit_key)
            .and_then(|output| results(state, output).into_iter().next());
        if let Some(entry) = entry {
            select(state, &submit_key, output_name.as_deref(), &entry);
        }
    })
    .on_cancel(move |state: &mut ShellState, _| state.dismiss_search(&cancel_key));
    sized_box(track_search(search, focus_target)).fixed_height(SEARCH_BOX_HEIGHT.px())
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
    fn empty_search_does_not_list_running_windows() {
        let mut state = state();
        assert!(results(&state, &state.outputs[0]).is_empty());
        state.toggle_search("eDP-1");
        assert!(results(&state, &state.outputs[0]).is_empty());
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
            state.outputs[0].show_search = true;
            let entry = Entry::Window(state.windows[0].clone());
            select(&mut state, "eDP-1", Some("eDP-1"), &entry);
            assert_eq!(state.outputs[0].query.is_empty(), accepted);
            assert_eq!(!state.outputs[0].show_search, accepted);
            assert_eq!(state.outputs[0].launch_error.is_none(), accepted);
            server.join().unwrap();
        }
    }
}
