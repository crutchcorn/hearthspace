//! Small everyday session controls, kept separate from application search.
use std::time::Duration;

use chrono::Local;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon as Glyph, icon},
};
use xilem::{
    WidgetView, WindowView,
    core::{MessageProxy, NoElement, View},
    masonry::{
        layout::AsUnit,
        parley::style::FontWeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, button, flex_col, flex_row, label, task_raw},
};

use super::{
    command_client::send_command,
    icons::{Icon, icon_button},
    popup::{self, PopupKind},
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
    state.canvas_dragging = false;
    if let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == output_key)
    {
        output.confirm_logout = enabled;
        output.show_power = true;
        output.launch_error = None;
    }
}

fn confirm_logout(state: &mut ShellState, output_key: &str) {
    if !state
        .outputs
        .iter()
        .any(|output| output.key() == output_key && output.show_power && output.confirm_logout)
    {
        return;
    }
    if let Err(error) = send_command(&state.command_socket, "quit") {
        state.finish_launch(output_key, Err(error));
    }
}

pub(super) fn power_button(output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    icon_button(Icon::Power, "Power", output.show_power, move |state| {
        state.toggle_power(&key)
    })
}

pub(super) fn power_window(
    output: &ShellOutputInstance,
    power: &super::power_task::PowerState,
    battery: Option<&crate::battery::BatteryStatus>,
) -> WindowView<ShellState> {
    let app_id = output.power_app_id();
    let key = output.key().to_string();
    let size = popup::size(output, PopupKind::Power, 320.0, 280.0);
    popup::window(
        output.power_window_id,
        "Hearthspace Power",
        app_id,
        size,
        power_panel(output, power, battery),
        move |state| state.dismiss_power(&key),
    )
}

/// Full-width native buttons retain the shared hover and keyboard focus states.
fn action<F: Fn(&mut ShellState) + Send + Sync + 'static>(
    glyph: Glyph,
    name: &'static str,
    enabled: bool,
    confirming: bool,
    callback: F,
) -> impl WidgetView<ShellState> + use<F> {
    let color = if !enabled {
        components::TEXT_MUTED
    } else if confirming {
        components::ACCENT_STRONG
    } else {
        components::TEXT
    };
    components::button_surface(
        button(
            flex_row((
                icon(glyph, 20.0).color(color),
                label(name).text_size(14.0).color(color).flex(1.0),
            ))
            .gap(12.0.px()),
            callback,
        )
        .disabled(!enabled)
        .height(48.0.px())
        .padding(Padding::from_vh(10.0.px(), 14.0.px()))
        .corner_radius(components::RADIUS_CONTROL.px()),
        components::ButtonSurface::Control,
        confirming,
    )
}

pub(super) fn power_panel(
    output: &ShellOutputInstance,
    power: &super::power_task::PowerState,
    battery: Option<&crate::battery::BatteryStatus>,
) -> impl WidgetView<ShellState> + use<> {
    let output_key = output.key().to_string();
    let cancel_key = output_key.clone();
    let lock_key = output_key.clone();
    let suspend_key = output_key.clone();
    let close_key = output_key.clone();
    let confirming = output.confirm_logout;
    let lock = (!confirming).then(|| {
        action(Glyph::Lock, "Lock", true, false, move |state| {
            let result = send_command(&state.command_socket, "lock-session");
            state.finish_launch(&lock_key, result);
        })
    });
    let suspend = (!confirming).then(|| {
        action(
            Glyph::Moon,
            if power.busy() {
                "Suspending…"
            } else {
                "Suspend"
            },
            power.can_suspend(),
            false,
            move |state| super::power_task::suspend(state, &suspend_key),
        )
    });
    let logout = action(
        Glyph::LogOut,
        if confirming {
            "Confirm log out"
        } else {
            "Log out"
        },
        true,
        confirming,
        move |state| {
            if confirming {
                confirm_logout(state, &output_key);
            } else {
                set_confirmation(state, &output_key, true);
            }
        },
    );
    let cancel = confirming.then(|| {
        action(Glyph::ArrowLeft, "Cancel", true, false, move |state| {
            set_confirmation(state, &cancel_key, false);
        })
    });
    let hint = confirming.then(|| {
        components::field_label("Close your apps and end this session?")
            .color(components::TEXT_MUTED)
            .line_break_mode(LineBreaking::WordWrap)
    });
    let error = output
        .launch_error
        .clone()
        .map(|error| components::field_label(error).line_break_mode(LineBreaking::WordWrap));
    let unavailable = (!confirming && !power.status.available && !power.busy()).then(|| {
        components::field_label(
            power
                .status
                .error
                .clone()
                .unwrap_or_else(|| "Suspend is unavailable in this session.".into()),
        )
        .color(components::TEXT_MUTED)
        .line_break_mode(LineBreaking::WordWrap)
    });
    popup::panel_with_header(
        output,
        PopupKind::Power,
        flex_row((
            icon(Glyph::Power, 20.0).color(components::ACCENT_STRONG),
            label("Power")
                .text_size(16.0)
                .weight(FontWeight::MEDIUM)
                .color(components::TEXT)
                .flex(1.0),
            icon_button(Icon::Close, "Close power menu", false, move |state| {
                state.dismiss_power(&close_key)
            }),
        ))
        .gap(12.0.px()),
        flex_col((
            hint,
            error,
            flex_col((lock, suspend, logout, cancel))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(8.0.px()),
            unavailable,
            super::battery_view::detail(battery),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(16.0.px()),
    )
}

#[cfg(test)]
#[path = "power_view_tests.rs"]
mod view_tests;

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
