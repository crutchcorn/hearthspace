//! Bounded, asynchronous access to the session's notification inbox.
use std::{path::Path, time::Duration};

use serde_json::{Value, json};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::state::ShellState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Notification {
    pub(super) id: u32,
    pub(super) app_name: String,
    pub(super) summary: String,
    pub(super) body: String,
    pub(super) actions: Vec<NotificationAction>,
    pub(super) urgency: u8,
    pub(super) active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NotificationAction {
    pub(super) key: String,
    pub(super) label: String,
}

fn request(socket: &Path, request: Value) -> Result<Value, String> {
    let response = hearthspace_ipc::transport::request_line(
        socket,
        &format!("{request}\n"),
        Duration::from_millis(250),
    )
    .map_err(|error| format!("Notifications unavailable: {error}"))?;
    let response: Value = serde_json::from_str(&response)
        .map_err(|error| format!("Invalid notification response: {error}"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Notification request failed")
            .to_string());
    }
    Ok(response)
}

fn parse_notifications(response: &Value) -> Result<Vec<Notification>, String> {
    let items = response
        .get("notifications")
        .and_then(Value::as_array)
        .filter(|items| items.len() <= 24)
        .ok_or("Invalid notification list")?;
    items
        .iter()
        .map(|item| {
            let text = |key: &str| {
                item.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let actions = item
                .get("actions")
                .and_then(Value::as_array)
                .filter(|actions| actions.len() <= 4)
                .ok_or("Invalid notification actions")?
                .iter()
                .map(|action| {
                    Ok(NotificationAction {
                        key: action
                            .get("key")
                            .and_then(Value::as_str)
                            .ok_or("Missing action key")?
                            .to_string(),
                        label: action
                            .get("label")
                            .and_then(Value::as_str)
                            .ok_or("Missing action label")?
                            .to_string(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Notification {
                id: item
                    .get("id")
                    .and_then(Value::as_u64)
                    .and_then(|id| u32::try_from(id).ok())
                    .filter(|id| *id > 0)
                    .ok_or("Invalid notification ID")?,
                app_name: text("app_name"),
                summary: text("summary"),
                body: text("body"),
                actions,
                urgency: item
                    .get("urgency")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .min(2) as u8,
                active: item
                    .get("active")
                    .and_then(Value::as_bool)
                    .ok_or("Invalid notification state")?,
            })
        })
        .collect()
}

pub(super) fn notification_task()
-> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Result<Vec<Notification>, String>>, state: &mut ShellState| {
            let socket = state.notifications_socket.clone();
            async move {
                let mut previous = None;
                loop {
                    let socket = socket.clone();
                    let result = xilem::tokio::task::spawn_blocking(move || {
                        let socket =
                            socket.ok_or("Notification service has no runtime directory")?;
                        request(&socket, json!({"command":"list"}))
                            .and_then(|value| parse_notifications(&value))
                    })
                    .await
                    .unwrap_or_else(|error| Err(format!("Notification query failed: {error}")));
                    if previous.as_ref() != Some(&result) {
                        previous = Some(result.clone());
                        if proxy.message(result).is_err() {
                            break;
                        }
                    }
                    xilem::tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        },
        |state: &mut ShellState, result| match result {
            Ok(items) => {
                state.notifications = items;
                state.notifications_error = None;
            }
            Err(error) => {
                state.notifications.clear();
                state.notifications_error = Some(error);
            }
        },
    )
}

pub(super) fn dismiss(state: &mut ShellState, id: u32) {
    perform(state, json!({"command":"dismiss","id":id}));
}

pub(super) fn invoke(state: &mut ShellState, id: u32, key: &str) {
    perform(state, json!({"command":"action","id":id,"key":key}));
}

fn perform(state: &mut ShellState, command: Value) {
    let result = state
        .notifications_socket
        .as_ref()
        .ok_or_else(|| "Notification service has no runtime directory".to_string())
        .and_then(|socket| request(socket, command));
    state.notifications_error = result.err();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        thread,
    };

    #[test]
    fn inbox_preserves_active_and_history_states_and_action_keys() {
        let value = json!({"notifications":[
            {"id":7,"app_name":"Mail","summary":"New mail","body":"Hello 🌍", "actions":[{"key":"read","label":"Read"}],"urgency":2,"active":true},
            {"id":8,"app_name":"Editor","summary":"Saved","body":"", "actions":[],"active":false}
        ]});
        let items = parse_notifications(&value).unwrap();
        assert!(items[0].active);
        assert_eq!(items[0].actions[0].key, "read");
        assert_eq!(items[0].body, "Hello 🌍");
        assert!(!items[1].active);
        assert!(parse_notifications(&json!({"notifications":[{"id":0}]})).is_err());
    }

    #[test]
    fn action_request_preserves_opaque_keys_and_reports_daemon_rejection() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("notify.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut command = String::new();
            stream.read_to_string(&mut command).unwrap();
            let command: Value = serde_json::from_str(&command).unwrap();
            assert_eq!(
                command,
                json!({"command":"action","id":7,"key":"read\nmessage"})
            );
            stream
                .write_all(b"{\"ok\":false,\"error\":\"notification expired\"}\n")
                .unwrap();
        });
        let error = request(
            &socket,
            json!({"command":"action","id":7,"key":"read\nmessage"}),
        )
        .unwrap_err();
        assert_eq!(error, "notification expired");
        server.join().unwrap();
    }
}
