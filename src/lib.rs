pub mod accessibility;
pub mod compositor;
pub mod config;
pub mod geometry;
pub mod shell;
#[cfg(feature = "test-apps")]
pub mod test_apps;

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    pub scroll_zooms_without_super: bool,
    pub headless: bool,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_with_options(RunOptions::default())
}

pub fn run_with_options(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    if options.headless {
        return compositor::run_headless(options);
    }

    compositor::run_winit(options)
}
