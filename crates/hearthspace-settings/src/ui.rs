use hearthspace_ipc::{BackgroundMode, MonitorConfig, client_set_test};
use hearthspace_ui_components as components;
use rfd::FileDialog;
use xilem::{
    AnyWidgetView, WidgetView,
    core::fork,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, checkbox, flex_col, flex_item, flex_row, label, sized_box, worker},
};

use crate::{
    actions::{save_background_image_path, save_background_mode, save_monitor_scale},
    monitor_layout_view::monitor_layout_editor,
    monitors::{
        MONITOR_SCALE_OPTIONS, monitor_logical_height, monitor_logical_width,
        settings_with_live_outputs,
    },
    state::{BackgroundPickerRequest, BackgroundPickerResponse, SettingsApp, SettingsCategory},
};

pub(crate) fn app_logic(state: &mut SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let content = components::app_background(
        flex_row((
            sized_box(sidebar_view(state)).fixed_width(220.0.px()),
            flex_item(category_view(state), 1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(20.0.px()),
    );

    fork(
        fork(content, crate::ai_worker::worker_view()),
        worker(
            |proxy, mut rx| async move {
                while let Some(BackgroundPickerRequest::PickFile) = rx.recv().await {
                    let response = match xilem::tokio::task::spawn_blocking(|| {
                        FileDialog::new()
                            .add_filter("PNG image", &["png"])
                            .pick_file()
                    })
                    .await
                    {
                        Ok(Some(path)) => BackgroundPickerResponse::Selected(path),
                        Ok(None) => BackgroundPickerResponse::Cancelled,
                        Err(error) => BackgroundPickerResponse::Failed(error.to_string()),
                    };

                    drop(proxy.message(response));
                }
            },
            |state: &mut SettingsApp, sender| {
                state.background_picker_request_sender = Some(sender);
            },
            |state: &mut SettingsApp, response| {
                state.background_picker_pending = false;
                match response {
                    BackgroundPickerResponse::Selected(path) => {
                        save_background_image_path(state, Some(path));
                    }
                    BackgroundPickerResponse::Cancelled => {
                        state.status = "Background image selection cancelled".to_string();
                    }
                    BackgroundPickerResponse::Failed(error) => {
                        state.status = format!("Background image picker failed: {error}");
                    }
                }
            },
        ),
    )
}

fn sidebar_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let selected_category = state.selected_category;

    components::sidebar_panel(components::scroll_area(
        flex_col((
            components::page_title("Settings"),
            components::muted_text("Hearthspace"),
            components::sidebar_item(
                SettingsCategory::Display.label(),
                selected_category == SettingsCategory::Display,
                |state: &mut SettingsApp| {
                    state.select_category(SettingsCategory::Display);
                },
            ),
            components::sidebar_item(
                SettingsCategory::Appearance.label(),
                selected_category == SettingsCategory::Appearance,
                |state: &mut SettingsApp| {
                    state.select_category(SettingsCategory::Appearance);
                },
            ),
            components::sidebar_item(
                SettingsCategory::Search.label(),
                selected_category == SettingsCategory::Search,
                |state: &mut SettingsApp| state.select_category(SettingsCategory::Search),
            ),
            components::sidebar_item(
                SettingsCategory::Ai.label(),
                selected_category == SettingsCategory::Ai,
                |state: &mut SettingsApp| state.select_category(SettingsCategory::Ai),
            ),
            components::sidebar_item(
                SettingsCategory::Advanced.label(),
                selected_category == SettingsCategory::Advanced,
                |state: &mut SettingsApp| {
                    state.select_category(SettingsCategory::Advanced);
                },
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px()),
    ))
}

fn category_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let category = state.selected_category;

    components::scroll_area(sized_box(
        flex_col((
            components::page_title(category.label()),
            components::muted_text(category.description()),
            selected_category_view(state),
            status_footer(state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(16.0.px()),
    ))
}

fn selected_category_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    match state.selected_category {
        SettingsCategory::Display => display_settings_view(state),
        SettingsCategory::Appearance => appearance_settings_view(state),
        SettingsCategory::Search => crate::favorites_view::view(state),
        SettingsCategory::Ai => crate::ai_view::view(state),
        SettingsCategory::Advanced => advanced_settings_view(state),
    }
}

fn display_settings_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let monitor_summary = format_monitor_summary(&state.settings.monitors);

    components::settings_section(
        "Monitor Layout",
        flex_col((
            components::muted_text("Drag monitors to arrange their relative positions."),
            sized_box(monitor_layout_editor()).fixed_height(320.0.px()),
            components::muted_text(monitor_summary),
            monitor_details_view(state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(12.0.px()),
    )
    .boxed()
}

fn appearance_settings_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let background_mode = state.settings.background_mode;
    let background_image_label = state
        .settings
        .background_image_path
        .as_deref()
        .unwrap_or("Default background")
        .to_string();
    let select_background_button_label = if state.background_picker_pending {
        "Selecting background image..."
    } else {
        "Select background image"
    };

    components::settings_section(
        "Canvas Background",
        flex_col((
            components::field_label("Background fit"),
            background_mode_picker(background_mode),
            components::field_label(format!("Background image: {background_image_label}")),
            flex_row((
                components::action_button(
                    select_background_button_label,
                    |state: &mut SettingsApp| {
                        if state.background_picker_pending {
                            state.status = "Background image picker is already open".to_string();
                            return;
                        }

                        let Some(sender) = &state.background_picker_request_sender else {
                            state.status = "Background image picker is not ready".to_string();
                            return;
                        };

                        match sender.send(BackgroundPickerRequest::PickFile) {
                            Ok(()) => {
                                state.background_picker_pending = true;
                                state.status = "Opening background image picker".to_string();
                            }
                            Err(error) => {
                                state.status =
                                    format!("Failed to open background image picker: {error}");
                            }
                        }
                    },
                ),
                components::action_button("Use default background", |state: &mut SettingsApp| {
                    save_background_image_path(state, None);
                }),
            ))
            .gap(10.0.px()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(12.0.px()),
    )
    .boxed()
}

fn advanced_settings_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let test_checked = state.settings.test;

    components::settings_section(
        "Diagnostics",
        flex_col((
            checkbox(
                "Test setting",
                test_checked,
                |state: &mut SettingsApp, checked| {
                    let previous = state.settings.test;
                    state.settings.test = checked;

                    match client_set_test(checked) {
                        Ok(settings) => {
                            let saved_test = settings.test;
                            state.settings =
                                settings_with_live_outputs(settings, &state.live_outputs);
                            state.status = format!("Saved test = {saved_test}");
                        }
                        Err(error) => {
                            state.settings.test = previous;
                            state.status = format!("Failed to save setting: {error}");
                        }
                    }
                },
            ),
            components::muted_text(state.config_label.clone()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(12.0.px()),
    )
    .boxed()
}

fn status_footer(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    sized_box(
        flex_col((
            components::field_label("Status"),
            components::muted_text(state.status.clone()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(4.0.px()),
    )
    .background_color(components::SURFACE_SUBTLE)
    .corner_radius(8.0.px())
    .padding(12.0.px())
}

fn background_mode_picker(selected_mode: BackgroundMode) -> impl WidgetView<SettingsApp> + use<> {
    flex_row((
        components::segmented_option(
            "Repeat with canvas",
            selected_mode == BackgroundMode::RepeatWithCanvas,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::RepeatWithCanvas);
            },
        ),
        components::segmented_option(
            "Stretch",
            selected_mode == BackgroundMode::Stretch,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::Stretch);
            },
        ),
        components::segmented_option(
            "Contains",
            selected_mode == BackgroundMode::Contains,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::Contains);
            },
        ),
        components::segmented_option(
            "Cover",
            selected_mode == BackgroundMode::Cover,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::Cover);
            },
        ),
    ))
    .gap(8.0.px())
}

fn format_monitor_summary(monitors: &[MonitorConfig]) -> String {
    if monitors.is_empty() {
        return "No monitors are currently available.".to_string();
    }

    monitors
        .iter()
        .map(|monitor| {
            format!(
                "{}: {}x{} @ {}x at {},{}",
                monitor.name, monitor.width, monitor.height, monitor.scale, monitor.x, monitor.y
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn monitor_details_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let Some(monitor) = selected_monitor(state) else {
        return label("Select a monitor to edit its settings.").boxed();
    };

    let selected_scale = monitor.scale;
    let scale_buttons = MONITOR_SCALE_OPTIONS.map(|scale| {
        let id = monitor.id.clone();
        components::segmented_option(
            format!("{}x", scale),
            selected_scale == scale,
            move |state: &mut SettingsApp| {
                save_monitor_scale(state, &id, scale);
            },
        )
    });

    flex_col((
        label(format!("Selected: {}", monitor.name)),
        label(format!(
            "Physical: {}x{} | Desktop: {}x{} | Position: {},{}",
            monitor.width,
            monitor.height,
            monitor_logical_width(monitor),
            monitor_logical_height(monitor),
            monitor.x,
            monitor.y
        )),
        flex_row((label(format!("Scale: {}x", monitor.scale)), scale_buttons)),
    ))
    .gap(8.0.px())
    .boxed()
}

fn selected_monitor(state: &SettingsApp) -> Option<&MonitorConfig> {
    let selected_id = state.selected_monitor_id.as_ref()?;
    state
        .settings
        .monitors
        .iter()
        .find(|monitor| &monitor.id == selected_id)
}
