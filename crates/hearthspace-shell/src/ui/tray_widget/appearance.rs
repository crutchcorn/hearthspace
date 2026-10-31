//! Shared appearance and accessibility state for native shell menu controls.
use crate::tray::TrayToggle;
use xilem::masonry::peniko::ImageBrush;

#[derive(Clone, PartialEq)]
pub(in crate::ui) struct Appearance {
    pub name: String,
    pub description: String,
    pub image: Option<ImageBrush>,
    pub icon: bool,
    pub scroll_in_parent: bool,
    pub attention: bool,
    pub enabled: bool,
    pub selected: bool,
    pub autofocus: bool,
    /// Finish the opening gesture before handing focus to another control.
    pub activate_on_key_release: bool,
    pub menu: bool,
    pub separator: bool,
    pub submenu: bool,
    pub expanded: Option<bool>,
    pub toggle: Option<TrayToggle>,
    pub wrap_label: bool,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            image: None,
            icon: false,
            scroll_in_parent: false,
            attention: false,
            enabled: true,
            selected: false,
            autofocus: false,
            activate_on_key_release: false,
            menu: false,
            separator: false,
            submenu: false,
            expanded: None,
            toggle: None,
            wrap_label: false,
        }
    }
}
