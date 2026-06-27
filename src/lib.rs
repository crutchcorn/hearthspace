pub mod accessibility;
pub mod compositor;
pub mod config;
pub mod geometry;
pub mod shell;
#[cfg(feature = "test-apps")]
pub mod test_apps;

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub scroll_zooms_without_super: bool,
    pub headless: bool,
    pub headless_output_size: Option<(i32, i32)>,
    pub start_shell: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            scroll_zooms_without_super: false,
            headless: false,
            headless_output_size: None,
            start_shell: true,
        }
    }
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
