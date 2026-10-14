//! Live compositor metadata for finding running applications from the launcher.
use serde_json::Value;
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
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid window list: {error}"))?;
    let windows = value
        .get("windows")
        .and_then(Value::as_array)
        .ok_or("Invalid window list")?;
    windows
        .iter()
        .map(|window| {
            Ok(RunningWindow {
                id: window
                    .get("id")
                    .and_then(Value::as_u64)
                    .filter(|id| *id > 0)
                    .ok_or("Invalid window ID")?,
                title: window
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                app_id: window
                    .get("app_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                focused: window
                    .get("focused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
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
        |state: &mut ShellState, result: Result<Vec<RunningWindow>, String>| match result {
            Ok(windows) => {
                state.windows = windows;
                state.windows_error = None;
            }
            Err(error) => {
                state.windows.clear();
                state.windows_error = Some(error);
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_windows_match_live_title_and_application_with_stable_ids() {
        let windows = parse_windows(br#"{"windows":[{"id":42,"title":"Project Notes","app_id":"org.editor","focused":true},{"id":43,"title":null,"app_id":null}]}"#).unwrap();
        assert_eq!(windows[0].id, 42);
        assert!(windows[0].matches("EDITOR notes"));
        assert!(!windows[0].matches("missing"));
        assert_eq!(windows[1].label(), "Window: Untitled window");
        assert!(parse_windows(br#"{"windows":[{"id":0}]}"#).is_err());
        assert!(parse_windows(br#"{"wrong":[]}"#).is_err());
    }
}
