//! Complete launcher membership, independent of the bounded desktop tile list.
use crate::ui::{app_context, command_client};
use hearthspace_desktop::app_catalog::AppCatalog;
use std::path::Path;

#[derive(Default)]
pub(in crate::ui::desktop::task) struct AppRegistration {
    ids: Option<Vec<String>>,
}

impl AppRegistration {
    pub(in crate::ui::desktop::task) fn sync(
        &mut self,
        socket: &Path,
        catalog: &AppCatalog,
    ) -> Result<(), String> {
        let mut ids: Vec<_> = catalog
            .apps()
            .iter()
            .filter(|app| app_context::validate_app_id(&app.id).is_ok())
            .map(|app| app.id.clone())
            .collect();
        ids.sort_unstable();
        if self.ids.as_ref() == Some(&ids) {
            return Ok(());
        }
        // A partial acknowledgement is never a committed registration. Retry
        // from offset zero, including after app removal or an empty catalog.
        self.ids = None;
        for command in commands(&ids)? {
            command_client::send_command(socket, &command)
                .map_err(|error| format!("Could not register applications: {error}"))?;
        }
        self.ids = Some(ids);
        Ok(())
    }

    pub(in crate::ui::desktop::task) fn open(
        &mut self,
        socket: &Path,
        catalog: &AppCatalog,
        request: &app_context::Request,
    ) -> Result<(), String> {
        if !request.current() {
            return Err("The app menu request expired. Open the menu again.".into());
        }
        app_context::validate_app_id(&request.target.app_id)?;
        if catalog.app_by_id(&request.target.app_id).is_none() {
            return Err("This application is no longer installed.".into());
        }
        // This runs in the same serial worker as sync and all menu intents.
        // No opening can observe an offset-zero reset with missing later batches.
        self.sync(socket, catalog)?;
        request.perform(socket)
    }
}

pub(super) fn commands(ids: &[String]) -> Result<Vec<String>, String> {
    if ids.is_empty() {
        return Ok(vec![
            "desktop-app-register {\"offset\":0,\"ids\":[]}".into(),
        ]);
    }
    let mut commands = Vec::new();
    let mut offset = 0;
    while offset < ids.len() {
        let build = |end| {
            format!(
                "desktop-app-register {}",
                serde_json::json!({"offset": offset, "ids": &ids[offset..end]}),
            )
        };
        let mut end = offset + 1;
        let mut command = build(end);
        if command.len() + 1 > hearthspace_ipc::transport::MAX_REQUEST_BYTES {
            return Err("An application ID is too long to register.".into());
        }
        while end < ids.len() {
            let next = build(end + 1);
            if next.len() + 1 > hearthspace_ipc::transport::MAX_REQUEST_BYTES {
                break;
            }
            command = next;
            end += 1;
        }
        commands.push(command);
        offset = end;
    }
    Ok(commands)
}

#[cfg(test)]
#[path = "app_registration_tests.rs"]
mod tests;
