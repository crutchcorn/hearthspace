use hearthspace_common::{CONTROL_BAR_HEIGHT, SHELL_OUTPUT_WATCHER_APP_ID};
use xilem::{
    AnyWidgetView, TextAlign, WidgetView, WindowOptionsExtLinux, WindowView,
    core::fork,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily, properties::Gap},
    style::Style,
    view::{
        CrossAxisAlignment, FlexSpacer, button, flex_col, flex_row, label, sized_box, text_button,
        text_input,
    },
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
        if !results.is_empty() {
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
    let size = launcher_size(results.len());
    window(
        launcher_id,
        "Hearthspace Launcher",
        launcher_view(output_key, results),
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
    let search = text_input(
        output.query.clone(),
        move |state: &mut ShellState, value: String| {
            if let Some(query) = state.output_query_mut(&output_key) {
                *query = value;
            }
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
            launch_app(&state.command_socket, &app.id);
            if let Some(query) = state.output_query_mut(&search_output_key) {
                query.clear();
            }
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
        text_button(command.label(), move |state: &mut ShellState| {
            send_command(&state.command_socket, &command.wire_name());
        })
        .boxed()
    })
    .collect();

    flex_row((
        FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
        sized_box(search)
            .fixed_width(SEARCH_BOX_WIDTH.px())
            .fixed_height(SEARCH_BOX_HEIGHT.px()),
        command_buttons,
        FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
    ))
}

fn output_watcher_view() -> impl WidgetView<ShellState> + use<> {
    sized_box(fork(label(""), output_watcher_task()))
        .fixed_width(1.0.px())
        .fixed_height(1.0.px())
}

fn launcher_view(
    output_key: String,
    results: Vec<DesktopApp>,
) -> impl WidgetView<ShellState> + use<> {
    let buttons: Vec<Box<AnyWidgetView<ShellState>>> = results
        .into_iter()
        .map(|app| {
            let app_id = app.id;
            let output_key = output_key.clone();
            button(
                sized_box(label(app.name).text_alignment(TextAlign::Start))
                    .fixed_width(LAUNCHER_ITEM_WIDTH.px()),
                move |state: &mut ShellState| {
                    launch_app(&state.command_socket, &app_id);
                    if let Some(query) = state.output_query_mut(&output_key) {
                        query.clear();
                    }
                },
            )
            .boxed()
        })
        .collect();

    flex_col(buttons)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Gap::ZERO)
}
