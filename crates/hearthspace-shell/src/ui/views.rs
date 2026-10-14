use hearthspace_common::{CONTROL_BAR_HEIGHT, SHELL_OUTPUT_WATCHER_APP_ID};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView, WindowOptionsExtLinux, WindowView,
    core::fork,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily},
    style::Style,
    view::{CrossAxisAlignment, FlexSpacer, flex_col, flex_row, label, sized_box, text_input},
    window,
};

use crate::{ShellCommand, app_catalog::DesktopApp};

use super::{
    command_client::{launch_app, send_command},
    constants::{
        LAUNCHER_ITEM_WIDTH, MAX_RESULTS, SEARCH_BOX_HEIGHT, SEARCH_BOX_WIDTH, SHELL_BAR_H_PAD,
        launcher_size,
    },
    output_watcher::output_watcher_task,
    state::{ShellOutputInstance, ShellState},
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

        let output_key = output.key().to_string();
        let results = search_results(state, &output_key);
        if !results.is_empty() || output.launch_error.is_some() {
            windows.push(launcher_window(output, output_key, results));
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
    output_key: String,
    results: Vec<DesktopApp>,
) -> WindowView<ShellState> {
    let launcher_app_id = output.launcher_app_id();
    let launcher_id = output.launcher_window_id;
    let output_name = output.output_name.clone();
    let mut size = launcher_size(results.len());
    if output.launch_error.is_some() {
        size.height += 100.0;
    }
    window(
        launcher_id,
        "Hearthspace Launcher",
        launcher_view(
            output_key,
            output_name,
            results,
            output.launch_error.clone(),
        ),
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

fn search_results(state: &ShellState, output_key: &str) -> Vec<DesktopApp> {
    let Some(output) = state
        .outputs
        .iter()
        .find(|output| output.key() == output_key)
    else {
        return Vec::new();
    };
    let query = output.query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    state.catalog.search(query, MAX_RESULTS)
}

fn bar_view(output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let output_key = output.key().to_string();
    let search_output_key = output_key.clone();
    let search_output_name = output.output_name.clone();
    let search = text_input(
        output.query.clone(),
        move |state: &mut ShellState, value: String| {
            state.change_query(&output_key, value);
        },
    )
    .placeholder("Search apps...")
    .font(GenericFamily::SystemUi)
    .on_enter(move |state: &mut ShellState, value: String| {
        let query = value.trim();
        if query.is_empty() {
            return;
        }
        if let Some(app) = state.catalog.search(query, 1).into_iter().next() {
            let result = launch_app(
                &state.command_socket,
                &app.id,
                search_output_name.as_deref(),
            );
            state.finish_launch(&search_output_key, result);
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

    sized_box(
        flex_row((
            FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
            sized_box(search)
                .fixed_width(SEARCH_BOX_WIDTH.px())
                .fixed_height(SEARCH_BOX_HEIGHT.px()),
            command_buttons,
            FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
        ))
        .gap(8.0.px()),
    )
    .background_color(components::SURFACE)
    .padding(7.0.px())
}

fn output_watcher_view() -> impl WidgetView<ShellState> + use<> {
    sized_box(fork(label(""), output_watcher_task()))
        .fixed_width(1.0.px())
        .fixed_height(1.0.px())
}

fn launcher_view(
    output_key: String,
    output_name: Option<String>,
    results: Vec<DesktopApp>,
    launch_error: Option<String>,
) -> impl WidgetView<ShellState> + use<> {
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = results
        .into_iter()
        .map(|app| {
            let app_id = app.id;
            let output_key = output_key.clone();
            let output_name = output_name.clone();
            components::launcher_item(
                app.name,
                LAUNCHER_ITEM_WIDTH,
                move |state: &mut ShellState| {
                    let result = launch_app(&state.command_socket, &app_id, output_name.as_deref());
                    state.finish_launch(&output_key, result);
                },
            )
            .boxed()
        })
        .collect();

    if let Some(error) = launch_error {
        rows.insert(
            0,
            sized_box(components::field_label(format!(
                "Could not start the app: {error}"
            )))
            .background_color(components::DANGER_SUBTLE)
            .padding(8.0.px())
            .boxed(),
        );
    }

    sized_box(
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(4.0.px()),
    )
    .background_color(components::SURFACE)
    .padding(6.0.px())
}
