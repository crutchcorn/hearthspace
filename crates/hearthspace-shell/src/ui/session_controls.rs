//! Small everyday session controls, kept separate from application search.
use std::time::Duration;

use chrono::Local;
use hearthspace_ui_components as components;
use xilem::{
    WidgetView,
    core::{MessageProxy, NoElement, View},
    masonry::layout::AsUnit,
    style::Style,
    view::{flex_row, task_raw},
};

use super::{
    command_client::send_command,
    state::{ShellOutputInstance, ShellState},
};

pub(super) fn current_clock() -> String {
    Local::now().format("%a %b %-d  %H:%M").to_string()
}

pub(super) fn clock_task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<String>, _state: &mut ShellState| async move {
            let mut previous = String::new();
            loop {
                let next = current_clock();
                if next != previous {
                    previous = next.clone();
                    if proxy.message(next).is_err() {
                        break;
                    }
                }
                xilem::tokio::time::sleep(Duration::from_secs(1)).await;
            }
        },
        |state: &mut ShellState, value| state.clock = value,
    )
}

fn set_confirmation(state: &mut ShellState, output_key: &str, enabled: bool) {
    if let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == output_key)
    {
        output.confirm_logout = enabled;
        output.launch_error = None;
    }
}

fn confirm_logout(state: &mut ShellState, output_key: &str) {
    if !state
        .outputs
        .iter()
        .any(|output| output.key() == output_key && output.confirm_logout)
    {
        return;
    }
    if let Err(error) = send_command(&state.command_socket, "quit") {
        state.finish_launch(output_key, Err(error));
    }
}

pub(super) fn session_controls(
    output: &ShellOutputInstance,
    clock: String,
) -> impl WidgetView<ShellState> + use<> {
    let output_key = output.key().to_string();
    let cancel_key = output_key.clone();
    let lock_key = output_key.clone();
    let confirming = output.confirm_logout;
    let logout = components::compact_action_button(
        if confirming {
            "Confirm log out"
        } else {
            "Log out"
        },
        move |state: &mut ShellState| {
            if confirming {
                confirm_logout(state, &output_key);
            } else {
                set_confirmation(state, &output_key, true);
            }
        },
    );
    let cancel = confirming.then(|| {
        components::compact_action_button("Cancel", move |state: &mut ShellState| {
            set_confirmation(state, &cancel_key, false);
        })
    });
    let lock = (!confirming).then(|| {
        components::compact_action_button("Lock", move |state: &mut ShellState| {
            if let Err(error) = send_command(&state.command_socket, "lock-session") {
                state.finish_launch(&lock_key, Err(error));
            }
        })
    });
    flex_row((components::muted_text(clock), lock, logout, cancel)).gap(8.0.px())
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
    fn logout_requires_confirmation_and_reports_a_failed_request() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("control.sock");
        let mut state = ShellState::new(socket.clone(), vec!["eDP-1".into()]);
        // An unarmed request must not even try the missing socket.
        confirm_logout(&mut state, "eDP-1");
        assert!(state.outputs[0].launch_error.is_none());
        set_confirmation(&mut state, "eDP-1", true);
        state.dismiss_launcher("eDP-1");
        assert!(!state.outputs[0].confirm_logout);

        let listener = UnixListener::bind(socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert_eq!(request, "quit\n");
            stream.write_all(b"err session unavailable\n").unwrap();
        });
        set_confirmation(&mut state, "eDP-1", true);
        confirm_logout(&mut state, "eDP-1");
        assert_eq!(
            state.outputs[0].launch_error.as_deref(),
            Some("session unavailable")
        );
        set_confirmation(&mut state, "eDP-1", false);
        assert!(!state.outputs[0].confirm_logout);
        assert!(state.outputs[0].launch_error.is_none());
        server.join().unwrap();
    }
}
