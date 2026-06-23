use std::time::Duration;

pub const WAYLAND_DISPLAY_NAME: &str = "wayland-hearthspace-0";
pub const DEFAULT_APP: &str = "foot";

pub const KEYBOARD_REPEAT_DELAY_MS: i32 = 600;
pub const KEYBOARD_REPEAT_RATE: i32 = 25;

pub const CONTROL_BAR_HEIGHT: i32 = 48;
pub const TITLE_BAR_HEIGHT: i32 = 30;
pub const MIN_WINDOW_WIDTH: i32 = 260;

pub const SPAWN_OFFSET_STEP: i32 = 36;
pub const SPAWN_OFFSET_WRAP: i32 = 180;

pub const MIN_ZOOM: f64 = 0.5;
pub const MAX_ZOOM: f64 = 2.0;
pub const ZOOM_STEP: f64 = 1.25;
pub const VIEWPORT_ANIMATION_DURATION: Duration = Duration::from_millis(180);

pub const IDLE_SLEEP: Duration = Duration::from_millis(1);

pub const BUTTON_Y: i32 = 8;
pub const BUTTON_HEIGHT: i32 = 32;
pub const BUTTON_GAP: i32 = 8;
