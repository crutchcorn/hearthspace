//! App identity belongs to the complete catalog, not the capped tile surfaces.
use crate::desktop_items::{DesktopItem, DesktopItemKind};
use hearthspace_desktop::app_catalog::{AppCatalog, DesktopApp};

pub(super) fn app<'a>(catalog: &'a AppCatalog, id: &str) -> Option<&'a DesktopApp> {
    catalog.app_by_id(id.strip_prefix("app:")?)
}

pub(super) fn resolve<'a>(
    id: &str,
    catalog: &AppCatalog,
    known: impl IntoIterator<Item = &'a DesktopItem>,
) -> Option<DesktopItem> {
    if id.starts_with("app:") {
        let app = app(catalog, id)?;
        return Some(DesktopItem {
            id: id.into(),
            title: app.name.clone(),
            kind: DesktopItemKind::App {
                app_id: app.id.clone(),
                icon: app.icon.clone(),
            },
        });
    }
    // Catalog membership confers no file-operation authority. Removed apps
    // also cannot fall back to an obsolete shortcut left in the tile list.
    known
        .into_iter()
        .find(|item| item.id == id && !matches!(item.kind, DesktopItemKind::App { .. }))
        .cloned()
}
