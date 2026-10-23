//! Delegate file properties to the session's file manager using its standard API.
use super::{DesktopItem, DesktopItemKind};
use gio::prelude::*;
use std::{fs, path::PathBuf};

const FILE_MANAGER: &str = "org.freedesktop.FileManager1";
const FILE_MANAGER_PATH: &str = "/org/freedesktop/FileManager1";

pub fn properties(items: &[DesktopItem]) -> Result<(), String> {
    show_properties(
        items,
        std::env::var("HEARTHSPACE_SESSION_BUS_ADDRESS")
            .ok()
            .as_deref(),
    )
}

fn show_properties(items: &[DesktopItem], address: Option<&str>) -> Result<(), String> {
    let uris = property_uris(items)?;
    // Never autolaunch a bus or use the inherited host desktop's file manager.
    // The supervisor already publishes our display to this bus for activation.
    let address = address
        .filter(|address| !address.is_empty())
        .ok_or("The desktop session bus is unavailable; could not open file properties.")?;
    let connection = gio::DBusConnection::for_address_sync(
        address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .map_err(|error| format!("Could not connect to the desktop file manager: {error}"))?;
    connection
        .call_sync(
            Some(FILE_MANAGER),
            FILE_MANAGER_PATH,
            FILE_MANAGER,
            "ShowItemProperties",
            Some(&(uris, "").to_variant()),
            Some(gio::glib::VariantTy::UNIT),
            gio::DBusCallFlags::NONE,
            5_000,
            gio::Cancellable::NONE,
        )
        .map_err(|error| format!("Could not open properties in the file manager: {error}"))?;
    Ok(())
}

fn property_uris(items: &[DesktopItem]) -> Result<Vec<String>, String> {
    if items.is_empty() {
        return Err("Select an item to view its properties.".into());
    }
    items
        .iter()
        .map(|item| {
            let path: PathBuf = match &item.kind {
                DesktopItemKind::File { path } | DesktopItemKind::Folder { path } => path.clone(),
                DesktopItemKind::App { app_id, .. } => gio_unix::DesktopAppInfo::new(app_id)
                    .and_then(|app| app.filename())
                    .ok_or("This application's desktop entry is no longer available.")?,
            };
            // Properties describe the link itself, including a dangling link.
            if !path.is_absolute() || fs::symlink_metadata(&path).is_err() {
                return Err("This desktop item is no longer available.".into());
            }
            Ok(gio::File::for_path(path).uri().to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests;
