//! Register native file URLs once per existing desktop discovery update.
use crate::desktop_items::{DesktopItem, DesktopItemKind};
use hearthspace_common::desktop_files::file_uri;

pub(super) fn commands(items: &[DesktopItem]) -> Vec<String> {
    let entries: Vec<_> = items
        .iter()
        .filter_map(|item| {
            let path = match &item.kind {
                DesktopItemKind::File { path } | DesktopItemKind::Folder { path } => path,
                DesktopItemKind::App { .. } => return None,
            };
            file_uri(path)
                .ok()
                .map(|uri| serde_json::json!({"id":item.id,"uri":uri}))
        })
        .collect();
    let mut commands = Vec::new();
    let mut offset = 0;
    while offset < entries.len() {
        let build = |end| {
            format!(
                "desktop-file-register {}",
                serde_json::json!({"offset":offset,"entries":&entries[offset..end]})
            )
        };
        let mut end = offset + 1;
        let mut command = build(end);
        while end < entries.len() {
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
    if entries.is_empty() {
        commands.push("desktop-file-register {\"offset\":0,\"entries\":[]}".into());
    }
    commands
}
