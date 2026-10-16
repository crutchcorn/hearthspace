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
            ai: AiSettings::default(),
            favorites,
            settings: Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Ai,
            selected_monitor_id: None,
            background_picker_request_sender: None,
            background_picker_pending: false,
            status: String::new(),
            config_label: String::new(),
        },
    )
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::view(state))
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
    ui.state.select_category(SettingsCategory::Display);
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
