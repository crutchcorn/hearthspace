use hearthspace_ipc::{BackgroundMode, MonitorConfig, client_set_test};
use rfd::FileDialog;
use xilem::{
    AnyWidgetView, WidgetView,
    core::fork,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily},
    style::Style,
    view::{
        checkbox, flex_col, flex_row, label, radio_button, radio_group, sized_box, text_button,
        worker,
    },
};

use crate::{
    actions::{save_background_image_path, save_background_mode, save_monitor_scale},
    monitor_layout_view::monitor_layout_editor,
    monitors::{
        MONITOR_SCALE_OPTIONS, monitor_logical_height, monitor_logical_width,
        settings_with_live_outputs,
    },
    state::{BackgroundPickerRequest, BackgroundPickerResponse, SettingsApp},
};

pub(crate) fn app_logic(state: &mut SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let test_checked = state.settings.test;
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
    let monitor_summary = format_monitor_summary(&state.settings.monitors);

    let content = sized_box(
        flex_col((
            label("Hearthspace Settings")
                .font(GenericFamily::SystemUi)
                .text_size(22.0),
            label("Drag monitors to arrange their relative positions."),
            sized_box(monitor_layout_editor()).fixed_height(320.0.px()),
            label(monitor_summary),
            monitor_details_view(state),
            checkbox("test", test_checked, |state: &mut SettingsApp, checked| {
                let previous = state.settings.test;
                state.settings.test = checked;

                match client_set_test(checked) {
                    Ok(settings) => {
                        let saved_test = settings.test;
                        state.settings = settings_with_live_outputs(settings, &state.live_outputs);
                        state.status = format!("Saved test = {saved_test}");
                    }
                    Err(error) => {
                        state.settings.test = previous;
                        state.status = format!("Failed to save setting: {error}");
                    }
                }
            }),
            label("Background fit"),
            background_mode_picker(background_mode),
            label(format!("Background image: {background_image_label}")),
            flex_row((
                text_button(select_background_button_label, |state: &mut SettingsApp| {
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
                }),
                text_button("Use default background", |state: &mut SettingsApp| {
                    save_background_image_path(state, None);
                }),
            )),
            label(state.config_label.clone()),
            label(state.status.clone()),
        ))
        .gap(12.0.px()),
    )
    .padding(20.0.px());

    fork(
        content,
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

fn background_mode_picker(selected_mode: BackgroundMode) -> impl WidgetView<SettingsApp> + use<> {
    radio_group(flex_row((
        radio_button(
            "Repeat with canvas",
            selected_mode == BackgroundMode::RepeatWithCanvas,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::RepeatWithCanvas);
            },
        ),
        radio_button(
            "Stretch",
            selected_mode == BackgroundMode::Stretch,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::Stretch);
            },
        ),
        radio_button(
            "Contains",
            selected_mode == BackgroundMode::Contains,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::Contains);
            },
        ),
        radio_button(
            "Cover",
            selected_mode == BackgroundMode::Cover,
            |state: &mut SettingsApp| {
                save_background_mode(state, BackgroundMode::Cover);
            },
        ),
    )))
}

fn format_monitor_summary(monitors: &[MonitorConfig]) -> String {
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

    let scale_buttons = MONITOR_SCALE_OPTIONS.map(|scale| {
        let id = monitor.id.clone();
        text_button(format!("{}x", scale), move |state: &mut SettingsApp| {
            save_monitor_scale(state, &id, scale);
        })
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
