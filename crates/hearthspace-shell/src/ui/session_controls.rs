//! Small everyday session controls, kept separate from application search.
use std::time::Duration;

use chrono::Local;
#[cfg(test)]
use xilem::WidgetView;
use xilem::{
    WindowView,
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

#[cfg(test)]
use super::icons::{Icon, icon_button};
use super::{
    command_client::send_command,
    popup,
    state::{ShellOutputInstance, ShellState},
};

mod power_confirmation;
mod power_dialog;
mod power_view;
pub(super) use power_confirmation::Confirmation;
#[cfg(test)]
pub(super) use power_view::size as power_size;
pub(super) use power_view::{PowerUiState, panel as power_panel};

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

#[cfg(test)]
pub(super) fn power_button(output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    icon_button(Icon::Power, "Power", output.show_power, move |state| {
        state.toggle_power(&key)
    })
}

pub(super) fn power_window(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> WindowView<ShellState> {
    let (id, app_id) = if output.power_confirmation.is_some() {
        (
            output.power_confirm_window_id,
            output
                .output_name
                .as_deref()
                .map(hearthspace_common::power_confirm_app_id_for_output)
                .unwrap_or_else(|| hearthspace_common::POWER_CONFIRM_APP_ID.to_owned()),
        )
    } else {
        (output.power_window_id, output.power_app_id())
    };
    let key = output.key().to_string();
    let size = power_view::size(state, output);
    popup::window(
        id,
        "Hearthspace Power",
        app_id,
        size,
        power_panel(state, output),
        move |state| state.dismiss_power(&key),
    )
}

#[cfg(test)]
#[path = "power_view_tests.rs"]
mod view_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
#[path = "power_confirmation_tests.rs"]
mod confirmation_tests;

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
        power_confirmation::confirm(&mut state, "eDP-1", Confirmation::LogOff);
        assert!(state.outputs[0].launch_error.is_none());
        state.toggle_power("eDP-1");
        power_confirmation::open(&mut state, "eDP-1", Confirmation::LogOff);
        state.dismiss_launcher("eDP-1");
        assert!(state.outputs[0].power_confirmation.is_none());

        let listener = UnixListener::bind(socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert_eq!(request, "quit\n");
            stream.write_all(b"err session unavailable\n").unwrap();
        });
        state.toggle_power("eDP-1");
        power_confirmation::open(&mut state, "eDP-1", Confirmation::LogOff);
        power_confirmation::confirm(&mut state, "eDP-1", Confirmation::LogOff);
        assert_eq!(
            state.outputs[0].launch_error.as_deref(),
            Some("session unavailable")
        );
        state.dismiss_power("eDP-1");
        assert!(state.outputs[0].power_confirmation.is_none());
        assert!(state.outputs[0].launch_error.is_none());
        server.join().unwrap();
    }
}
