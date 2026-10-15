use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, flex_col, flex_row, label, sized_box},
};

use super::{
    command_client::{launch_app, send_command},
    constants::MAX_RESULTS,
    icons::{Icon, icon_button},
    search_focus::{FocusTarget, focus_on_activation},
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
    pub(super) fn id(&self) -> super::search_selection::EntryId {
        match self {
            Self::Window(window) => super::search_selection::EntryId::Window(window.id),
            Self::App(app) => super::search_selection::EntryId::App(app.id.clone()),
        }
    }

    pub(super) fn label(&self) -> String {
        match self {
            Self::Window(window) => window.label(),
            Self::App(app) => format!("Open: {}", app.name),
        }
    }
}

pub(super) fn results(state: &ShellState, output: &ShellOutputInstance) -> Vec<Entry> {
    let query = output.query.trim();
    if output.drawing.active() {
        return Vec::new();
    }
    if query.is_empty() {
        return state
            .favorites
            .store
            .entries()
            .iter()
            .filter_map(|f| state.catalog.app_by_id(&f.app_id).cloned())
            .map(|app| Entry::App(Box::new(app)))
            .collect();
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
    state: &ShellState,
    output: &ShellOutputInstance,
    entries: Vec<Entry>,
    windows_error: Option<String>,
) -> impl WidgetView<ShellState, Widget = super::search_focus::SearchFocus> + use<> {
    let focus_target = FocusTarget::default();
    let search = super::search_input::input(output, focus_target.clone());
    let close_key = output.key().to_string();
    let close = icon_button(Icon::Close, "Close search", false, move |state| {
        state.dismiss_search(&close_key)
    });
    let selected = super::search_selection::selected_index(output, &entries);
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = if output.drawing.active() {
        vec![super::favorite_view::drawing(output)]
    } else {
        let mut rows = if output.query.trim().is_empty() {
            super::favorite_view::browse(state, output)
        } else {
            Vec::new()
        };
        for (index, entry) in entries.into_iter().enumerate() {
            let shape = if output.query.trim().is_empty() {
                match &entry {
                    Entry::App(app) => state
                        .favorites
                        .store
                        .entries()
                        .iter()
                        .find(|f| f.app_id == app.id)
                        .and_then(|f| f.gesture.as_ref())
                        .map(|g| super::gesture_pad::thumbnail(g.points().to_vec())),
                    _ => None,
                }
            } else {
                None
            };
            rows.push(
                flex_row((
                    super::search_result::row(
                        output,
                        entry,
                        Some(index) == selected,
                        focus_target.clone(),
                    )
                    .flex(1.0),
                    shape,
                ))
                .gap(8.0.px())
                .boxed(),
            );
        }
        if output.query.trim().is_empty() {
            rows.extend(super::favorite_view::missing(state));
        }
        if rows.is_empty() {
            rows.push(label(if output.query.trim().is_empty() {
                "Type to search apps and open windows. Add apps to favorites to assign gestures."
            } else { "No matching apps or windows" }).line_break_mode(LineBreaking::WordWrap).boxed());
        }
        rows
    };
    if !output.drawing.active()
        && let Some(message) = &output.drawing.message
    {
        rows.push(
            label(message.clone())
                .line_break_mode(LineBreaking::WordWrap)
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
        super::popup::panel_with_header(
            output,
            super::popup::PopupKind::Search,
            flex_col((
                flex_row((
                    components::section_title("Search"),
                    FlexSpacer::Flex(1.0),
                    close,
                )),
                (!output.drawing.active()).then_some(search),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(4.0.px()),
        ),
        focus_target,
    )
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
