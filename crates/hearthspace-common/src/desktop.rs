//! Canvas-local shell surfaces. The transparent margin keeps focus rings intact.

pub const DESKTOP_ITEM_WIDTH: i32 = 88;
pub const DESKTOP_ITEM_HEIGHT: i32 = 96;
pub const DESKTOP_ITEM_MARGIN: i32 = 6;
pub const DESKTOP_SURFACE_WIDTH: i32 = DESKTOP_ITEM_WIDTH + 2 * DESKTOP_ITEM_MARGIN;
pub const DESKTOP_SURFACE_HEIGHT: i32 = DESKTOP_ITEM_HEIGHT + 2 * DESKTOP_ITEM_MARGIN;
pub const DESKTOP_ITEM_APP_ID: &str = "dev.hearthspace.desktop-item";
pub const MAX_DESKTOP_ITEMS: usize = 4096;

pub fn desktop_item_app_id(id: &str) -> String {
    format!(
        "{DESKTOP_ITEM_APP_ID}.item-{}",
        super::hex_encode(id.as_bytes())
    )
}

pub fn desktop_item_id_from_app_id(app_id: &str) -> Option<String> {
    let encoded = app_id
        .strip_prefix(DESKTOP_ITEM_APP_ID)?
        .strip_prefix(".item-")?;
    // Bound private protocol payloads as well as accidental lookalike app IDs.
    if encoded.is_empty() || encoded.len() > 2048 {
        return None;
    }
    let id = String::from_utf8(super::hex_decode(encoded)?).ok()?;
    (!id.chars().any(char::is_control)).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_identity_is_distinct_from_other_shell_surfaces() {
        for id in [
            "app:dev.hearthspace.Settings.desktop",
            "file:616263",
            "app:图像.desktop",
        ] {
            let app_id = desktop_item_app_id(id);
            assert_eq!(desktop_item_id_from_app_id(&app_id).as_deref(), Some(id));
            assert_eq!(crate::shell_panel_output_name_from_app_id(&app_id), None);
            assert_eq!(crate::shell_bar_output_name_from_app_id(&app_id), None);
        }
    }

    #[test]
    fn malformed_desktop_identities_are_not_shell_roles() {
        for app_id in [
            DESKTOP_ITEM_APP_ID,
            "dev.hearthspace.desktop-item.item-",
            "dev.hearthspace.desktop-item.item-ff",
            "dev.hearthspace.desktop-item.item-0a",
            "dev.hearthspace.desktop-item.item-1",
            "dev.hearthspace.desktop-item.item-zz",
        ] {
            assert_eq!(desktop_item_id_from_app_id(app_id), None);
        }
    }
}
