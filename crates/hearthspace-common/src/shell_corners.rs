//! Stable identities shared by the shell and its output-pinned compositor layer.
use super::{CONTROL_BAR_HEIGHT, shell_app_id_for_output, shell_output_name_from_app_id};

pub const SHELL_CORNER_MARGIN: i32 = 24;
pub const SHELL_CORNER_CLEARANCE: i32 = CONTROL_BAR_HEIGHT + SHELL_CORNER_MARGIN + 12;
/// Logical radius shared by the shell frame and its compositor backdrop.
pub const SHELL_CORNER_RADIUS: f64 = 20.0;
/// Space for the notification bell, status text, and their shared button padding.
pub const NOTIFICATION_CORNER_WIDTH: i32 = 110;
/// Space for selection, search, window groups, and the canvas map.
pub const CANVAS_CORNER_WIDTH: i32 = 198;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellCorner {
    Notifications,
    System,
    Canvas,
    Apps,
}

impl ShellCorner {
    const ALL: [Self; 4] = [Self::Notifications, Self::System, Self::Canvas, Self::Apps];

    fn app_id(self) -> &'static str {
        match self {
            Self::Notifications => "dev.hearthspace.corner-notifications",
            Self::System => "dev.hearthspace.corner-system",
            Self::Canvas => "dev.hearthspace.corner-canvas",
            Self::Apps => "dev.hearthspace.corner-apps",
        }
    }

    pub fn is_bottom(self) -> bool {
        matches!(self, Self::Canvas | Self::Apps)
    }

    pub fn is_right(self) -> bool {
        matches!(self, Self::System | Self::Apps)
    }
}

pub fn shell_corner_app_id(corner: ShellCorner, output_name: Option<&str>) -> String {
    output_name
        .map(|name| shell_app_id_for_output(corner.app_id(), name))
        .unwrap_or_else(|| corner.app_id().to_string())
}

pub fn shell_corner_from_app_id(app_id: &str) -> Option<ShellCorner> {
    ShellCorner::ALL
        .into_iter()
        .find(|corner| shell_output_name_from_app_id(corner.app_id(), app_id).is_some())
}

pub(super) fn output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    let corner = shell_corner_from_app_id(app_id)?;
    shell_output_name_from_app_id(corner.app_id(), app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_round_trip_output_identity_and_keep_legacy_shell_ids_distinct() {
        for corner in ShellCorner::ALL {
            for output in [None, Some("card0/DP-1 🖥")] {
                let id = shell_corner_app_id(corner, output);
                assert_eq!(shell_corner_from_app_id(&id), Some(corner));
                assert_eq!(
                    crate::shell_bar_output_name_from_app_id(&id),
                    Some(output.map(str::to_string))
                );
                assert_eq!(crate::shell_panel_output_name_from_app_id(&id), None);
            }
            assert_eq!(
                shell_corner_from_app_id(&format!("{}.output-ff", corner.app_id())),
                None
            );
        }
        assert_eq!(shell_corner_from_app_id(crate::SHELL_BAR_APP_ID), None);
        assert_eq!(shell_corner_from_app_id(crate::LAUNCHER_APP_ID), None);
    }
}
