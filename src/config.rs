use std::time::Duration;

pub const WAYLAND_DISPLAY_NAME: &str = "wayland-99";
pub const GTK_TEST_APP_FLAG: &str = "--gtk-test-app";
pub const SHELL_FLAG: &str = "--shell";
pub const GTK_TEST_APP_ID: &str = "dev.hearthspace.A11yTest";
pub const GTK_TEST_APP_TITLE: &str = "Hearthspace Research Demo";
pub const SHELL_BAR_APP_ID: &str = "dev.hearthspace.shell-bar";
pub const SHELL_COMMAND_SOCKET_NAME: &str = "hearthspace-shell.sock";
pub const SHELL_COMMAND_SOCKET_ENV: &str = "HEARTHSPACE_COMMAND_SOCKET";
pub const GTK_CLIENT_CONFIG_DIR_NAME: &str = "hearthspace-gtk-client-config";
pub const SCROLL_ZOOMS_FLAG: &str = "--scroll-zooms";

pub const KEYBOARD_REPEAT_DELAY_MS: i32 = 600;
pub const KEYBOARD_REPEAT_RATE: i32 = 25;

pub const CONTROL_BAR_HEIGHT: i32 = 48;
pub const TITLE_BAR_HEIGHT: i32 = 30;
pub const CLOSE_BUTTON_SIZE: i32 = 18;
pub const CLOSE_BUTTON_MARGIN: i32 = 6;
pub const MIN_WINDOW_WIDTH: i32 = 260;
pub const MIN_WINDOW_HEIGHT: i32 = 120;
/// Thickness (canvas pixels) of the invisible interactive resize border drawn
/// around server-side decorated windows.
pub const RESIZE_BORDER_THICKNESS: i32 = 8;

pub const SPAWN_OFFSET_STEP: i32 = 36;
pub const SPAWN_OFFSET_WRAP: i32 = 180;

pub const MIN_ZOOM: f64 = 0.5;
pub const MAX_ZOOM: f64 = 2.0;
pub const ZOOM_STEP: f64 = 1.25;
pub const SCROLL_ZOOM_SENSITIVITY: f64 = 0.005;
pub const WHEEL_SCROLL_PIXEL_EQUIVALENT: f64 = 40.0;
pub const VIEWPORT_ANIMATION_DURATION: Duration = Duration::from_millis(180);
pub const WINDOW_IDLE_THRESHOLDS: [Duration; 3] = [
    Duration::from_secs(5 * 60),
    Duration::from_secs(10 * 60),
    Duration::from_secs(30 * 60),
];

pub const ANIMATION_FRAME_INTERVAL: Duration = Duration::from_millis(16);
