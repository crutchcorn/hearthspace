//! The tray's window selection control reflects compositor-owned group state.
use serde::Deserialize;
use xilem::{
    WidgetView,
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::{
    command_client::{query_group, send_command},
    icons::{Icon, icon_button},
    state::ShellState,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct GroupState {
    pub(super) selecting: bool,
    pub(super) selected: usize,
    pub(super) gathering: bool,
    pub(super) error: Option<String>,
}

impl GroupState {
    pub(super) fn hint(&self) -> Option<String> {
        if let Some(error) = &self.error {
            Some(error.clone())
        } else if self.gathering {
            Some("Gathering similar windows…".into())
        } else if self.selecting {
            Some("Click or drag to select · Enter done · Esc clear".into())
        } else if self.selected == 1 {
            Some("1 window grouped".into())
        } else if self.selected > 0 {
            Some(format!("{} windows grouped", self.selected))
        } else {
            None
        }
    }
}

pub(super) fn button(group: &GroupState) -> impl WidgetView<ShellState> + use<> {
    icon_button(
        Icon::SelectGroup,
        "Select window group",
        group.selecting,
        |state| {
            state.dismiss_panels();
            state.window_group.error =
                send_command(&state.command_socket, "group-selection-toggle").err();
        },
    )
}

fn parse(bytes: &[u8]) -> Result<GroupState, String> {
    #[derive(Deserialize)]
    struct Snapshot {
        version: u32,
        selecting: bool,
        selected: Vec<u64>,
        gathering: bool,
        error: Option<String>,
    }
    let snapshot: Snapshot = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid window group status: {error}"))?;
    if snapshot.version != 1 || snapshot.selected.contains(&0) {
        return Err("Invalid window group status".into());
    }
    Ok(GroupState {
        selecting: snapshot.selecting,
        selected: snapshot.selected.len(),
        gathering: snapshot.gathering,
        error: snapshot.error,
    })
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Result<GroupState, String>>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            async move {
                loop {
                    let socket = socket.clone();
                    let result = xilem::tokio::task::spawn_blocking(move || {
                        query_group(&socket).and_then(|bytes| parse(&bytes))
                    })
                    .await
                    .unwrap_or_else(|error| Err(format!("Window group query failed: {error}")));
                    if proxy.message(result).is_err() {
                        return;
                    }
                    xilem::tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        },
        |state, result| match result {
            Ok(group) => state.window_group = group,
            // Do not retain a selected appearance if the session disconnected.
            Err(_) => state.window_group = GroupState::default(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_support::Ui;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
    };
    use xilem::view::sized_box;

    fn view(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(button(&state.window_group))
    }

    #[test]
    fn selection_button_sends_command_and_dismisses_other_popups() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("control.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert_eq!(request, "group-selection-toggle\n");
            stream.write_all(b"ok\n").unwrap();
        });
        let mut state = ShellState::new(socket, vec!["eDP-1".into()]);
        state.toggle_map("eDP-1");
        let mut ui = Ui::new(state, view, (48, 48));
        ui.activate(ui.button("Select window group"));
        ui.dispatch(&view);
        assert!(!ui.state.outputs[0].show_map);
        assert!(ui.state.window_group.error.is_none());
        server.join().unwrap();
    }

    #[test]
    fn status_tracks_selection_busy_and_error_without_window_titles() {
        let mut group = parse(
            br#"{"version":1,"selecting":true,"selected":[1,2],"gathering":false,"error":null}"#,
        )
        .unwrap();
        assert!(group.selecting);
        assert_eq!(group.selected, 2);
        assert!(group.hint().unwrap().starts_with("Click or drag"));
        group.gathering = true;
        assert_eq!(group.hint().as_deref(), Some("Gathering similar windows…"));
        group.error = Some("Install D1 in Settings".into());
        assert_eq!(group.hint().as_deref(), Some("Install D1 in Settings"));
        assert!(
            parse(
                br#"{"version":2,"selecting":false,"selected":[],"gathering":false,"error":null}"#
            )
            .is_err()
        );
        assert!(
            parse(
                br#"{"version":1,"selecting":false,"selected":[0],"gathering":false,"error":null}"#
            )
            .is_err()
        );
        assert!(GroupState::default().hint().is_none());
    }
}
