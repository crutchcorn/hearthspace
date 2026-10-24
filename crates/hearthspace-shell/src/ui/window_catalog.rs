//! Live compositor metadata for finding running applications from the launcher.
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::{command_client::query_windows, state::ShellState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RunningWindow {
    pub(super) id: u64,
    pub(super) title: String,
    pub(super) app_id: String,
    pub(super) focused: bool,
}

impl RunningWindow {
    pub(super) fn matches(&self, query: &str) -> bool {
        let text = format!("{} {}", self.title, self.app_id).to_lowercase();
        query
            .to_lowercase()
            .split_whitespace()
            .all(|term| text.contains(term))
    }

    pub(super) fn label(&self) -> String {
        let name = if !self.title.is_empty() {
            &self.title
        } else if !self.app_id.is_empty() {
            &self.app_id
        } else {
            "Untitled window"
        };
        format!("Window: {name}")
    }
}

fn parse_windows(bytes: &[u8]) -> Result<Vec<RunningWindow>, String> {
    Ok(hearthspace_ipc::windows::WindowList::decode(bytes)?
        .windows
        .into_iter()
        .map(|window| RunningWindow {
            id: window.id,
            title: window.title.unwrap_or_default(),
            app_id: window.app_id.unwrap_or_default(),
            focused: window.focused,
        })
        .collect())
}

pub(super) fn window_catalog_task()
-> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        move |proxy: MessageProxy<Result<Vec<RunningWindow>, String>>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            async move {
                let mut previous = None;
                loop {
                    let socket = socket.clone();
                    let result = xilem::tokio::task::spawn_blocking(move || {
                        query_windows(&socket).and_then(|bytes| parse_windows(&bytes))
                    })
                    .await
                    .unwrap_or_else(|error| Err(format!("Window query failed: {error}")));
                    if previous.as_ref() != Some(&result) {
                        previous = Some(result.clone());
                        if proxy.message(result).is_err() {
                            break;
                        }
                    }
                    xilem::tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        },
        apply_window_catalog,
    )
}

fn apply_window_catalog(state: &mut ShellState, result: Result<Vec<RunningWindow>, String>) {
    match result {
        Ok(windows) => {
            state.windows = windows;
            state.windows_error = None;
        }
        Err(error) => {
            // A slow capture or busy compositor must not remove a visible
            // selection just before Enter. Keep the last confirmed IDs while
            // showing the error; the compositor validates each focus request.
            state.windows_error = Some(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_windows_match_live_title_and_application_with_stable_ids() {
        let windows = parse_windows(br#"{"version":1,"windows":[{"id":42,"title":"Project Notes","app_id":"org.editor","focused":true},{"id":43,"title":null,"app_id":null}]}"#).unwrap();
        assert_eq!(windows[0].id, 42);
        assert!(windows[0].matches("EDITOR notes"));
        assert!(!windows[0].matches("missing"));
        assert_eq!(windows[1].label(), "Window: Untitled window");
        assert!(parse_windows(br#"{"version":1,"windows":[{"id":0}]}"#).is_err());
        assert!(parse_windows(br#"{"wrong":[]}"#).is_err());
    }

    #[test]
    fn transient_failure_preserves_visible_selection_until_a_successful_refresh() {
        use crate::ui::launcher::{self, Entry};

        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        state.catalog = Default::default();
        state.outputs[0].query = "Notes".into();
        apply_window_catalog(&mut state, Err("initial connection failed".into()));
        assert!(launcher::results(&state, &state.outputs[0]).is_empty());

        let windows = parse_windows(
            br#"{"version":1,"windows":[{"id":42,"title":"Notes","app_id":"editor"}]}"#,
        )
        .unwrap();
        apply_window_catalog(&mut state, Ok(windows));
        apply_window_catalog(&mut state, Err("capture delayed query".into()));
        assert!(
            matches!(launcher::results(&state, &state.outputs[0]).as_slice(), [Entry::Window(window)] if window.id == 42)
        );
        assert!(state.windows_error.is_some());

        state.change_query("eDP-1", String::new());
        assert!(launcher::results(&state, &state.outputs[0]).is_empty());
        apply_window_catalog(&mut state, Ok(Vec::new()));
        assert!(state.windows.is_empty());
        assert!(state.windows_error.is_none());
    }
}
