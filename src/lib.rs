pub mod accessibility;
pub mod compositor;
pub mod config;
pub mod geometry;
pub mod shell;
#[cfg(feature = "test-apps")]
pub mod test_apps;

#[derive(Debug, Clone, Copy)]
pub enum BackendSelection {
    Auto,
    Winit,
    Udev,
    Headless,
}

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub scroll_zooms_without_super: bool,
    pub backend: BackendSelection,
    pub headless_output_size: Option<(i32, i32)>,
    pub headless_output_scale: Option<i32>,
    pub start_shell: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            scroll_zooms_without_super: false,
            backend: BackendSelection::Auto,
            headless_output_size: None,
            headless_output_scale: None,
            start_shell: true,
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_with_options(RunOptions::default())
}

pub fn run_with_options(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    match selected_backend(options.backend) {
        BackendSelection::Headless => compositor::run_headless(options),
        BackendSelection::Winit => run_winit_backend(options),
        BackendSelection::Udev => run_udev_backend(options),
        BackendSelection::Auto => unreachable!("selected_backend never returns Auto"),
    }
}

fn selected_backend(selection: BackendSelection) -> BackendSelection {
    match selection {
        BackendSelection::Auto if std::env::var_os("WAYLAND_DISPLAY").is_some() => {
            BackendSelection::Winit
        }
        BackendSelection::Auto if std::env::var_os("DISPLAY").is_some() => BackendSelection::Winit,
        BackendSelection::Auto => BackendSelection::Udev,
        explicit => explicit,
    }
}

#[cfg(feature = "winit")]
fn run_winit_backend(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    compositor::run_winit(options)
}

#[cfg(not(feature = "winit"))]
fn run_winit_backend(_options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    Err("winit backend support is not enabled; rebuild with `--features winit`".into())
}

#[cfg(feature = "udev")]
fn run_udev_backend(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    compositor::run_udev(options)
}

#[cfg(not(feature = "udev"))]
fn run_udev_backend(_options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    Err("udev backend support is not enabled; rebuild with `--features udev`".into())
}
