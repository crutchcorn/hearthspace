//! The shell's desktop menu is anchored by the compositor that owns its input.
use serde::{Deserialize, Serialize};

const APP_ID: &str = "dev.hearthspace.desktop-context";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct DesktopContext {
    pub generation: u64,
    pub ids: Vec<String>,
    pub output: Option<String>,
    pub max_width: i32,
    pub max_height: i32,
}

pub fn app_id(generation: u64, output: Option<&str>) -> String {
    let base = output.map_or_else(
        || APP_ID.to_owned(),
        |output| super::shell_app_id_for_output(APP_ID, output),
    );
    format!("{base}.context-{generation}")
}

pub fn identity(app_id: &str) -> Option<(u64, Option<String>)> {
    let app_id = if let Some((parent, offset)) = app_id.rsplit_once(".submenu-") {
        offset
            .parse::<u32>()
            .ok()
            .filter(|offset| *offset <= 16_384)?;
        parent
    } else {
        app_id
    };
    let (base, generation) = app_id.rsplit_once(".context-")?;
    let generation = generation.parse::<u64>().ok().filter(|n| *n > 0)?;
    Some((
        generation,
        super::shell_output_name_from_app_id(APP_ID, base)?,
    ))
}

/// A separate child surface, aligned with the visible parent row in logical pixels.
pub fn submenu_app_id(generation: u64, output: Option<&str>, row_y: u32) -> String {
    format!(
        "{}.submenu-{}",
        app_id(generation, output),
        row_y.min(16_384)
    )
}

pub fn submenu_offset(app_id: &str) -> Option<i32> {
    identity(app_id)?;
    app_id.rsplit_once(".submenu-")?.1.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_identity_is_generation_and_output_scoped() {
        let id = app_id(12, Some("DP-1"));
        assert_eq!(identity(&id), Some((12, Some("DP-1".into()))));
        assert_eq!(identity(&app_id(0, None)), None);
        assert_eq!(identity("dev.hearthspace.desktop-context"), None);
        assert_eq!(
            super::super::shell_panel_output_name_from_app_id(&id),
            Some(Some("DP-1".into()))
        );
    }
}

#[cfg(test)]
mod submenu_tests {
    use super::*;
    #[test]
    fn child_identity_keeps_generation_and_output_with_a_bounded_row_anchor() {
        let child = submenu_app_id(12, Some("DP-1"), 34);
        assert_eq!(identity(&child), Some((12, Some("DP-1".into()))));
        assert_eq!(submenu_offset(&child), Some(34));
        assert_eq!(submenu_offset(&app_id(12, Some("DP-1"))), None);
        assert_eq!(
            identity("dev.hearthspace.desktop-context.context-12.submenu-NaN"),
            None
        );
        assert_eq!(
            identity("dev.hearthspace.desktop-context.context-12.submenu-99999"),
            None
        );
    }
}
