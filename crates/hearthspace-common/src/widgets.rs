//! Independently owned desktop widget copies; placement stays with the compositor.
mod identity;
mod preferences;
mod store;

pub use identity::InstanceId;
pub use preferences::{MAX_DATE_FORMAT_BYTES, Preferences};
pub use store::{MAX_INSTANCES, MAX_SNAPSHOT_BYTES, WidgetInstance, WidgetStore};

/// The complete catalog of addable tools; membership never creates an instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Notifications,
    Clock,
    Wifi,
    Sound,
    Brightness,
    Battery,
    ApplicationTray,
    Power,
    CanvasTools,
    AppDock,
    AppLauncher,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidField(&'static str),
    UnknownInstance(InstanceId),
    TooManyInstances,
    InputTooLarge,
    IdentityExhausted,
    InvalidSnapshot(&'static str),
    UnsupportedSchema(u32),
    InvalidJson(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for Error {}
