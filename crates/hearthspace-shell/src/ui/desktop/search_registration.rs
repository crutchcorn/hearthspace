//! Search indexes names and object kinds, never file contents or private paths.
use crate::desktop_items::{DesktopItem, DesktopItemKind};
use serde_json::json;

pub(super) fn commands(items: &[DesktopItem]) -> Vec<String> {
    let entries: Vec<_> = items
        .iter()
        .map(|item| {
            let kind = match &item.kind {
                DesktopItemKind::App { .. } => "app application shortcut",
                DesktopItemKind::Folder { .. } => "folder directory",
                DesktopItemKind::File { .. } => "file",
            };
            // Names displayed by the shell can be unusually long application titles.
            // Bound metadata in bytes while retaining valid UTF-8 and useful prefixes.
            let mut title: String = item
                .title
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            if title.len() > 256 {
                let end = title.floor_char_boundary(256);
                title.truncate(end);
            }
            json!({"id":item.id,"title":title,"search_text":format!("{title} {kind}")})
        })
        .collect();
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < entries.len() {
        let build = |end| {
            format!(
                "canvas-search-desktop-register {}",
                json!({"offset":offset,"entries":&entries[offset..end]})
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
        result.push(command);
        offset = end;
    }
    if entries.is_empty() {
        result.push("canvas-search-desktop-register {\"offset\":0,\"entries\":[]}".into());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folders_are_searchable_without_disclosing_paths_and_batches_stay_bounded() {
        let items: Vec<_> = (0..20)
            .map(|index| DesktopItem {
                id: format!("file:{index:03}"),
                title: "🖥".repeat(600),
                kind: DesktopItemKind::Folder {
                    path: format!("/private/Desktop/{index}").into(),
                },
            })
            .collect();
        let commands = commands(&items);
        let mut count = 0;
        for command in commands {
            assert!(command.len() < hearthspace_ipc::transport::MAX_REQUEST_BYTES);
            assert!(!command.contains("/private"));
            let value: serde_json::Value =
                serde_json::from_str(command.split_once(' ').unwrap().1).unwrap();
            assert_eq!(value["offset"], count);
            for entry in value["entries"].as_array().unwrap() {
                assert!(
                    entry["search_text"]
                        .as_str()
                        .unwrap()
                        .ends_with("folder directory")
                );
                count += 1;
            }
        }
        assert_eq!(count, items.len());
    }
}
