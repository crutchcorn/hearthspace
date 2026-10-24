/// A unique bus connection, object path, and host-connection generation.
/// Reconnection cannot accidentally route an old menu click to a new process.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemKey {
    pub owner: String,
    pub path: String,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrayStatus {
    Passive,
    #[default]
    Active,
    NeedsAttention,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayIcon {
    /// Invalidates named-file caches when an app emits an icon-change signal.
    pub revision: u64,
    pub name: Option<String>,
    pub theme_path: Option<String>,
    pub pixmap: Option<TrayPixmap>,
}

pub use hearthspace_desktop::artwork::RasterImage as TrayPixmap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayItem {
    pub key: ItemKey,
    pub title: String,
    pub tooltip: String,
    pub status: TrayStatus,
    pub icon: TrayIcon,
    pub item_is_menu: bool,
    /// Some even before loading if the application exports a native menu.
    pub menu: Option<TrayMenu>,
    pub menu_path: Option<String>,
    pub menu_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayMenu {
    pub items: Vec<TrayMenuItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub separator: bool,
    pub toggle: Option<TrayToggle>,
    pub submenu: bool,
    pub children: Vec<TrayMenuItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayToggle {
    Check(bool),
    Radio(bool),
    Indeterminate,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TraySnapshot {
    pub items: Vec<TrayItem>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum TrayCommand {
    Activate {
        key: ItemKey,
        x: i32,
        y: i32,
    },
    Secondary {
        key: ItemKey,
        x: i32,
        y: i32,
    },
    Context {
        key: ItemKey,
        x: i32,
        y: i32,
    },
    Scroll {
        key: ItemKey,
        delta: i32,
        horizontal: bool,
    },
    OpenMenu {
        key: ItemKey,
        menu_path: String,
        id: i32,
    },
    MenuClick {
        key: ItemKey,
        menu_path: String,
        id: i32,
    },
}

impl TrayCommand {
    pub(super) fn key(&self) -> &ItemKey {
        match self {
            Self::Activate { key, .. }
            | Self::Secondary { key, .. }
            | Self::Context { key, .. }
            | Self::Scroll { key, .. }
            | Self::OpenMenu { key, .. }
            | Self::MenuClick { key, .. } => key,
        }
    }
}
