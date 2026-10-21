//! Bind a popup to its initiating part before creating its native window.
use crate::ui::{quick_panel::Kind, state::ShellState};

#[derive(Clone, Debug)]
pub(in crate::ui) enum Popup {
    Apps,
    Notifications,
    Power,
    Map,
    Quick(Kind),
    Tray,
    TrayMenu(crate::tray::ItemKey),
}

pub(in crate::ui) fn defer(state: &mut ShellState, key: &str, popup: Popup) -> bool {
    if state.frame_layout.opening_popup || !state.frame_layout.visible {
        return false;
    }
    let Some(output) = state.outputs.iter().find(|output| output.key() == key) else {
        return false;
    };
    let (id, app_id) = match &popup {
        Popup::Apps => ("apps", output.launcher_app_id()),
        Popup::Notifications => ("notifications", output.notifications_app_id()),
        Popup::Power => ("power", output.power_app_id()),
        Popup::Map => ("tools", output.map_app_id()),
        Popup::Tray | Popup::TrayMenu(_) => (
            "tray",
            output
                .output_name
                .as_deref()
                .map(hearthspace_common::tray_menu_app_id_for_output)
                .unwrap_or_else(|| hearthspace_common::TRAY_MENU_APP_ID.into()),
        ),
        Popup::Quick(kind) => (
            match kind {
                Kind::Wifi => "wifi",
                Kind::Sound => "volume",
                Kind::Brightness => "brightness",
                Kind::Battery => "battery",
                Kind::Clock => "clock",
                Kind::Tools => "tools",
            },
            output
                .output_name
                .as_deref()
                .map(hearthspace_common::quick_panel_app_id_for_output)
                .unwrap_or_else(|| hearthspace_common::QUICK_PANEL_APP_ID.into()),
        ),
    };
    let command = format!(
        "frame-popup-anchor {}",
        serde_json::json!({"app_id":app_id,"output":output.key(),"target":hearthspace_common::frame_layout::LayoutTarget::widget(id)})
    );
    let request = super::task::Request {
        command,
        open: Some((key.to_owned(), popup)),
    };
    if let Some(requests) = &state.frame_layout.requests {
        if let Err(error) = requests.try_send(request) {
            state.frame_layout.error = Some(format!("Popup could not be opened: {error}"));
        }
        true
    } else {
        false
    }
}

pub(super) fn open(state: &mut ShellState, key: &str, popup: Popup) {
    if state.frame_layout.snapshot.editing {
        return;
    }
    state.frame_layout.opening_popup = true;
    match popup {
        Popup::Apps => state.toggle_search(key),
        Popup::Notifications => state.toggle_notifications(key),
        Popup::Power => state.toggle_power(key),
        Popup::Map => state.toggle_map(key),
        Popup::Quick(kind) => state.toggle_quick_panel(key, kind),
        Popup::Tray => crate::ui::tray_state::toggle(state, key),
        Popup::TrayMenu(item) => crate::ui::tray_state::open_menu(state, key, &item),
    }
    state.frame_layout.opening_popup = false;
}
