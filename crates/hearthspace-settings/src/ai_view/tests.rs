use crate::{
    ai::{AiSettings, Operation, Response},
    state::{SettingsApp, SettingsCategory},
};
use hearthspace_ai::{DevicePreference, ModelStatus};
use hearthspace_ipc::Settings;
use xilem::{WidgetView, tokio::sync::mpsc::unbounded_channel, view::sized_box};

use crate::test_support as support;
use support::Ui;

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (dir, favorites) = crate::favorites::tests::fixture();
    (
        dir,
        SettingsApp {
            activation_listener: None,
            ai: AiSettings::default(),
            favorites,
            settings: Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Ai,
            selected_monitor_id: None,
            background_picker_request_sender: None,
            background_picker_pending: false,
            background_fit_open: false,
            status: String::new(),
            config_label: String::new(),
        },
    )
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::view(state))
}

#[test]
fn feature_details_expand_with_keyboard_and_report_their_state() {
    use xilem::masonry::core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    };

    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (710, 680));
    let feature = ui.button("On-device AI");
    assert_eq!(
        ui.harness
            .access_node(feature)
            .unwrap()
            .data()
            .is_expanded(),
        Some(false)
    );
    ui.harness.focus_on(Some(feature));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(ui.state.ai.features_open[0]);
    assert_eq!(
        ui.harness
            .access_node(feature)
            .unwrap()
            .data()
            .is_expanded(),
        Some(true)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(feature));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert_eq!(
        ui.harness
            .access_node(feature)
            .unwrap()
            .data()
            .is_expanded(),
        Some(false)
    );
}

#[test]
fn download_click_queues_background_work_and_disables_conflicting_controls() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.ai.sender = Some(sender);
    let mut ui = Ui::new(state, view, (710, 680));
    let download = ui.button("Download D1 model");
    ui.activate(download);
    ui.dispatch(&view);
    assert_eq!(requests.try_recv().unwrap(), Operation::Install);
    assert!(ui.harness.access_node(download).unwrap().is_disabled());
    assert!(
        ui.harness
            .access_node(ui.button("Refresh status"))
            .unwrap()
            .is_disabled()
    );
    ui.state.select_category(SettingsCategory::Displays);
    assert_eq!(ui.state.ai.pending, Some(Operation::Install));
}

#[test]
fn partial_install_failure_offers_delete_and_retry_without_closing_settings() {
    let (_dir, mut state) = state();
    let (sender, mut requests) = unbounded_channel();
    state.ai.sender = Some(sender);
    state.ai.receive(Response::Finished {
        operation: Operation::Install,
        snapshot: Some(ModelStatus {
            installed: false,
            downloaded: true,
            warm: false,
            device: DevicePreference::Cpu,
            runtime_error: Some("Ollama is unavailable".into()),
        }),
        error: Some("Import failed".into()),
    });
    let mut ui = Ui::new(state, view, (710, 680));
    let delete = ui.button("Delete D1 model");
    assert!(!ui.harness.access_node(delete).unwrap().is_disabled());
    assert!(
        !ui.harness
            .access_node(ui.button("Download D1 model"))
            .unwrap()
            .is_disabled()
    );
    ui.activate(delete);
    ui.dispatch(&view);
    assert_eq!(requests.try_recv().unwrap(), Operation::Remove);
}
