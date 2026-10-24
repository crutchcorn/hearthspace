use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label, sized_box},
};

use super::{
    command_client::send_command,
    constants::MAX_RESULTS,
    search_focus::{FocusTarget, focus_on_activation},
    state::{ShellOutputInstance, ShellState},
    window_catalog::RunningWindow,
};
use hearthspace_desktop::app_catalog::DesktopApp;

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
            .catalog
            .apps()
            .iter()
            .cloned()
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
    results.extend(
        state
            .catalog
            .search(query, MAX_RESULTS)
            .into_iter()
            .map(|app| Entry::App(Box::new(app))),
    );
    results
}

pub(super) fn select(
    state: &mut ShellState,
    output_key: &str,
    output_name: Option<&str>,
    entry: &Entry,
) {
    let result = match entry {
        Entry::App(app) => {
            super::launch::app(state, output_key, &app.id, output_name, false);
            return;
        }
        Entry::Window(window) => send_command(
            &state.command_socket,
            &format!("focus-window {}", window.id),
        ),
    };
    state.finish_launch(output_key, result);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AppLayout {
    #[default]
    Grid,
    List,
}

#[path = "launcher_controls.rs"]
pub(super) mod controls;

pub(super) fn is_grid(output: &ShellOutputInstance) -> bool {
    output.query.trim().is_empty() && output.launcher_layout == AppLayout::Grid
}

pub(super) fn grid_columns(output: &ShellOutputInstance) -> usize {
    if !is_grid(output) {
        return 1;
    }
    let (padding, gap) = if output.launcher_width <= 480.0 {
        (28.0, 2.0)
    } else {
        (48.0, 12.0)
    };
    ((output.launcher_width - padding + gap) / (100.0 + gap))
        .floor()
        .clamp(1.0, 4.0) as usize
}

pub(super) fn launcher_view(
    state: &ShellState,
    output: &ShellOutputInstance,
    entries: Vec<Entry>,
    windows_error: Option<String>,
) -> impl WidgetView<ShellState, Widget = super::search_focus::SearchFocus> + use<> {
    let focus_target = FocusTarget::default();
    let selected = super::search_selection::selected_index(output, &entries);
    let count = entries.len();
    let mut rows = Vec::<Box<AnyWidgetView<ShellState>>>::new();
    if output.drawing.active() {
        rows.push(super::favorite_view::drawing(state, output));
    } else {
        if entries.is_empty() {
            rows.push(
                sized_box(
                    flex_col((
                        components::icons::icon(components::icons::Icon::Search, 30.0)
                            .theme_color(Tone::Muted),
                        label(if output.query.trim().is_empty() {
                            "No installed apps found"
                        } else {
                            "No matching apps or windows"
                        })
                        .text_size(20.0)
                        .line_break_mode(LineBreaking::WordWrap),
                    ))
                    .gap(12.0.px()),
                )
                .height(204.0.px())
                .boxed(),
            );
        } else if is_grid(output) {
            let columns = grid_columns(output);
            for (row, chunk) in entries.chunks(columns).enumerate() {
                let mut cells = chunk
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(column, entry)| {
                        super::search_result::row(
                            state,
                            output,
                            entry,
                            selected == Some(row * columns + column),
                            focus_target.clone(),
                        )
                        .boxed()
                        .flex(1.0)
                    })
                    .collect::<Vec<_>>();
                for _ in chunk.len()..columns {
                    cells.push(sized_box(label("")).boxed().flex(1.0));
                }
                rows.push(
                    flex_row(cells)
                        .gap(
                            if output.launcher_width <= 480.0 {
                                2.0
                            } else {
                                12.0
                            }
                            .px(),
                        )
                        .boxed(),
                );
            }
        } else {
            rows.extend(entries.into_iter().enumerate().map(|(index, entry)| {
                super::search_result::row(
                    state,
                    output,
                    entry,
                    Some(index) == selected,
                    focus_target.clone(),
                )
                .boxed()
            }));
        }
    }
    // Drawing owns measured geometry until final launch admission. Its pending
    // feedback uses the reserved space below the pad instead of shifting it.
    if output.launch_pending.is_some() && !output.drawing.active() {
        rows.insert(0, components::field_label("Opening application…").boxed());
    }
    if let Some(error) = output.launch_error.as_ref().or(windows_error.as_ref()) {
        rows.insert(
            0,
            sized_box(
                components::field_label(format!("Could not complete the request: {error}"))
                    .line_break_mode(LineBreaking::WordWrap),
            )
            .theme_background(Tone::Danger)
            .padding(8.0.px())
            .boxed(),
        );
    }
    let output_key = output.key().to_owned();
    let horizontal_padding = if output.launcher_width <= 480.0 {
        14.0
    } else {
        24.0
    }
    .px();
    let collection = (!output.drawing.active()).then(|| {
        sized_box(controls::collection(output, count)).padding(Padding {
            top: 12.0.px(),
            bottom: 4.0.px(),
            ..Padding::horizontal(horizontal_padding)
        })
    });
    let footer = (!output.drawing.active()).then(|| {
        sized_box(
            sized_box(
                flex_row((
                    components::icons::icon(components::icons::Icon::CornerDownLeft, 14.0)
                        .theme_color(Tone::Muted),
                    label("Enter to open")
                        .text_size(10.0)
                        .theme_color(Tone::Muted),
                ))
                .gap(6.0.px()),
            )
            .height(42.0.px())
            .theme_border(Tone::Border)
            .border_width(1.0.px()),
        )
        .padding(Padding {
            bottom: 12.0.px(),
            ..Padding::horizontal(horizontal_padding)
        })
    });
    let panel = super::popup::launcher_panel(
        output,
        flex_col((controls::header(output, focus_target.clone()), collection))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(0.0.px()),
        sized_box(
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(4.0.px()),
        )
        .padding(Padding {
            top: if output.drawing.active() { 12.0 } else { 0.0 }.px(),
            bottom: if output.drawing.active() { 12.0 } else { 4.0 }.px(),
            ..Padding::horizontal(horizontal_padding)
        }),
        footer,
    );
    focus_on_activation(
        xilem::view::resize_observer(
            move |state: &mut ShellState, size| {
                if let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|output| output.key() == output_key)
                {
                    output.launcher_width = size.width;
                }
            },
            panel,
        ),
        focus_target,
        output.key().to_owned(),
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
