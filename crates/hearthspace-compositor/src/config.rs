use std::time::Duration;

pub use hearthspace_common::{
    CONTROL_BAR_HEIGHT, HEARTHSPACE_WAYLAND_DISPLAY_ENV, LAUNCHER_APP_ID, LOG_FILE_ENV,
    NO_SHELL_FLAG, SHELL_BAR_APP_ID, SHELL_COMMAND_SOCKET_ENV, SHELL_COMMAND_SOCKET_NAME,
    SHELL_OUTPUT_WATCHER_APP_ID, WAYLAND_DISPLAY_ENV, WAYLAND_DISPLAY_NAME, wayland_display_name,
};
pub const GTK_TEST_APP_FLAG: &str = "--gtk-test-app";
pub const SHELL_FLAG: &str = "--shell";
pub const HEADLESS_FLAG: &str = "--headless";
pub const TTY_FLAG: &str = "--tty";
pub const WINIT_FLAG: &str = "--winit";
pub const REMOTE_DESKTOP_FLAG: &str = "--remote-desktop";
pub const SCREEN_CAST_FLAG: &str = "--screen-cast";
pub const GTK_TEST_APP_ID: &str = "dev.hearthspace.A11yTest";
pub const GTK_TEST_APP_TITLE: &str = "Hearthspace Research Demo";
pub const GTK_CLIENT_CONFIG_DIR_NAME: &str = "hearthspace-gtk-client-config";
pub const SCROLL_ZOOMS_FLAG: &str = "--scroll-zooms";
pub const HEADLESS_SIZE_FLAG: &str = "--headless-size";
pub const HEADLESS_SCALE_FLAG: &str = "--headless-scale";
pub const EXIT_AFTER_MS_FLAG: &str = "--exit-after-ms";
pub const HEADLESS_OUTPUT_WIDTH: i32 = 1280;
pub const HEADLESS_OUTPUT_HEIGHT: i32 = 720;

pub const KEYBOARD_REPEAT_DELAY_MS: i32 = 600;
pub const KEYBOARD_REPEAT_RATE: i32 = 25;

pub const TITLE_BAR_HEIGHT: i32 = 44;
pub const CLOSE_BUTTON_SIZE: i32 = 32;
pub const CLOSE_BUTTON_MARGIN: i32 = 6;
pub const TITLE_BAR_BUTTON_GAP: i32 = 4;
pub const MIN_WINDOW_WIDTH: i32 = 260;
pub const MIN_WINDOW_HEIGHT: i32 = 120;
/// The interactive resize handle is a band centered on each window edge: it
/// reaches `RESIZE_HANDLE_OUTSET` pixels outside the edge and
/// `RESIZE_HANDLE_INSET` pixels inside it, so the visible edge sits in the
/// middle of a comfortable grab zone (rather than only just outside the window).
pub const RESIZE_HANDLE_OUTSET: i32 = 8;
pub const RESIZE_HANDLE_INSET: i32 = 8;

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
