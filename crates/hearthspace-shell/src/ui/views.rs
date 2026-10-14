use hearthspace_common::{CONTROL_BAR_HEIGHT, SHELL_OUTPUT_WATCHER_APP_ID};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView, WindowOptionsExtLinux, WindowView,
    core::fork,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily},
    style::Style,
    view::{FlexSpacer, flex_row, label, sized_box, text_input},
    window,
};

use crate::ShellCommand;

use super::{
    command_client::send_command,
    constants::{MAX_RESULTS, SEARCH_BOX_HEIGHT, SEARCH_BOX_WIDTH, SHELL_BAR_H_PAD, launcher_size},
    launcher::{self, Entry},
    output_watcher::output_watcher_task,
    search_focus::{FocusTarget, focus_on_activation, track_search},
    state::{ShellOutputInstance, ShellState},
    window_catalog::window_catalog_task,
};

pub(super) fn app_logic(
    state: &mut ShellState,
) -> impl Iterator<Item = WindowView<ShellState>> + use<> {
    let watcher = window(
        state.watcher_window_id,
        "Hearthspace Output Watcher",
        output_watcher_view(),
    )
    .with_options(|options| {
        options
            .with_name(SHELL_OUTPUT_WATCHER_APP_ID, SHELL_OUTPUT_WATCHER_APP_ID)
            .with_decorations(false)
            .with_resizable(false)
            .with_initial_inner_size(LogicalSize::new(1.0, 1.0))
            .on_close(ShellState::stop)
    });
    let mut windows = vec![watcher];

    for output in &state.outputs {
        windows.push(shell_bar_window(output));

        let results = launcher::results(state, output);
        if !output.query.trim().is_empty() || output.show_windows || output.launch_error.is_some() {
            windows.push(launcher_window(
                output,
                results,
                state.windows_error.clone(),
            ));
        }
    }

    windows.into_iter()
}

fn shell_bar_window(output: &ShellOutputInstance) -> WindowView<ShellState> {
    let bar_app_id = output.bar_app_id();
    window(output.bar_window_id, "Hearthspace Shell", bar_view(output)).with_options(
        move |options| {
            options
                .with_name(&bar_app_id, &bar_app_id)
                .with_decorations(false)
                .with_resizable(false)
                .with_initial_inner_size(LogicalSize::new(760.0, f64::from(CONTROL_BAR_HEIGHT)))
                .on_close(ShellState::stop)
        },
    )
}

fn launcher_window(
    output: &ShellOutputInstance,
    results: Vec<Entry>,
    windows_error: Option<String>,
) -> WindowView<ShellState> {
    let launcher_app_id = output.launcher_app_id();
    let mut size = launcher_size(results.len().max(1));
    if output.show_windows && output.query.trim().is_empty() {
        size.height += 40.0;
    }
    if output.launch_error.is_some() || windows_error.is_some() {
        size.height += 100.0;
    }
    window(
        output.launcher_window_id,
        "Hearthspace Launcher",
        launcher::launcher_view(output, results, windows_error),
    )
    .with_options(move |options| {
        options
            .with_name(&launcher_app_id, &launcher_app_id)
            .with_decorations(false)
            .with_resizable(false)
            .with_initial_inner_size(launcher_size(MAX_RESULTS))
            .with_min_inner_size(size)
            .with_max_inner_size(size)
    })
}

fn bar_view(output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let focus_target = FocusTarget::default();
    let output_key = output.key().to_string();
    let search_output_key = output_key.clone();
    let cancel_output_key = output_key.clone();
    let windows_output_key = output_key.clone();
    let search_output_name = output.output_name.clone();
    let search = text_input(
        output.query.clone(),
        move |state: &mut ShellState, value: String| {
            state.change_query(&output_key, value);
        },
    )
    .placeholder("Search apps and windows...")
    .font(GenericFamily::SystemUi)
    .on_enter(move |state: &mut ShellState, value: String| {
        state.change_query(&search_output_key, value);
        let entry = state
            .outputs
            .iter()
            .find(|output| output.key() == search_output_key)
            .and_then(|output| launcher::results(state, output).into_iter().next());
        if let Some(entry) = entry {
            launcher::select(
                state,
                &search_output_key,
                search_output_name.as_deref(),
                &entry,
            );
        }
    })
    .on_cancel(move |state: &mut ShellState, _value: String| {
        state.dismiss_launcher(&cancel_output_key);
    });

    let windows_button =
        components::compact_action_button("Windows", move |state: &mut ShellState| {
            state.toggle_windows(&windows_output_key);
            if let Err(error) = send_command(&state.command_socket, "focus-launcher") {
                state.finish_launch(&windows_output_key, Err(error));
            }
        });

    let command_buttons: Vec<Box<AnyWidgetView<ShellState>>> = [
        ShellCommand::PanLeft,
        ShellCommand::PanRight,
        ShellCommand::PanUp,
        ShellCommand::PanDown,
        ShellCommand::ZoomIn,
        ShellCommand::ZoomOut,
        ShellCommand::LogAccessibilityTree,
    ]
    .into_iter()
    .map(|command| {
        components::compact_action_button(command.label(), move |state: &mut ShellState| {
            let _ = send_command(&state.command_socket, &command.wire_name());
        })
        .boxed()
    })
    .collect();

    focus_on_activation(
        sized_box(
            flex_row((
                FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
                sized_box(track_search(search, focus_target.clone()))
                    .fixed_width(SEARCH_BOX_WIDTH.px())
                    .fixed_height(SEARCH_BOX_HEIGHT.px()),
                windows_button,
                command_buttons,
                FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
            ))
            .gap(8.0.px()),
        )
        .background_color(components::SURFACE)
        .padding(7.0.px()),
        focus_target,
    )
}

fn output_watcher_view() -> impl WidgetView<ShellState> + use<> {
    sized_box(fork(
        fork(label(""), output_watcher_task()),
        window_catalog_task(),
    ))
    .fixed_width(1.0.px())
    .fixed_height(1.0.px())
}
