use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedValue, Type, Value},
};

use super::{
    TrayMenu, TrayMenuItem, TrayToggle,
    properties::{Properties, boolean, error, string},
};

const MAX_DEPTH: usize = 8;
const MAX_NODES: usize = 256;

#[derive(Debug, Deserialize, Serialize, Type)]
pub(super) struct Layout {
    pub id: i32,
    pub properties: Properties,
    pub children: Vec<OwnedValue>,
}

pub(super) async fn load(
    connection: &Connection,
    owner: &str,
    path: &str,
    about_to_show: Option<i32>,
) -> Result<TrayMenu, String> {
    let proxy = Proxy::new(connection, owner, path, "com.canonical.dbusmenu")
        .await
        .map_err(error)?;
    if let Some(id) = about_to_show {
        // Some implementations omit this optional preparation method. Still
        // load the existing menu, including when the app reports no changes.
        let _: Result<bool, _> = proxy.call("AboutToShow", &(id,)).await;
    }
    let properties = [
        "label",
        "enabled",
        "visible",
        "type",
        "toggle-type",
        "toggle-state",
        "children-display",
    ];
    let (_, layout): (u32, Layout) = proxy
        .call(
            "GetLayout",
            &(0_i32, MAX_DEPTH as i32, properties.as_slice()),
        )
        .await
        .map_err(error)?;
    parse(layout)
}

pub(super) fn parse(layout: Layout) -> Result<TrayMenu, String> {
    let mut ids = HashSet::new();
    let items = children(layout.children, 0, &mut ids)?;
    Ok(TrayMenu { items })
}

fn children(
    values: Vec<OwnedValue>,
    depth: usize,
    ids: &mut HashSet<i32>,
) -> Result<Vec<TrayMenuItem>, String> {
    if depth > MAX_DEPTH {
        return Err("Tray menu is too deeply nested".into());
    }
    values
        .into_iter()
        .map(|value| {
            let (id, properties, values): (i32, Properties, Vec<OwnedValue>) =
                value.try_into().map_err(error)?;
            if ids.len() >= MAX_NODES || id == 0 || !ids.insert(id) {
                return Err("Tray menu has too many items or duplicate IDs".into());
            }
            let children = children(values, depth + 1, ids)?;
            let state = properties
                .get("toggle-state")
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or(-1);
            let toggle = match (string(&properties, "toggle-type").as_deref(), state) {
                (Some("checkmark" | "radio"), -1) => Some(TrayToggle::Indeterminate),
                (Some("checkmark"), state) => Some(TrayToggle::Check(state == 1)),
                (Some("radio"), state) => Some(TrayToggle::Radio(state == 1)),
                _ => None,
            };
            Ok(TrayMenuItem {
                id,
                label: label(&string(&properties, "label").unwrap_or_default()),
                enabled: boolean(&properties, "enabled", true),
                visible: boolean(&properties, "visible", true),
                separator: string(&properties, "type").as_deref() == Some("separator"),
                submenu: string(&properties, "children-display").as_deref() == Some("submenu")
                    || !children.is_empty(),
                toggle,
                children,
            })
        })
        .collect()
}

fn label(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '_' {
            result.push(c);
        } else if chars.peek() == Some(&'_') {
            result.push('_');
            chars.next();
        }
    }
    result
}

pub(super) fn actionable(menu: &TrayMenu, id: i32, open: bool) -> bool {
    fn visit(items: &[TrayMenuItem], id: i32, open: bool) -> bool {
        items
            .iter()
            .filter(|item| item.enabled && item.visible && !item.separator)
            .any(|item| (item.id == id && item.submenu == open) || visit(&item.children, id, open))
    }
    open && id == 0 || visit(&menu.items, id, open)
}

pub(super) async fn click(
    connection: &Connection,
    owner: &str,
    path: &str,
    id: i32,
) -> Result<(), String> {
    let proxy = Proxy::new(connection, owner, path, "com.canonical.dbusmenu")
        .await
        .map_err(error)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32;
    proxy
        .call::<_, _, ()>("Event", &(id, "clicked", Value::I32(0), timestamp))
        .await
        .map_err(error)
}
