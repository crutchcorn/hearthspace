//! Bind a popup to its initiating part before creating its native window.
use crate::ui::{popup::PopupKind, quick_panel::Kind, state::ShellState};

#[derive(Clone, Debug)]
pub(in crate::ui) enum Popup {
    Apps,
    AppsRequest {
        token: u64,
        toggle: bool,
    },
    Notifications,
    Power,
    Map,
    Quick(Kind),
    TrayMenu(crate::tray::ItemKey),
    DockTrayMenu {
        app: String,
        item: crate::tray::ItemKey,
        menu_path: String,
    },
}

impl Popup {
    pub(super) fn toggles(&self) -> bool {
        !matches!(
            self,
            Self::AppsRequest { toggle: false, .. } | Self::TrayMenu(_) | Self::DockTrayMenu { .. }
        )
    }

    pub(super) fn same_destination(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Apps | Self::AppsRequest { .. }, Self::Apps | Self::AppsRequest { .. }) => true,
            (Self::Quick(a), Self::Quick(b)) => a == b,
            (Self::TrayMenu(a), Self::TrayMenu(b)) => a == b,
            (
                Self::DockTrayMenu {
                    app: a,
                    item: ai,
                    menu_path: ap,
                },
                Self::DockTrayMenu {
                    app: b,
                    item: bi,
                    menu_path: bp,
                },
            ) => a == b && ai == bi && ap == bp,
            (Self::Notifications, Self::Notifications)
            | (Self::Power, Self::Power)
            | (Self::Map, Self::Map) => true,
            _ => false,
        }
    }

    fn visible(&self, output: &crate::ui::state::ShellOutputInstance) -> bool {
        match self {
            Self::Apps | Self::AppsRequest { .. } => output.show_search,
            Self::Notifications => output.show_notifications,
            Self::Power => output.show_power,
            Self::Map => output.show_map,
            Self::Quick(kind) => output.show_controls && output.quick_panel == *kind,
            Self::TrayMenu(item) => output.tray.open && output.tray.item.as_ref() == Some(item),
            Self::DockTrayMenu { item, .. } => {
                output.tray.open && output.tray.item.as_ref() == Some(item)
            }
        }
    }

    pub(super) fn matches(&self, kind: PopupKind) -> bool {
        matches!(
            (self, kind),
            (Self::Apps | Self::AppsRequest { .. }, PopupKind::Search)
                | (Self::Notifications, PopupKind::Notifications)
                | (Self::Power, PopupKind::Power)
                | (Self::Map, PopupKind::Map)
                | (Self::Quick(_), PopupKind::Controls)
                | (Self::TrayMenu(_), PopupKind::Tray)
                | (Self::DockTrayMenu { .. }, PopupKind::Tray)
        )
    }
}

pub(in crate::ui) fn defer(state: &mut ShellState, key: &str, popup: Popup) -> bool {
    if state.frame_layout.opening_popup || !state.frame_layout.visible {
        return false;
    }
    let Some(output) = state.outputs.iter().find(|output| output.key() == key) else {
        return false;
    };
    // Closing never needs an anchor. Apply it synchronously, so a following
    // reopening receives its own anchor and (for AppsRequest) accepted token.
    if popup.toggles() && popup.visible(output) {
        return false;
    }
    let (id, app_id) = match &popup {
        Popup::Apps => ("apps", output.launcher_app_id()),
        Popup::AppsRequest { token, .. } => (
            "apps",
            hearthspace_common::launcher_app_id_for_request(output.output_name.as_deref(), *token),
        ),
        Popup::Notifications => ("notifications", output.notifications_app_id()),
        Popup::Power => ("power", output.power_app_id()),
        Popup::Map => ("tools", output.map_app_id()),
        Popup::TrayMenu(_) | Popup::DockTrayMenu { .. } => (
            if matches!(popup, Popup::DockTrayMenu { .. }) {
                "apps"
            } else {
                "tray"
            },
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
    if let Some(prior) =
        state
            .frame_layout
            .intents
            .reverse(key, &popup, state.frame_layout.popup_epoch)
    {
        // Fold open then close in one UI update. Reuse the real handlers so
        // dismissal/reset effects survive (including quick kind and query).
        open(state, key, prior);
        open(state, key, popup);
        return true;
    }
    let intent =
        state
            .frame_layout
            .intents
            .begin(key.to_owned(), popup, state.frame_layout.popup_epoch);
    let request = super::task::Request {
        command,
        open: Some(intent),
    };
    if let Some(requests) = &state.frame_layout.requests {
        if let Err(error) = requests.try_send(request) {
            state.frame_layout.intents.cancel();
            state.frame_layout.error = Some(format!("Popup could not be opened: {error}"));
        }
        true
    } else {
        false
    }
}

pub(in crate::ui) fn open(state: &mut ShellState, key: &str, popup: Popup) {
    if state.frame_layout.snapshot.editing
        || state.frame_layout.snapshot.confirmation.is_some()
        || !state.outputs.iter().any(|output| output.key() == key)
    {
        return;
    }
    state.frame_layout.opening_popup = true;
    match popup {
        Popup::Apps => state.toggle_search(key),
        Popup::AppsRequest { token, toggle } => {
            // The anchor acknowledgement is asynchronous. Preserve a newer open
            // launcher's query, and attach the accepted token only to this opening.
            if toggle
                || !state
                    .outputs
                    .iter()
                    .any(|output| output.key() == key && output.show_search)
            {
                state.toggle_search(key);
                if let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|output| output.key() == key && output.show_search)
                {
                    output.search_request_token = Some(token);
                }
            }
        }
        Popup::Notifications => state.toggle_notifications(key),
        Popup::Power => state.toggle_power(key),
        Popup::Map => state.toggle_map(key),
        Popup::Quick(kind) => state.toggle_quick_panel(key, kind),
        Popup::TrayMenu(item) => crate::ui::tray_state::open_menu(state, key, &item),
        Popup::DockTrayMenu {
            app,
            item,
            menu_path,
        } => crate::ui::dock::tray::open_menu(state, key, &app, &item, &menu_path),
    }
    state.frame_layout.opening_popup = false;
}

#[cfg(test)]
#[path = "popups_tests.rs"]
pub(in crate::ui) mod tests;
