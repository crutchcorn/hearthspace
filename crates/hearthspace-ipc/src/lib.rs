pub mod command;
pub mod frame_layout;
pub mod outputs;
pub mod shortcuts;
pub mod transport;
pub mod windows;

mod display_layout;
mod model;
pub use display_layout::{
    client_set_display_layout, client_set_display_layout_at, display_layout_request,
    validate_display_layout, validate_monitor_configs,
};
pub use hearthspace_common::{AccentColor, Appearance, ColorMode};
pub use model::{BackgroundMode, MonitorConfig, Settings, SettingsRequest};

mod settings_client;
pub mod settings_snapshot;
mod settings_wire;
pub use settings_client::*;
#[cfg(test)]
use settings_wire::encode_background_path;
use settings_wire::parse_settings_response;
pub use settings_wire::{error_response, parse_settings_request, settings_response};

#[cfg(test)]
mod tests;
