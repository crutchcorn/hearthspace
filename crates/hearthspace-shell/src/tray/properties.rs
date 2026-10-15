use std::collections::HashMap;
use zbus::{
    Connection,
    fdo::PropertiesProxy,
    names::InterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use super::{ItemKey, TrayIcon, TrayItem, TrayMenu, TrayPixmap, TrayStatus};

pub(super) const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
type Pixmaps = Vec<(i32, i32, Vec<u8>)>;
type Tooltip = (String, Pixmaps, String, String);
pub(super) type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Debug)]
pub(super) struct Item {
    pub view: TrayItem,
    pub menu_path: Option<String>,
    pub menu_loaded: bool,
    pub menu_revision: u64,
}

pub(super) async fn read(connection: &Connection, key: ItemKey) -> Result<Item, String> {
    let proxy = PropertiesProxy::builder(connection)
        .destination(key.owner.clone())
        .map_err(error)?
        .path(key.path.clone())
        .map_err(error)?
        .build()
        .await
        .map_err(error)?;
    let properties = proxy
        .get_all(InterfaceName::from_static_str_unchecked(ITEM_INTERFACE))
        .await
        .map_err(error)?;
    from_properties(key, &properties)
}

pub(super) fn from_properties(key: ItemKey, properties: &Properties) -> Result<Item, String> {
    let id = string(properties, "Id").ok_or("Tray item has no Id")?;
    let title = string(properties, "Title").unwrap_or(id);
    let status = match string(properties, "Status").as_deref() {
        Some("Passive") => TrayStatus::Passive,
        Some("NeedsAttention") => TrayStatus::NeedsAttention,
        _ => TrayStatus::Active,
    };
    let tooltip = properties
        .get("ToolTip")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Tooltip::try_from(value).ok())
        .map(|(_, _, title, description)| {
            [title, description]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| title.clone());
    let mut icon = read_icon(properties, "Icon");
    if status == TrayStatus::NeedsAttention {
        let attention = read_icon(properties, "AttentionIcon");
        if attention.name.is_some() || attention.pixmap.is_some() {
            icon = attention;
        }
    }
    let menu_path = properties
        .get("Menu")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| OwnedObjectPath::try_from(value).ok())
        .map(|path| path.to_string())
        .filter(|path| path != "/");
    Ok(Item {
        view: TrayItem {
            key,
            title: limit(&title),
            tooltip: limit(&tooltip),
            status,
            icon,
            item_is_menu: boolean(properties, "ItemIsMenu", false),
            menu: menu_path.as_ref().map(|_| TrayMenu::default()),
            menu_path: menu_path.clone(),
            menu_error: None,
        },
        menu_path,
        menu_loaded: false,
        menu_revision: 0,
    })
}

fn read_icon(properties: &Properties, prefix: &str) -> TrayIcon {
    let pixmaps = properties
        .get(&format!("{prefix}Pixmap"))
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Pixmaps::try_from(value).ok());
    TrayIcon {
        revision: 0,
        name: string(properties, &format!("{prefix}Name")),
        theme_path: string(properties, "IconThemePath"),
        pixmap: pixmaps.and_then(select_pixmap),
    }
}

pub(super) fn select_pixmap(pixmaps: Pixmaps) -> Option<TrayPixmap> {
    // Prefer the closest image at or above two-times our 24px logical icon.
    // Reject invalid geometry before any allocation or multiplication.
    let (width, height, argb) = pixmaps
        .into_iter()
        .filter(|(width, height, bytes)| {
            (1..=512).contains(width)
                && (1..=512).contains(height)
                && bytes.len() == *width as usize * *height as usize * 4
        })
        .min_by_key(|(width, height, _)| {
            let size = (*width).min(*height);
            (size < 48, size.abs_diff(48))
        })?;
    let rgba = argb
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|argb| [argb[1], argb[2], argb[3], argb[0]])
        .collect();
    Some(TrayPixmap {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

pub(super) fn string(properties: &Properties, key: &str) -> Option<String> {
    properties
        .get(key)
        .and_then(|value| <&str>::try_from(value).ok())
        .filter(|value| !value.is_empty())
        .map(limit)
}

pub(super) fn boolean(properties: &Properties, key: &str, default: bool) -> bool {
    properties
        .get(key)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(default)
}

pub(super) fn limit(text: &str) -> String {
    text.chars().take(4096).collect()
}
pub(super) fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
